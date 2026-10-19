//! Density matrices stored in a `.fchk`, and the natural orbitals they
//! define (item 5 of the improvement backlog, 2026-10-19).
//!
//! A `.fchk` holds the one-particle density matrix of its calculation in
//! the AO basis ("Total SCF Density", plus "Spin SCF Density" for an open
//! shell), as a lower triangle row by row. Gaussian writes a correlated
//! density in a block of its own ("Total MP2 Density", "Total CI
//! Density", "Total CC Density", with `density=current`). APOST-3D's
//! `utils/apost3d.py` writes the 1-RDM of whatever wavefunction it is
//! given into "Total SCF Density", so for a CASSCF, FCI, CISD or CCSD file
//! that block holds the *correlated* density despite its name; line 2 of
//! such a file names the method (CASSCF, also for FCI and CISD, written as
//! a CAS with every orbital active; CCSD), and its MO coefficients are not
//! natural orbitals (their "orbital energies" are zeros).
//!
//! Natural orbitals diagonalize the density matrix: rho = sum_i n_i
//! |phi_i|^2, exactly the same density, and as cheap to evaluate as the
//! occupied orbitals (evaluating sum_mu,nu P_mu,nu chi_mu chi_nu directly
//! costs the square of the basis size at every grid point). They are
//! found without the overlap matrix (which a `.fchk` doesn't store): the
//! file's MO coefficients C span the basis, so with C's left inverse
//! L = (C^T C)^-1 C^T the density in the MO basis is P_MO = L P L^T (since
//! P = C P_MO C^T), and its eigenvectors U give the natural orbitals C U.
//! This holds with fewer MOs than basis functions (linear dependencies
//! removed) as long as the density lies in their span, which it does for
//! a density computed from them.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use faer::linalg::solvers::Solve;
use faer::{Mat, Side};

use crate::fchk::{parse_header, read_array_tokens, HeaderKind};
use crate::invalid_data;
use crate::wavefunction::MolecularOrbitals;

/// Which density matrices a `.fchk` holds, found without reading them.
#[derive(Debug, Clone, PartialEq)]
pub struct FchkDensities {
    /// The method named on line 2 ("RB3LYP", "RMP2-FC", "CASSCF", ...).
    pub method: String,
    /// The kinds of density, in file order: "SCF", "MP2", "CI", "CC", ...
    /// (the word between "Total" and "Density").
    pub kinds: Vec<String>,
    /// "Total SCF Density" holds a correlated 1-RDM: a CASSCF (also FCI
    /// and CISD) or CCSD file, by its method line or its CAS fields.
    pub correlated_scf: bool,
}

impl FchkDensities {
    /// Whether the density of `kind` is a correlated one.
    pub fn is_correlated(&self, kind: &str) -> bool {
        kind != "SCF" || self.correlated_scf
    }
}

/// A density matrix of a `.fchk`, lower triangle row by row: the total
/// density and, for an open shell, the spin density (alpha minus beta).
pub struct DensityMatrix {
    pub kind: String,
    pub total: Vec<f64>,
    pub spin: Option<Vec<f64>>,
}

/// The kind of a density block's label ("Total MP2 Density" -> "MP2"),
/// and whether it is the spin block.
fn density_kind(label: &str) -> Option<(String, bool)> {
    let rest = label.strip_suffix(" Density")?;
    if let Some(kind) = rest.strip_prefix("Total ") {
        Some((kind.to_string(), false))
    } else {
        rest.strip_prefix("Spin ").map(|kind| (kind.to_string(), true))
    }
}

/// How many lines an array of `count` values takes: 6 integers or 5 reals
/// or 12-character strings per line (strings may contain spaces, so they
/// can't be counted as tokens).
fn array_lines(line: &str, count: usize) -> usize {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let per_line = match tokens.iter().find(|t| matches!(**t, "I" | "R" | "C")) {
        Some(&"I") => 6,
        _ => 5,
    };
    count.div_ceil(per_line)
}

/// Skips `n` lines.
fn skip_lines(lines: &mut impl Iterator<Item = io::Result<String>>, n: usize) -> io::Result<()> {
    for _ in 0..n {
        lines.next().transpose()?;
    }
    Ok(())
}

/// Lists the density matrices of a `.fchk` (header lines only; the values
/// are skipped).
pub fn scan_fchk_densities(path: &Path) -> io::Result<FchkDensities> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let _title = lines.next().transpose()?;
    let method = lines.next().transpose()?.and_then(|l| l.split_whitespace().nth(1).map(str::to_string)).unwrap_or_default();
    let mut kinds = Vec::new();
    let mut cas_fields = false;
    while let Some(line) = lines.next() {
        let line = line?;
        let Some(header) = parse_header(&line) else { continue };
        if header.label.starts_with("Number of CAS") {
            cas_fields = true;
        }
        if let HeaderKind::Array(count) = header.kind {
            if let Some((kind, false)) = density_kind(&header.label) {
                kinds.push(kind);
            }
            skip_lines(&mut lines, array_lines(&line, count))?;
        }
    }
    let correlated_scf = cas_fields || matches!(method.as_str(), "CASSCF" | "CCSD");
    Ok(FchkDensities { method, kinds, correlated_scf })
}

