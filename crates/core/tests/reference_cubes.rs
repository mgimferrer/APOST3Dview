//! Orbitals evaluated by APOST3Dview against the cube files the programs
//! that computed them wrote themselves (2026-10-21): PySCF `.molden` files
//! (`TESTS-VISUALIZER/molden-pyscf/`, s-p, Cartesian d, pure through g, an
//! unrestricted triplet; kept here so the ORCA corrections can't change
//! them unnoticed), Q-Chem 6.3 `.fchk`
//! files (pure d/f, Cartesian d, pure g, an unrestricted triplet) and
//! ORCA 6.1.1 `.molden` files from `orca_2mkl` (s-p-d, f, g, an
//! unrestricted triplet, BiCl3 with an ECP, CASSCF natural orbitals), plus
//! ORCA's CASSCF density against the density of the natural orbitals with
//! their occupations. Each must match to numerical noise. The data is in
//! `TESTS-VISUALIZER/qchem/` and `TESTS-VISUALIZER/orca/` (inputs and
//! READMEs there); the test passes with a notice when it's absent.

use std::path::{Path, PathBuf};

use apost3dview_core::units::ANGSTROM_PER_BOHR;
use apost3dview_core::{evaluate_basis_functions, parse_cube, parse_fchk_wavefunction, parse_molden_wavefunction, Wavefunction};
use glam::DVec3;

fn data(folder: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TESTS-VISUALIZER").join(folder);
    dir.is_dir().then_some(dir).or_else(|| {
        eprintln!("TESTS-VISUALIZER/{folder} not found; test skipped");
        None
    })
}

/// RMS difference relative to the largest reference value, over every
/// third grid point along each axis; `value` gives ours from the basis
/// function values at a point.
fn compare(wfn: &Wavefunction, cube: &Path, value: impl Fn(&[f64]) -> f64) -> f64 {
    let grid = parse_cube(cube).unwrap_or_else(|e| panic!("{}: {e}", cube.display())).grid;
    let (mut sum, mut largest, mut count, mut chi) = (0.0, 0.0f64, 0usize, Vec::new());
    for i in (0..grid.dims[0]).step_by(3) {
        for j in (0..grid.dims[1]).step_by(3) {
            for k in (0..grid.dims[2]).step_by(3) {
                let p = grid.world_position(i as f32, j as f32, k as f32);
                evaluate_basis_functions(&wfn.basis, DVec3::new(p.x as f64, p.y as f64, p.z as f64) / ANGSTROM_PER_BOHR, &mut chi).unwrap();
                let reference = grid.value_at(i, j, k) as f64;
                sum += (value(&chi) - reference).powi(2);
                largest = largest.max(reference.abs());
                count += 1;
            }
        }
    }
    (sum / count as f64).sqrt() / largest
}

/// An orbital (0-based `index`, beta if `beta`) against its cube, either sign.
fn check_orbital(name: &str, wfn: &Wavefunction, cube: &Path, beta: bool, index: usize) {
    let orbitals = if beta { wfn.beta.as_ref().expect("beta orbitals") } else { &wfn.alpha };
    let c = orbitals.coefficients_for(index);
    let phi = |chi: &[f64]| chi.iter().zip(c).map(|(x, y)| x * y).sum::<f64>();
    let same = compare(wfn, cube, phi);
    let flipped = compare(wfn, cube, |chi| -phi(chi));
    let error = same.min(flipped);
    eprintln!("{name}: RMS {:.1e} of peak", error);
    assert!(error < 1e-5, "{name}: RMS {error:e} of peak");
}

#[test]
fn qchem_fchk_orbitals_match_qchem_cubes() {
    let Some(dir) = data("qchem") else { return };
    for (job, cases) in [
        ("H2O-QC-ccpVTZ", &[("mo.5", false, 4), ("mo.6", false, 5)][..]),
        ("H2O-QC-631Gstar-cart", &[("mo.5", false, 4), ("mo.6", false, 5)]),
        ("H2O-QC-ccpVQZ", &[("mo.5", false, 4), ("mo.6", false, 5)]),
        ("H2O-QC-T-ccpVTZ", &[("mo.6", false, 5), ("mo.7", false, 6), ("mo_b.4", true, 3), ("mo_b.5", true, 4)]),
    ] {
        let wfn = parse_fchk_wavefunction(&dir.join(format!("{job}.fchk"))).unwrap();
        for &(cube, beta, index) in cases {
            check_orbital(&format!("{job} {cube}"), &wfn, &dir.join(format!("{job}.plots/{cube}.cube")), beta, index);
        }
    }
}

