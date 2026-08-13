//! Parses the basis-set and molecular-orbital sections of a `.fchk` file —
//! everything needed to evaluate an MO or density on an arbitrary 3D grid
//! ourselves (Phase 3), rather than requiring an external `cubegen`/
//! Multiwfn/APOST-3D step to produce a `.cube` file first.
//!
//! Deliberately a separate parse from `fchk::parse_fchk_geometry`, which
//! stops reading as soon as it has the geometry (near the top of the
//! file) for speed — the basis-set/MO blocks sit much further in and can
//! be large, so a caller that only wants geometry (the common case, e.g.
//! opening a `.fchk` to look at the molecule) shouldn't pay for reading
//! them. This parser doesn't need geometry at all, in fact: each shell's
//! own center is given directly (`Coordinates of each shell`), in the
//! same Bohr units as everything else here — no cross-referencing atom
//! positions required.
//!
//! Handles both restricted (closed-shell) and unrestricted (open-shell)
//! wavefunctions. A restricted calculation only ever writes an "Alpha"
//! MO block — beta is implicitly identical, so `Wavefunction::beta` is
//! `None`. An unrestricted one additionally writes "Beta Orbital
//! Energies"/"Beta MO coefficients" blocks with their own, genuinely
//! different, coefficients — detected by those blocks' *presence*, not
//! by multiplicity (a restricted-open-shell/ROHF file can have
//! multiplicity > 1 while still sharing one alpha=beta orbital set, so
//! multiplicity alone isn't a reliable signal).

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use glam::DVec3;

use crate::fchk::{parse_header, read_array_tokens, HeaderKind};

/// One contracted shell — already split out of Gaussian's combined "SP"
/// (`L`) shell type into separate pure-S and pure-P shells at parse time
/// (see `parse_fchk_wavefunction`), so everything downstream only ever
/// has to handle a single angular momentum per shell, never a composite.
pub struct Shell {
    /// 0=S, 1=P, 2=D, 3=F (magnitude only — `is_pure` carries the other
    /// bit of information the sign encodes in the raw `.fchk` shell-type
    /// codes).
    pub angular_momentum: u32,
    /// Spherical-harmonic (true) vs Cartesian (false) representation —
    /// meaningless for S/P (angular_momentum < 2), where both coincide.
    /// Determined from the *sign* of the shell's own raw type code, not
    /// the file's summary "Pure/Cartesian d/f shells" flags — confirmed
    /// against real reference files that those summary flags don't
    /// reliably match the per-shell codes (e.g. a 6-31G* file with
    /// "Pure/Cartesian d shells"=1 whose actual d shell is Cartesian).
    pub is_pure: bool,
    /// Bohr (native unit of the primitive exponents below).
    pub center: DVec3,
    pub primitive_exponents: Vec<f64>,
    /// One per primitive, already specific to *this* shell's own angular
    /// momentum (the S/P split for former "SP" shells already applied).
    pub contraction_coefficients: Vec<f64>,
}

pub struct BasisSet {
    pub shells: Vec<Shell>,
}

/// One spin channel's worth of MOs — either the alpha channel (always
/// present) or the beta channel (only for a genuinely unrestricted
/// calculation; identical to alpha and so omitted otherwise).
pub struct MolecularOrbitals {
    pub num_basis_functions: usize,
    pub orbital_energies: Vec<f64>,
    /// Flattened, MO-major: MO `m`'s coefficients are
    /// `coefficients[m * num_basis_functions .. (m+1) * num_basis_functions]`.
    pub coefficients: Vec<f64>,
    /// Electrons occupying *this* channel — the alpha count for the
    /// alpha channel, the beta count for the beta channel. Drives
    /// `homo_index`/`lumo_index` for this channel specifically (for a
    /// triplet, say, alpha and beta have different HOMOs).
    pub num_occupied: usize,
}

impl MolecularOrbitals {
    pub fn num_orbitals(&self) -> usize {
        if self.num_basis_functions == 0 {
            0
        } else {
            self.coefficients.len() / self.num_basis_functions
        }
    }

    pub fn coefficients_for(&self, mo_index: usize) -> &[f64] {
        let n = self.num_basis_functions;
        &self.coefficients[mo_index * n..(mo_index + 1) * n]
    }

    /// 1-based, matching how orbitals are conventionally numbered and
    /// how a UI would label them ("HOMO" = this index).
    pub fn homo_index(&self) -> usize {
        self.num_occupied
    }

    pub fn lumo_index(&self) -> usize {
        self.num_occupied + 1
    }
}

