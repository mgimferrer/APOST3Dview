//! Electron and spin densities from the occupied orbitals, checked against
//! the density matrices Gaussian itself writes to the `.fchk` ("Total SCF
//! Density", "Spin SCF Density"): rho(r) = sum_mu,nu P_mu,nu chi_mu(r)
//! chi_nu(r), evaluated here independently, at every point of a coarse
//! grid. Covers restricted (6-31G, 6-31G*, cc-pVTZ), an ECP with g shells
//! (BiCl3/def2-QZVPP) and an unrestricted triplet (H2O, cc-pVTZ).

use std::path::{Path, PathBuf};

use apost3dview_core::{evaluate_basis_functions, generate_density_grid, parse_fchk_wavefunction};
use glam::DVec3;

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../validation/gaussian").join(name)
}

/// A lower-triangle density matrix block of a `.fchk`, as a full matrix.
fn density_matrix(path: &Path, label: &str, n: usize) -> Option<Vec<f64>> {
    let text = std::fs::read_to_string(path).unwrap();
    let start = text.lines().position(|l| l.starts_with(label))?;
    let count: usize = text.lines().nth(start).unwrap().split_whitespace().last().unwrap().parse().unwrap();
    assert_eq!(count, n * (n + 1) / 2);
    let values: Vec<f64> = text.lines().skip(start + 1).flat_map(|l| l.split_whitespace().map(|t| t.parse::<f64>().unwrap()).collect::<Vec<_>>()).take(count).collect();
    let mut full = vec![0.0; n * n];
    let mut k = 0;
    for i in 0..n {
        for j in 0..=i {
            full[i * n + j] = values[k];
            full[j * n + i] = values[k];
            k += 1;
        }
    }
    Some(full)
}

fn check(name: &str, spin: bool) {
    let path = testdata(name);
    let wfn = parse_fchk_wavefunction(&path).unwrap();
    let n = wfn.alpha.num_basis_functions;
    let label = if spin { "Spin SCF Density" } else { "Total SCF Density" };
    let p = density_matrix(&path, label, n).unwrap_or_else(|| panic!("{name}: no {label}"));
    let terms = if spin { wfn.spin_density_weights().unwrap() } else { wfn.total_density_weights() };
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
                let mut rho = 0.0;
                for (a, ca) in chi.iter().enumerate() {
                    rho += ca * p[a * n..(a + 1) * n].iter().zip(&chi).map(|(pab, cb)| pab * cb).sum::<f64>();
                }
                let ours = grid.values[(i * ny + j) * nz + k] as f64;
                max_diff = max_diff.max((ours - rho).abs());
                max_value = max_value.max(rho.abs());
            }
        }
    }
    // f32 storage: relative to the largest value on the grid.
    assert!(max_diff <= 1e-5 * max_value.max(1e-3), "{name} {label}: max difference {max_diff:e}, largest value {max_value:e}");
    eprintln!("{name} {label}: max difference {max_diff:.2e} of {max_value:.3e}");
}

#[test]
fn total_densities_match_the_fchk_density_matrix() {
    for name in ["H2O-631G.fchk", "H2O-631Gstar.fchk", "H2O-ccpVTZ.fchk", "BiCl3-def2QZVPP.fchk", "H2O-T-ccpVTZ.fchk"] {
        check(name, false);
    }
}

#[test]
fn the_spin_density_matches_the_fchk_spin_density_matrix() {
    check("H2O-T-ccpVTZ.fchk", true);
    // A closed shell has none.
    assert!(parse_fchk_wavefunction(&testdata("H2O-631G.fchk")).unwrap().spin_density_weights().is_none());
}
