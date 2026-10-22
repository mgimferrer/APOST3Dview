//! Correlated densities and their natural orbitals, from the density
//! matrices of a `.fchk` (item 5 of the improvement backlog, 2026-10-19).
//!
//! A mean-field file keeps its densities from the occupied orbitals (the
//! pictures don't change). A file with a correlated density (Gaussian's
//! "Total MP2/CI/CC Density", or the 1-RDM APOST-3D's `apost3d.py` writes
//! into "Total SCF Density" for CASSCF, FCI, CISD and CCSD) offers its
//! natural orbitals, computed once on opening: they give that density
//! exactly as a sum of squares (and its spin density, from the spin
//! block), and are drawable as an orbital set of their own.

use std::path::Path;

use apost3dview_core::{natural_orbitals, read_fchk_density, scan_fchk_densities, FchkDensities, MolecularOrbitals, Wavefunction};

/// Natural occupations below this are left out of a density (their sum
/// stays far below anything an isosurface shows).
const LEAST_OCCUPATION: f64 = 1e-6;

/// The natural orbitals of one density matrix of a `.fchk`.
pub(crate) struct NaturalOrbitals {
    /// The density block: "SCF", "MP2", "CI", "CC".
    pub(crate) kind: String,
    /// What to call it: the block's kind, or the method for a correlated
    /// density stored as "SCF" ("CASSCF", "CCSD").
    pub(crate) name: String,
    /// `alpha`: the natural orbitals of the total density, largest
    /// occupation first, occupations as weights; no `beta`.
    pub(crate) orbitals: Wavefunction,
    /// The natural orbitals of the spin density (signed occupations), when
    /// the file has a spin block that isn't zero.
    pub(crate) spin: Option<MolecularOrbitals>,
}

impl NaturalOrbitals {
    /// The orbitals and weights of the total or spin density.
    pub(crate) fn density_weights(&self, spin: bool) -> Option<Vec<(&MolecularOrbitals, Vec<f64>)>> {
        let orbitals = if spin { self.spin.as_ref()? } else { &self.orbitals.alpha };
        let weights = orbitals.occupations.iter().map(|&n| if n.abs() < LEAST_OCCUPATION { 0.0 } else { n }).collect();
        Some(vec![(orbitals, weights)])
    }

    /// The key a session records for orbitals of this set.
    pub(crate) fn key(&self) -> String {
        format!("natural-{}", self.kind)
    }
}

/// The density matrices of a `.fchk` beyond the mean-field one.
pub(crate) struct Densities {
    pub(crate) info: FchkDensities,
    /// The natural orbitals of each correlated density, in file order.
    pub(crate) natural: Vec<NaturalOrbitals>,
    /// What the "Total density" and "Spin density" are made of: `None`
    /// the occupied orbitals, `Some(i)` the density of `natural[i]`.
    pub(crate) chosen: Option<usize>,
    /// The Surfaces window lists `natural[i]` instead of the SCF orbitals.
    pub(crate) listed: Option<usize>,
    /// The natural orbitals ticked for generation (0-based), of the listed set.
    pub(crate) selected: std::collections::HashSet<usize>,
}

impl Densities {
    /// Whether the occupied orbitals give a density of this file at all
    /// (not when its only density is a correlated one stored as "SCF").
    pub(crate) fn offers_mean_field(&self) -> bool {
        !self.info.correlated_scf
    }

    /// The natural orbitals the densities are made of, if not the occupied orbitals.
    pub(crate) fn chosen(&self) -> Option<&NaturalOrbitals> {
        self.chosen.and_then(|i| self.natural.get(i))
    }

    pub(crate) fn by_key(&self, key: &str) -> Option<&NaturalOrbitals> {
        self.natural.iter().find(|n| n.key() == key)
    }
}

/// The two natural orbitals either side of occupation 1 (the last above
/// and the first below), ticked when a set is first listed. Not the set's
/// `num_occupied`, which counts occupations above 0.5: for a frontier pair
/// such as 1.21/0.79 (Mg(cAAC)2) it would give the 0.79 orbital and the
/// one after it.
pub(crate) fn frontier(set: &NaturalOrbitals) -> std::collections::HashSet<usize> {
    frontier_of(&set.orbitals.alpha.occupations)
}

/// `frontier` on occupations sorted largest first.
fn frontier_of(occupations: &[f64]) -> std::collections::HashSet<usize> {
    let above = occupations.iter().filter(|&&n| n > 1.0).count();
    [above.checked_sub(1), (above < occupations.len()).then_some(above)].into_iter().flatten().collect()
}

/// "MP2", "CI", or the method ("CASSCF") for a correlated density stored
/// as "SCF".
fn density_name(info: &FchkDensities, kind: &str) -> String {
    if kind == "SCF" && info.correlated_scf {
        if info.method.is_empty() { "correlated".to_string() } else { info.method.clone() }
    } else {
        kind.to_string()
    }
}

fn natural_set(path: &Path, wfn: &Wavefunction, kind: &str, name: String) -> Result<NaturalOrbitals, String> {
    let matrix = read_fchk_density(path, kind).map_err(|e| e.to_string())?;
    let total = natural_orbitals(&wfn.alpha, &matrix.total)?;
    // A singlet's spin block (apost3d.py writes one) is all zeros.
    let spin = match matrix.spin.filter(|s| s.iter().any(|v| v.abs() > 1e-10)) {
        Some(spin) => Some(natural_orbitals(&wfn.alpha, &spin)?),
        None => None,
    };
    Ok(NaturalOrbitals { kind: kind.to_string(), name, orbitals: Wavefunction { basis: wfn.basis.clone(), alpha: total, beta: None }, spin })
}

/// The correlated densities of the `.fchk` at `path` and their natural
/// orbitals; `None` for a mean-field file. Problems are returned to be
/// reported (the densities that worked are still offered).
pub(crate) fn load_densities(path: &Path, wfn: &Wavefunction) -> (Option<Densities>, Vec<String>) {
    let Ok(info) = scan_fchk_densities(path) else { return (None, Vec::new()) };
    let kinds: Vec<String> = info.kinds.iter().filter(|k| info.is_correlated(k)).cloned().collect();
    if kinds.is_empty() {
        return (None, Vec::new());
    }
    let mut natural = Vec::new();
    let mut problems = Vec::new();
    for kind in kinds {
        let name = density_name(&info, &kind);
        match natural_set(path, wfn, &kind, name.clone()) {
            Ok(set) => natural.push(set),
            Err(err) => problems.push(format!("the {name} density could not be used ({err})")),
        }
    }
    // The correlated density by default (the last one: Gaussian writes
    // the SCF density first).
    let chosen = natural.len().checked_sub(1);
    let selected = natural.last().map(frontier).unwrap_or_default();
    (Some(Densities { info, natural, chosen, listed: None, selected }), problems)
}

#[cfg(test)]
mod tests {
    use super::frontier_of;

    #[test]
    fn the_frontier_is_either_side_of_occupation_one() {
        // Mg(cAAC)2, CASSCF(10,8): the LUNO's 0.79 is above 0.5 but not 1.
        assert_eq!(frontier_of(&[2.0, 1.998, 1.977, 1.208, 0.794, 0.026, 0.0]), [3, 4].into());
        assert_eq!(frontier_of(&[2.0, 1.640, 0.362, 0.022]), [1, 2].into());
        // Nothing above or nothing below: one orbital.
        assert_eq!(frontier_of(&[0.9, 0.1]), [0].into());
        assert_eq!(frontier_of(&[2.0, 1.5]), [1].into());
    }
}