pub struct Wavefunction {
    pub basis: BasisSet,
    pub alpha: MolecularOrbitals,
    /// `Some` only for an unrestricted calculation — see the module doc
    /// comment on how that's detected.
    pub beta: Option<MolecularOrbitals>,
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn parse_f64_tokens(tokens: &[String], what: &str) -> io::Result<Vec<f64>> {
    tokens.iter().map(|t| t.parse::<f64>().map_err(|_| invalid_data(format!("malformed {what} value '{t}' in .fchk")))).collect()
}

fn parse_usize_tokens(tokens: &[String], what: &str) -> io::Result<Vec<usize>> {
    tokens.iter().map(|t| t.parse::<usize>().map_err(|_| invalid_data(format!("malformed {what} value '{t}' in .fchk")))).collect()
}

pub fn parse_fchk_wavefunction(path: &Path) -> io::Result<Wavefunction> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();

    let mut num_basis_functions: Option<usize> = None;
    let mut num_alpha_electrons: Option<usize> = None;
    let mut num_beta_electrons: Option<usize> = None;
    let mut shell_types: Option<Vec<i64>> = None;
    let mut primitives_per_shell: Option<Vec<usize>> = None;
    let mut primitive_exponents: Option<Vec<f64>> = None;
    let mut contraction_coefficients: Option<Vec<f64>> = None;
    let mut sp_contraction_coefficients: Option<Vec<f64>> = None;
    let mut shell_coordinates: Option<Vec<f64>> = None;
    let mut alpha_orbital_energies: Option<Vec<f64>> = None;
    let mut alpha_mo_coefficients: Option<Vec<f64>> = None;
    let mut beta_orbital_energies: Option<Vec<f64>> = None;
    let mut beta_mo_coefficients: Option<Vec<f64>> = None;

    while let Some(line) = lines.next() {
        let line = line?;
        let Some(header) = parse_header(&line) else { continue };

        match (header.label.as_str(), &header.kind) {
            ("Number of basis functions", HeaderKind::Scalar) => {
                num_basis_functions = line.split_whitespace().last().and_then(|t| t.parse().ok());
            }
            ("Number of alpha electrons", HeaderKind::Scalar) => {
                num_alpha_electrons = line.split_whitespace().last().and_then(|t| t.parse().ok());
            }
            ("Number of beta electrons", HeaderKind::Scalar) => {
                num_beta_electrons = line.split_whitespace().last().and_then(|t| t.parse().ok());
            }
            ("Shell types", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                let values: Vec<i64> = tokens
                    .iter()
                    .map(|t| t.parse::<i64>().map_err(|_| invalid_data(format!("malformed shell type '{t}'"))))
                    .collect::<io::Result<_>>()?;
                shell_types = Some(values);
            }
            ("Number of primitives per shell", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                primitives_per_shell = Some(parse_usize_tokens(&tokens, "primitives-per-shell")?);
            }
            ("Primitive exponents", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                primitive_exponents = Some(parse_f64_tokens(&tokens, "primitive exponent")?);
            }
            ("Contraction coefficients", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                contraction_coefficients = Some(parse_f64_tokens(&tokens, "contraction coefficient")?);
            }
            ("P(S=P) Contraction coefficients", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                sp_contraction_coefficients = Some(parse_f64_tokens(&tokens, "SP contraction coefficient")?);
            }
            ("Coordinates of each shell", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                shell_coordinates = Some(parse_f64_tokens(&tokens, "shell coordinate")?);
            }
            ("Alpha Orbital Energies", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                alpha_orbital_energies = Some(parse_f64_tokens(&tokens, "orbital energy")?);
            }
            ("Beta Orbital Energies", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                beta_orbital_energies = Some(parse_f64_tokens(&tokens, "beta orbital energy")?);
            }
            ("Alpha MO coefficients", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                alpha_mo_coefficients = Some(parse_f64_tokens(&tokens, "MO coefficient")?);
                // A restricted file (the common case) has nothing more
                // this parser needs after this block — stop here rather
                // than reading the rest of a potentially huge file. An
                // unrestricted one still has a "Beta MO coefficients"
                // block coming up next (confirmed field order: alpha
                // energies, beta energies, alpha coefficients, beta
                // coefficients) — `beta_orbital_energies` having already
                // been seen by this point is how we know to keep going.
                if beta_orbital_energies.is_none() {
                    break;
                }
            }
            ("Beta MO coefficients", HeaderKind::Array(count)) => {
                let tokens = read_array_tokens(&mut lines, *count)?;
                beta_mo_coefficients = Some(parse_f64_tokens(&tokens, "beta MO coefficient")?);
                break;
            }
            (_, HeaderKind::Array(count)) => {
                read_array_tokens(&mut lines, *count)?;
            }
            _ => {}
        }
    }

