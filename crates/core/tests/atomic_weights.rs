//! The atomic weights rebuilt from the results file (`atomic_weights`,
//! schema 0.16) against APOST-3D's own: `validation/apost3d/` holds, per
//! system, the results file and 105 points (at and near the nuclei, along
//! bonds, random) with every atom's and fragment's weight from an
//! evaluator that reproduces APOST-3D to 1e-13 (TFVC: CH3F GEOS, FeCO2 EOS;
//! Becke profile: CH3F EOS with BECKE-RHO).

use std::path::{Path, PathBuf};

use apost3dview_core::{generate_mo_grids, generate_weighted_mo_grids, load_apost_results, parse_fchk_wavefunction, CellScratch};
use glam::DVec3;
use serde::Deserialize;

fn data() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../validation/apost3d");
    dir.is_dir().then_some(dir).or_else(|| {
        eprintln!("validation/apost3d not found; test skipped");
        None
    })
}

#[derive(Deserialize)]
struct WeightsCheck {
    source: String,
    profile: String,
    points_bohr: Vec<[f64; 3]>,
    w_atoms: Vec<Vec<f64>>,
    w_fragments: Vec<Vec<f64>>,
}

#[test]
fn atomic_weights_match_apost3d() {
    let Some(dir) = data() else { return };
    let mut checked = 0;
    for job in ["CH3F-GEOS", "FeCO2-PBEPBE", "CH3F-BECKE-RHO-EOS"] {
        let check: WeightsCheck = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{job}.weights-check.json"))).unwrap()).unwrap();
        let results = load_apost_results(&dir.join(&check.source)).unwrap();
        let cells = results.atom_cells().expect("no atomic_weights block").unwrap();
        let n = cells.n_atoms();
        let (mut worst_atom, mut worst_fragment) = (0.0f64, 0.0f64);
        let mut weights = vec![0.0; n];
        let mut scratch = CellScratch::default();
        for (i, point) in check.points_bohr.iter().enumerate() {
            cells.atom_weights(DVec3::from_array(*point), &mut scratch, &mut weights);
            for (w, expected) in weights.iter().zip(&check.w_atoms[i]) {
                worst_atom = worst_atom.max((w - expected).abs());
            }
            for (f, fragment) in results.fragments().iter().enumerate() {
                let w: f64 = results.fragment_atoms(&[fragment.id]).iter().map(|&a| weights[a]).sum();
                worst_fragment = worst_fragment.max((w - check.w_fragments[i][f]).abs());
            }
        }
        println!("{job} ({}): {} points, largest difference {worst_atom:.1e} (atoms), {worst_fragment:.1e} (fragments)", check.profile, check.points_bohr.len());
        assert!(worst_atom < 1e-11 && worst_fragment < 1e-11, "{job}: {worst_atom:e} / {worst_fragment:e}");
        checked += 1;
    }
    assert_eq!(checked, 3);
}

/// Hilbert-space atoms have no weight functions, and files before 0.16
/// none either: nothing to weight with.
#[test]
fn no_block_no_weights() {
    let Some(dir) = data() else { return };
    let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("CH3F-GEOS.apost.json")).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("atomic_weights");
    let results = apost3dview_core::parse_apost_results(&value.to_string()).unwrap();
    assert!(results.atom_cells().is_none());
}

/// The weighted grid is the plain one times w_F, point by point, and
/// nearly vanishes on the other fragment (CH3F GEOS, the example run).
#[test]
fn weighted_grids_are_the_orbitals_times_the_fragment_weight() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/CH3F-GEOS");
    let results = load_apost_results(&dir.join("CH3F-GEOS.apost.json")).unwrap();
    let Some(cells) = results.atom_cells().map(Result::unwrap) else {
        eprintln!("examples/CH3F-GEOS has no atomic weights (written before schema 0.16); test skipped");
        return;
    };
    let wfn = parse_fchk_wavefunction(&dir.join("CH3F-GEOS-GEOS-EFOs.fchk")).unwrap();
    // Every paired EFO of each fragment, weighted by its own fragment.
    let sets = results.orbital_sets();
    let geos = sets.iter().find(|s| s.analysis == "geos").unwrap();
    let mut requests = Vec::new();
    let mut atoms = Vec::new();
    for (index, label) in geos.alpha.iter().enumerate() {
        if let Some(label) = label {
            requests.push((&wfn.alpha, index));
            atoms.push(Some(results.fragment_atoms(&label.fragments)));
        }
    }
    let (spacing, padding) = (0.35, 4.0);
    let plain = generate_mo_grids(&wfn.basis, &requests, spacing, padding).unwrap();
    let weighted = generate_weighted_mo_grids(&wfn.basis, &requests, &cells, &atoms, spacing, padding).unwrap();
    let mut weights = vec![0.0; cells.n_atoms()];
    let mut scratch = CellScratch::default();
    let (mut worst, mut outside_before, mut outside_after) = (0.0f64, 0.0f64, 0.0f64);
    for (r, (p, w)) in plain.iter().zip(&weighted).enumerate() {
        let group = atoms[r].as_ref().unwrap();
        for i in 0..p.dims[0] {
            for j in 0..p.dims[1] {
                for k in 0..p.dims[2] {
                    let at = p.world_position(i as f32, j as f32, k as f32);
                    let point = DVec3::new(at.x as f64, at.y as f64, at.z as f64) / apost3dview_core::units::ANGSTROM_PER_BOHR;
                    cells.atom_weights(point, &mut scratch, &mut weights);
                    let w_f: f64 = group.iter().map(|&a| weights[a]).sum();
                    let index = p.index(i, j, k);
                    let (before, after) = (p.values[index] as f64, w.values[index] as f64);
                    worst = worst.max((after - before * w_f).abs());
                    if w_f < 0.01 {
                        outside_before = outside_before.max(before.abs());
                        outside_after = outside_after.max(after.abs());
                    }
                }
            }
        }
    }
    println!("{} EFOs: |weighted - w_F x plain| <= {worst:.1e}; largest value where w_F < 0.01: {outside_before:.4} plain, {outside_after:.6} weighted", requests.len());
    // f32 storage and grid positions rounded through angstrom.
    assert!(worst < 1e-6, "{worst}");
    assert!(outside_after < 0.01 * outside_before.max(1e-3), "{outside_after} vs {outside_before}");
}
