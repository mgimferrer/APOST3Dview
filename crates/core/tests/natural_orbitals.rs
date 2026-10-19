//! Natural orbitals from the density matrices of a `.fchk`
//! (`density_matrix.rs`), checked against those matrices themselves: the
//! density sum_i n_i |phi_i|^2 of the natural orbitals equals
//! sum_mu,nu P_mu,nu chi_mu chi_nu, evaluated here independently, at every
//! point of a coarse grid; the occupations add up to the electrons (for a
//! spin density, to N_alpha - N_beta) and lie between 0 and 2. Covers the
//! correlated 1-RDMs APOST-3D's apost3d.py writes into "Total SCF
//! Density" (CASSCF and CISD water, FCI and CAS(2,2) LiH, one with linear
//! dependencies removed), Gaussian's MP2 density ("Total MP2 Density"),
//! and mean-field files, whose natural orbitals are the occupied orbitals.

use std::path::{Path, PathBuf};

use apost3dview_core::{
    evaluate_basis_functions, generate_density_grid, natural_orbitals, parse_fchk_wavefunction, read_fchk_density, scan_fchk_densities, Wavefunction,
};
use glam::DVec3;

fn fixture(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/apost3d").join(name);
    path.is_file().then_some(path).or_else(|| {
        eprintln!("{name} not found; run scripts/sync-apost3d-fixtures.sh");
        None
    })
}

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TESTS-VISUALIZER").join(name)
}

/// Compares the density of `orbitals` (weights = their occupations) with
/// the lower-triangle matrix `lower`, on a coarse grid.
fn check_density(name: &str, wfn: &Wavefunction, orbitals: &apost3dview_core::MolecularOrbitals, lower: &[f64]) {
    let n = wfn.alpha.num_basis_functions;
    let p = |a: usize, b: usize| {
        let (i, j) = if a >= b { (a, b) } else { (b, a) };
        lower[i * (i + 1) / 2 + j]
    };
    let terms = vec![(orbitals, orbitals.occupations.clone())];
    let (spacing, padding) = (0.9, 3.0);
    let grid = generate_density_grid(&wfn.basis, &terms, spacing, padding).unwrap();
    let bohr = 1.0 / 0.529_177_210_903;
    let origin = DVec3::new(grid.origin.x as f64, grid.origin.y as f64, grid.origin.z as f64) * bohr;
    let [nx, ny, nz] = grid.dims;
    let (mut max_diff, mut max_value) = (0.0f64, 0.0f64);
    let mut chi = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                let point = origin + DVec3::new(i as f64, j as f64, k as f64) * spacing;
                evaluate_basis_functions(&wfn.basis, point, &mut chi).unwrap();
                let rho: f64 = (0..n).map(|a| chi[a] * (0..n).map(|b| p(a, b) * chi[b]).sum::<f64>()).sum();
                max_diff = max_diff.max((grid.values[(i * ny + j) * nz + k] as f64 - rho).abs());
                max_value = max_value.max(rho.abs());
            }
        }
    }
    // f32 storage: relative to the largest value on the grid.
    assert!(max_diff <= 1e-5 * max_value.max(1e-3), "{name}: max difference {max_diff:e}, largest value {max_value:e}");
    eprintln!("{name}: max difference {max_diff:.2e} of {max_value:.3e}");
}

fn electrons(path: &Path) -> (f64, f64) {
    let text = std::fs::read_to_string(path).unwrap();
    let count = |label: &str| text.lines().find(|l| l.starts_with(label)).unwrap().split_whitespace().last().unwrap().parse::<f64>().unwrap();
    (count("Number of alpha electrons"), count("Number of beta electrons"))
}

