//! Orbitals evaluated by APOST3Dview against cube files written by other
//! programs: Gaussian `.fchk` files against Chemcraft's cubes
//! (`validation/gaussian/`), and against the cube files the programs
//! that computed them wrote themselves (2026-10-21): PySCF `.molden` files
//! (`validation/pyscf/`, s-p, Cartesian d, pure through g, an
//! unrestricted triplet; kept here so the ORCA corrections can't change
//! them unnoticed), Q-Chem 6.3 `.fchk`
//! files (pure d/f, Cartesian d, pure g, an unrestricted triplet) and
//! ORCA 6.1.1 `.molden` files from `orca_2mkl` (s-p-d, f, g, an
//! unrestricted triplet, BiCl3 with an ECP, CASSCF natural orbitals), plus
//! ORCA's CASSCF density against the density of the natural orbitals with
//! their occupations. Each must match to numerical noise. The data is in
//! `validation/`, one folder per program (inputs and READMEs there); a test
//! passes with a notice when its folder is absent.

use std::path::{Path, PathBuf};

use apost3dview_core::units::ANGSTROM_PER_BOHR;
use apost3dview_core::{evaluate_basis_functions, natural_orbitals, parse_cube, parse_fchk_wavefunction, parse_molden_wavefunction, read_fchk_density, MolecularOrbitals, Wavefunction};
use glam::DVec3;

fn data(folder: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../validation").join(folder);
    dir.is_dir().then_some(dir).or_else(|| {
        eprintln!("validation/{folder} not found; test skipped");
        None
    })
}

/// RMS and largest difference, relative to the largest reference value,
/// over every point of the cube; `value` gives ours from the basis
/// function values at a point.
fn compare(wfn: &Wavefunction, cube: &Path, value: impl Fn(&[f64]) -> f64) -> (f64, f64) {
    let grid = parse_cube(cube).unwrap_or_else(|e| panic!("{}: {e}", cube.display())).grid;
    let (mut sum, mut largest, mut worst, mut count, mut chi) = (0.0, 0.0f64, 0.0f64, 0usize, Vec::new());
    for i in 0..grid.dims[0] {
        for j in 0..grid.dims[1] {
            for k in 0..grid.dims[2] {
                let p = grid.world_position(i as f32, j as f32, k as f32);
                evaluate_basis_functions(&wfn.basis, DVec3::new(p.x as f64, p.y as f64, p.z as f64) / ANGSTROM_PER_BOHR, &mut chi).unwrap();
                let reference = grid.value_at(i, j, k) as f64;
                let d = value(&chi) - reference;
                sum += d * d;
                worst = worst.max(d.abs());
                largest = largest.max(reference.abs());
                count += 1;
            }
        }
    }
    ((sum / count as f64).sqrt() / largest, worst / largest)
}

/// RMS below 1e-5 and largest difference below 1e-3 of the peak: the cube
/// files carry 5 or 6 significant digits.
fn check(name: &str, (rms, max): (f64, f64)) {
    eprintln!("{name}: RMS {rms:.1e}, max {max:.1e} of peak");
    assert!(rms < 1e-5 && max < 1e-3, "{name}: RMS {rms:e}, max {max:e} of peak");
}

/// An orbital (0-based `index`, beta if `beta`) against its cube, either sign.
fn check_orbital(name: &str, wfn: &Wavefunction, cube: &Path, beta: bool, index: usize) {
    let orbitals = if beta { wfn.beta.as_ref().expect("beta orbitals") } else { &wfn.alpha };
    let c = orbitals.coefficients_for(index);
    let phi = |chi: &[f64]| chi.iter().zip(c).map(|(x, y)| x * y).sum::<f64>();
    let same = compare(wfn, cube, phi);
    let flipped = compare(wfn, cube, |chi| -phi(chi));
    check(name, if same.0 < flipped.0 { same } else { flipped });
}

/// Σ n_i φ_i² over orbitals with their occupations as weights.
fn density(mos: &MolecularOrbitals) -> impl Fn(&[f64]) -> f64 + '_ {
    move |chi: &[f64]| {
        (0..mos.num_orbitals())
            .filter(|&m| mos.occupations[m].abs() > 1e-12)
            .map(|m| mos.occupations[m] * mos.coefficients_for(m).iter().zip(chi).map(|(c, x)| c * x).sum::<f64>().powi(2))
            .sum()
    }
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
    check("CASSCF density", compare(&wfn, &dir.join("H2O-ORCA-CAS44-def2SVP-density.cube"), rho));
}