    let num_basis_functions = num_basis_functions.ok_or_else(|| invalid_data("fchk missing 'Number of basis functions'"))?;
    let num_alpha_electrons = num_alpha_electrons.ok_or_else(|| invalid_data("fchk missing 'Number of alpha electrons'"))?;
    let num_beta_electrons = num_beta_electrons.ok_or_else(|| invalid_data("fchk missing 'Number of beta electrons'"))?;
    let shell_types = shell_types.ok_or_else(|| invalid_data("fchk missing 'Shell types'"))?;
    let primitives_per_shell = primitives_per_shell.ok_or_else(|| invalid_data("fchk missing 'Number of primitives per shell'"))?;
    let primitive_exponents = primitive_exponents.ok_or_else(|| invalid_data("fchk missing 'Primitive exponents'"))?;
    let contraction_coefficients = contraction_coefficients.ok_or_else(|| invalid_data("fchk missing 'Contraction coefficients'"))?;
    let shell_coordinates = shell_coordinates.ok_or_else(|| invalid_data("fchk missing 'Coordinates of each shell'"))?;
    let alpha_orbital_energies = alpha_orbital_energies.ok_or_else(|| invalid_data("fchk missing 'Alpha Orbital Energies'"))?;
    let alpha_mo_coefficients = alpha_mo_coefficients.ok_or_else(|| invalid_data("fchk missing 'Alpha MO coefficients'"))?;

    if shell_coordinates.len() != shell_types.len() * 3 {
        return Err(invalid_data("shell coordinate count doesn't match 3x shell count"));
    }
    if alpha_mo_coefficients.len() != alpha_orbital_energies.len() * num_basis_functions {
        return Err(invalid_data("MO coefficient count doesn't match orbital-energy count x basis-function count"));
    }
    let beta = match (beta_orbital_energies, beta_mo_coefficients) {
        (Some(energies), Some(coefficients)) => {
            if coefficients.len() != energies.len() * num_basis_functions {
                return Err(invalid_data("beta MO coefficient count doesn't match beta orbital-energy count x basis-function count"));
            }
            Some(MolecularOrbitals { num_basis_functions, orbital_energies: energies, coefficients, num_occupied: num_beta_electrons })
        }
        _ => None,
    };

    // Walk the shells, consuming `primitives_per_shell[i]` exponents/
    // coefficients per shell, splitting any combined "SP" (type -1)
    // shell into a separate S shell and P shell (see `Shell`'s doc
    // comment on why — everything past this point only ever sees a
    // single angular momentum per shell).
    let mut shells = Vec::with_capacity(shell_types.len());
    let mut primitive_offset = 0usize;
    for (shell_idx, &raw_type) in shell_types.iter().enumerate() {
        let num_primitives = primitives_per_shell[shell_idx];
        let exponents = primitive_exponents[primitive_offset..primitive_offset + num_primitives].to_vec();
        let s_coefficients = contraction_coefficients[primitive_offset..primitive_offset + num_primitives].to_vec();
        let center = DVec3::new(
            shell_coordinates[shell_idx * 3],
            shell_coordinates[shell_idx * 3 + 1],
            shell_coordinates[shell_idx * 3 + 2],
        );

        if raw_type == -1 {
            let sp_coefficients = sp_contraction_coefficients
                .as_ref()
                .ok_or_else(|| invalid_data("fchk has an SP shell but no 'P(S=P) Contraction coefficients'"))?;
            let p_coefficients = sp_coefficients[primitive_offset..primitive_offset + num_primitives].to_vec();
            shells.push(Shell { angular_momentum: 0, is_pure: false, center, primitive_exponents: exponents.clone(), contraction_coefficients: s_coefficients });
            shells.push(Shell { angular_momentum: 1, is_pure: false, center, primitive_exponents: exponents, contraction_coefficients: p_coefficients });
        } else {
            shells.push(Shell {
                angular_momentum: raw_type.unsigned_abs() as u32,
                is_pure: raw_type < 0,
                center,
                primitive_exponents: exponents,
                contraction_coefficients: s_coefficients,
            });
        }

        primitive_offset += num_primitives;
    }

    Ok(Wavefunction {
        basis: BasisSet { shells },
        alpha: MolecularOrbitals { num_basis_functions, orbital_energies: alpha_orbital_energies, coefficients: alpha_mo_coefficients, num_occupied: num_alpha_electrons },
        beta,
    })
}