/// The total (and spin) natural orbitals of `kind`'s density in `path`.
/// `bounded`: occupations between 0 and 2, as for the density of a
/// wavefunction (CASSCF, CI, FCI, SCF); not for MP2, whose density isn't
/// one and whose occupations may stray slightly outside.
fn check_file(path: &Path, kind: &str, bounded: bool) {
    let name = format!("{} ({kind})", path.file_name().unwrap().to_string_lossy());
    let wfn = parse_fchk_wavefunction(path).unwrap();
    let density = read_fchk_density(path, kind).unwrap();
    let (na, nb) = electrons(path);

    let no = natural_orbitals(&wfn.alpha, &density.total).unwrap();
    let sum: f64 = no.occupations.iter().sum();
    assert!((sum - (na + nb)).abs() < 1e-5, "{name}: occupations add up to {sum}, not {}", na + nb);
    let margin = if bounded { 1e-6 } else { 0.01 };
    assert!(no.occupations.iter().all(|&o| (-margin..=2.0 + margin).contains(&o)), "{name}: {:?}", &no.occupations[..6.min(no.occupations.len())]);
    assert!(no.occupations.windows(2).all(|w| w[0] >= w[1]), "{name}: largest first");
    check_density(&name, &wfn, &no, &density.total);

    // (A singlet's spin block is all zeros: nothing to compare.)
    if let Some(spin) = density.spin.as_ref().filter(|s| s.iter().any(|v| v.abs() > 1e-10)) {
        let spin_no = natural_orbitals(&wfn.alpha, spin).unwrap();
        let sum: f64 = spin_no.occupations.iter().sum();
        assert!((sum - (na - nb)).abs() < 1e-5, "{name}: spin occupations add up to {sum}, not {}", na - nb);
        check_density(&format!("{name} spin"), &wfn, &spin_no, spin);
    }
}

#[test]
fn correlated_densities_and_their_natural_orbitals() {
    for name in ["H2O-CAS44.fchk", "H2O-CISD.fchk", "LiH-35-CAS22.fchk", "LiH-32-FCI.fchk", "LiH-32-FCI-LINDEP.fchk"] {
        let Some(path) = fixture(name) else { return };
        let blocks = scan_fchk_densities(&path).unwrap();
        assert!(blocks.correlated_scf, "{name}: {blocks:?}");
        assert!(blocks.is_correlated("SCF"));
        check_file(&path, "SCF", true);
        // Fractional occupations: a correlated density.
        let no = natural_orbitals(&parse_fchk_wavefunction(&path).unwrap().alpha, &read_fchk_density(&path, "SCF").unwrap().total).unwrap();
        assert!(no.occupations.iter().any(|&o| o > 0.01 && o < 1.99), "{name}: {:?}", &no.occupations[..6.min(no.occupations.len())]);
    }
}

#[test]
fn gaussian_mp2_density() {
    let Some(path) = fixture("H2O-MP2-DENS2.fchk") else { return };
    let blocks = scan_fchk_densities(&path).unwrap();
    assert_eq!(blocks.kinds, vec!["SCF".to_string(), "MP2".to_string()]);
    assert!(!blocks.correlated_scf && blocks.is_correlated("MP2") && !blocks.is_correlated("SCF"), "{blocks:?}");
    check_file(&path, "MP2", false);
    check_file(&path, "SCF", true);
}

/// Mean-field files: for a restricted closed shell the natural orbitals
/// are the occupied orbitals (occupations 2 and 0); for a UHF triplet the
/// spin-free natural orbitals have slightly fractional occupations (spin
/// contamination), and the spin density's natural orbitals add up to 2.
#[test]
fn mean_field_densities() {
    for (name, integer) in [("H2O-ccpVTZ.fchk", true), ("H2O-T-ccpVTZ.fchk", false)] {
        let path = testdata(name);
        let blocks = scan_fchk_densities(&path).unwrap();
        assert_eq!(blocks.kinds, vec!["SCF".to_string()], "{name}");
        assert!(!blocks.correlated_scf);
        check_file(&path, "SCF", true);
        let no = natural_orbitals(&parse_fchk_wavefunction(&path).unwrap().alpha, &read_fchk_density(&path, "SCF").unwrap().total).unwrap();
        let all_integer = no.occupations.iter().all(|&o| (o - o.round()).abs() < 1e-6);
        assert_eq!(all_integer, integer, "{name}: {:?}", &no.occupations[..8]);
    }
}