#[test]
fn orca_molden_orbitals_match_orca_cubes() {
    let Some(dir) = data("orca") else { return };
    for (job, cases) in [
        ("H2O-ORCA-def2SVP", &[("MO4", false, 4), ("MO5", false, 5)][..]),
        ("H2O-ORCA-ccpVTZ", &[("MO4", false, 4), ("MO5", false, 5)]),
        ("H2O-ORCA-ccpVQZ", &[("MO4", false, 4), ("MO5", false, 5)]),
        ("H2O-ORCA-CAS44-def2SVP", &[("MO4", false, 4), ("MO5", false, 5)]),
        ("O2-ORCA-T-ccpVTZ", &[("MO8", false, 8), ("MO9", false, 9), ("MO6b", true, 6), ("MO7b", true, 7)]),
        ("BiCl3-ORCA-def2TZVPP", &[("MO36", false, 36), ("MO37", false, 37)]),
    ] {
        let wfn = parse_molden_wavefunction(&dir.join(format!("{job}.molden"))).unwrap();
        for &(cube, beta, index) in cases {
            check_orbital(&format!("{job} {cube}"), &wfn, &dir.join(format!("{job}-{cube}.cube")), beta, index);
        }
    }
}

/// ORCA's CASSCF density equals sum n_i |phi_i|^2 over the natural orbitals
/// of the `.molden`, with their fractional occupations.
#[test]
fn orca_casscf_density_from_natural_occupations() {
    let Some(dir) = data("orca") else { return };
    let wfn = parse_molden_wavefunction(&dir.join("H2O-ORCA-CAS44-def2SVP.molden")).unwrap();
    assert!(wfn.alpha.occupations.iter().any(|&n| n > 0.01 && n < 1.99), "{:?}", &wfn.alpha.occupations[..8]);
    let terms = wfn.total_density_weights();
    let rho = |chi: &[f64]| {
        terms
            .iter()
            .map(|(mo, weights)| {
                weights.iter().enumerate().filter(|(_, w)| **w != 0.0).map(|(m, w)| w * mo.coefficients_for(m).iter().zip(chi).map(|(c, x)| c * x).sum::<f64>().powi(2)).sum::<f64>()
            })
            .sum::<f64>()
    };
    let error = compare(&wfn, &dir.join("H2O-ORCA-CAS44-def2SVP-density.cube"), rho);
    eprintln!("CASSCF density: RMS {error:.1e} of peak");
    assert!(error < 1e-5, "RMS {error:e} of peak");
}

#[test]
fn pyscf_molden_orbitals_match_pyscf_cubes() {
    let Some(dir) = data("molden-pyscf") else { return };
    for (file, beta, cubes) in [
        ("h2o_631g", false, ["h2o_631g_homo", "h2o_631g_lumo"]),
        ("h2o_631gstar_cart", false, ["h2o_631gstar_cart_homo", "h2o_631gstar_cart_lumo"]),
        ("h2o_ccpvqz_pure", false, ["h2o_ccpvqz_pure_homo", "h2o_ccpvqz_pure_lumo"]),
        ("o2_triplet_ccpvtz", false, ["o2_triplet_ccpvtz_alpha_homo", "o2_triplet_ccpvtz_alpha_lumo"]),
        ("o2_triplet_ccpvtz", true, ["o2_triplet_ccpvtz_beta_homo", "o2_triplet_ccpvtz_beta_lumo"]),
    ] {
        let wfn = parse_molden_wavefunction(&dir.join(format!("{file}.molden"))).unwrap();
        let orbitals = if beta { wfn.beta.as_ref().unwrap() } else { &wfn.alpha };
        let (homo, lumo) = (orbitals.homo_index() - 1, orbitals.lumo_index() - 1);
        for (cube, index) in cubes.into_iter().zip([homo, lumo]) {
            check_orbital(cube, &wfn, &dir.join(format!("{cube}.cube")), beta, index);
        }
    }
}