/// Reads the density matrix of `kind` ("SCF", "MP2", ...) of a `.fchk`.
pub fn read_fchk_density(path: &Path, kind: &str) -> io::Result<DensityMatrix> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let (mut total, mut spin) = (None, None);
    while let Some(line) = lines.next() {
        let line = line?;
        let Some(header) = parse_header(&line) else { continue };
        let HeaderKind::Array(count) = header.kind else { continue };
        match density_kind(&header.label) {
            Some((k, is_spin)) if k == kind => {
                let tokens = read_array_tokens(&mut lines, count)?;
                let values: Vec<f64> =
                    tokens.iter().map(|t| t.parse().map_err(|_| invalid_data(format!("malformed density value '{t}' in .fchk")))).collect::<io::Result<_>>()?;
                if is_spin {
                    spin = Some(values);
                } else {
                    total = Some(values);
                }
            }
            _ => skip_lines(&mut lines, array_lines(&line, count))?,
        }
    }
    let total = total.ok_or_else(|| invalid_data(format!("fchk has no 'Total {kind} Density'")))?;
    Ok(DensityMatrix { kind: kind.to_string(), total, spin })
}

/// The orbitals that diagonalize the density matrix `lower` (lower
/// triangle row by row, in the basis of `mos`), with their occupations as
/// weights, largest first: rho = sum_i n_i |phi_i|^2. For a spin density
/// the occupations are signed (alpha excess positive). `mos` are any MOs
/// of the same basis that span the density (the file's own).
pub fn natural_orbitals(mos: &MolecularOrbitals, lower: &[f64]) -> Result<MolecularOrbitals, String> {
    let (n, m) = (mos.num_basis_functions, mos.num_orbitals());
    if lower.len() != n * (n + 1) / 2 {
        return Err(format!("density matrix size {} doesn't match {n} basis functions", lower.len()));
    }
    if m == 0 {
        return Err("no orbitals".to_string());
    }
    let c = Mat::<f64>::from_fn(n, m, |i, k| mos.coefficients_for(k)[i]);
    let p = Mat::<f64>::from_fn(n, n, |i, j| {
        let (a, b) = if i >= j { (i, j) } else { (j, i) };
        lower[a * (a + 1) / 2 + b]
    });
    // L = (C^T C)^-1 C^T, the left inverse of C.
    let gram = c.transpose() * &c;
    let llt = gram.llt(Side::Lower).map_err(|_| "the orbitals are linearly dependent".to_string())?;
    let left_inverse = llt.solve(c.transpose());
    let p_mo = &left_inverse * &p * left_inverse.transpose();
    let p_mo = Mat::<f64>::from_fn(m, m, |i, j| 0.5 * (p_mo[(i, j)] + p_mo[(j, i)]));
    let eigen = p_mo.self_adjoint_eigen(Side::Lower).map_err(|_| "the density matrix could not be diagonalized".to_string())?;
    let vectors = &c * eigen.U();
    let values = eigen.S().column_vector();

    // Largest occupation first (the eigenvalues come smallest first).
    let order: Vec<usize> = (0..m).rev().collect();
    let occupations: Vec<f64> = order.iter().map(|&k| values[k]).collect();
    let coefficients: Vec<f64> = order.iter().flat_map(|&k| (0..n).map(move |i| (k, i))).map(|(k, i)| vectors[(i, k)]).collect();
    let num_occupied = occupations.iter().filter(|&&o| o > 0.5).count();
    Ok(MolecularOrbitals { num_basis_functions: n, orbital_energies: occupations.clone(), coefficients, num_occupied, occupations })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_block_labels() {
        assert_eq!(density_kind("Total SCF Density"), Some(("SCF".to_string(), false)));
        assert_eq!(density_kind("Spin MP2 Density"), Some(("MP2".to_string(), true)));
        assert_eq!(density_kind("Total CC Density"), Some(("CC".to_string(), false)));
        assert_eq!(density_kind("Alpha MO coefficients"), None);
    }

    /// Two orthonormal orbitals of a 2-function basis (not orthogonal
    /// functions: the left inverse handles that), occupied 1.5 and 0.5:
    /// the natural orbitals give them back.
    #[test]
    fn natural_orbitals_of_a_known_density() {
        let (a, b) = ([1.0, 0.0], [0.6, 0.8]);
        let mos = MolecularOrbitals {
            num_basis_functions: 2,
            orbital_energies: vec![0.0, 0.0],
            coefficients: [a, b].concat(),
            num_occupied: 1,
            occupations: vec![1.0, 1.0],
        };
        // P = 1.5 a a^T + 0.5 b b^T, lower triangle row by row.
        let p = |i: usize, j: usize| 1.5 * a[i] * a[j] + 0.5 * b[i] * b[j];
        let lower = [p(0, 0), p(1, 0), p(1, 1)];
        let no = natural_orbitals(&mos, &lower).unwrap();
        assert!((no.occupations[0] - 1.5).abs() < 1e-12 && (no.occupations[1] - 0.5).abs() < 1e-12, "{:?}", no.occupations);
        let first = no.coefficients_for(0);
        let sign = first[0].signum();
        assert!((first[0] * sign - a[0]).abs() < 1e-12 && (first[1] * sign - a[1]).abs() < 1e-12, "{first:?}");
        assert_eq!(no.num_occupied, 1);
    }
}