/// Gaussian 16 `.fchk` files against Chemcraft's cubes of the same orbitals
/// (the evaluator's first validation set, 2026-08-13): s-p, Cartesian d,
/// pure d/f, BiCl3 with an ECP (pure through f, and g at def2-QZVPP), and
/// both spin channels of an unrestricted triplet.
#[test]
fn gaussian_fchk_orbitals_match_chemcraft_cubes() {
    let Some(dir) = data("gaussian") else { return };
    for (job, beta, cubes) in [
        ("H2O-631G", false, &[("HOMO", 0), ("LUMO", 1)][..]),
        ("H2O-631Gstar", false, &[("HOMO", 0), ("LUMO", 1)]),
        ("H2O-ccpVDZ", false, &[("HOMO", 0), ("LUMO", 1)]),
        ("H2O-ccpVTZ", false, &[("HOMO", 0), ("LUMO", 1)]),
        ("H2O-T-ccpVTZ", false, &[("HOMO-alpha", 0), ("LUMO-alpha", 1)]),
        ("H2O-T-ccpVTZ", true, &[("HOMO-beta", 0), ("LUMO-beta", 1)]),
        ("BiCl3-def2TZVPP", false, &[("HOMO", 0), ("LUMO", 1)]),
        ("BiCl3-def2QZVPP", false, &[("HOMO", 0), ("LUMO", 1)]),
    ] {
        let wfn = parse_fchk_wavefunction(&dir.join(format!("{job}.fchk"))).unwrap();
        let orbitals = if beta { wfn.beta.as_ref().unwrap() } else { &wfn.alpha };
        let frontier = [orbitals.homo_index() - 1, orbitals.lumo_index() - 1];
        for &(cube, which) in cubes {
            check_orbital(&format!("{job} {cube}"), &wfn, &dir.join(format!("{job}-{cube}.cube")), beta, frontier[which]);
        }
    }
    // Two orbitals further from the frontier, numbered from 1 as in Gaussian.
    let wfn = parse_fchk_wavefunction(&dir.join("BiCl3-def2TZVPP.fchk")).unwrap();
    for mo in [45, 51] {
        check_orbital(&format!("BiCl3-def2TZVPP MO{mo}"), &wfn, &dir.join(format!("BiCl3-def2TZVPP-MO{mo}.cube")), false, mo - 1);
    }
}

#[test]
fn pyscf_molden_orbitals_match_pyscf_cubes() {
    let Some(dir) = data("pyscf") else { return };
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

/// Gaussian's own cube files (cubegen, Gaussian 16 A.03) of cases the
/// Chemcraft set lacks (`validation/gaussian/cubegen/`): Cartesian d and f
/// (6-31G(2df) 6D 10F), diffuse functions (aug-cc-pVTZ), core orbitals, a
/// restricted open-shell triplet, and an MP2 density, through its natural
/// orbitals. cubegen leaves out the frozen-core orbital (the O 1s, MO 1) of
/// densities from an MP2 frozen-core file; the viewer's density includes
/// it, so it is subtracted here.
#[test]
fn gaussian_fchk_orbitals_and_mp2_density_match_cubegen() {
    let Some(dir) = data("gaussian/cubegen") else { return };
    for (job, cases) in [
        ("H2O-631G2df-cart", &[("HOMO", 4), ("LUMO", 5), ("MO1", 0)][..]),
        ("H2O-augccpVTZ", &[("HOMO", 4), ("LUMO", 5), ("MO1", 0)]),
        ("CH2-T-ROHF-ccpVTZ", &[("HOMO", 4), ("LUMO", 5), ("MO3", 2)]),
    ] {
        let wfn = parse_fchk_wavefunction(&dir.join(format!("{job}.fchk"))).unwrap();
        for &(cube, index) in cases {
            check_orbital(&format!("{job} {cube}"), &wfn, &dir.join(format!("{job}-{cube}.cube")), false, index);
        }
    }
    let path = dir.join("H2O-MP2-ccpVTZ.fchk");
    let wfn = parse_fchk_wavefunction(&path).unwrap();
    let no = natural_orbitals(&wfn.alpha, &read_fchk_density(&path, "MP2").unwrap().total).unwrap();
    let rho = density(&no);
    let core = wfn.alpha.coefficients_for(0);
    let valence = |chi: &[f64]| rho(chi) - 2.0 * chi.iter().zip(core).map(|(x, c)| x * c).sum::<f64>().powi(2);
    check("H2O MP2 density", compare(&wfn, &dir.join("H2O-MP2-ccpVTZ-density-MP2.cube"), valence));
}

/// The same ORCA calculations as `orca_molden_orbitals_match_orca_cubes`,
/// converted to .fchk by APOST-3D's orca2fchk.py (commit 142bedd,
/// `validation/orca/orca2fchk/`), against ORCA's cube files: the route by
/// which ORCA users reach APOST-3D. The CASSCF density comes from the
/// file's density block (the CASSCF 1-RDM) through its natural orbitals.
#[test]
fn orca2fchk_files_match_orca_cubes() {
    let Some(dir) = data("orca") else { return };
    let converted = dir.join("orca2fchk");
    for (job, cases) in [
        ("H2O-ORCA-ccpVTZ", &[("MO4", false, 4), ("MO5", false, 5)][..]),
        ("O2-ORCA-T-ccpVTZ", &[("MO8", false, 8), ("MO9", false, 9), ("MO6b", true, 6), ("MO7b", true, 7)]),
        ("BiCl3-ORCA-def2TZVPP", &[("MO36", false, 36), ("MO37", false, 37)]),
        ("H2O-ORCA-CAS44-def2SVP", &[("MO4", false, 4), ("MO5", false, 5)]),
    ] {
        let wfn = parse_fchk_wavefunction(&converted.join(format!("{job}.fchk"))).unwrap();
        for &(cube, beta, index) in cases {
            check_orbital(&format!("{job}.fchk {cube}"), &wfn, &dir.join(format!("{job}-{cube}.cube")), beta, index);
        }
    }
    let path = converted.join("H2O-ORCA-CAS44-def2SVP.fchk");
    let wfn = parse_fchk_wavefunction(&path).unwrap();
    let no = natural_orbitals(&wfn.alpha, &read_fchk_density(&path, "SCF").unwrap().total).unwrap();
    check("H2O-ORCA-CAS44-def2SVP.fchk density", compare(&wfn, &dir.join("H2O-ORCA-CAS44-def2SVP-density.cube"), density(&no)));
}
