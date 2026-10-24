//! EFOs drawn from APOST-3D's unweighted orbital file (schema 0.17) against
//! the cube files APOST-3D writes of the same EFOs (`validation/apost3d/
//! efo-cubes/`: CH3F EOS, 15 EFOs; LiH FCI GEOS, a negative paired EFO).
//! The cube holds w_F·φ exactly, φ an orbital of the unweighted file and
//! w_F its fragment's weight, rebuilt from the results file; so the two
//! must agree to the cube's 5 printed digits. Each cube is matched to its
//! orbital by trying every EFO of its fragment (the closest one, either
//! sign). For the record, the same cube against the other orbital file
//! (least-squares fits of w_F·φ) as stored, and times w_F once more.

use std::path::{Path, PathBuf};

use apost3dview_core::units::ANGSTROM_PER_BOHR;
use apost3dview_core::{evaluate_basis_functions, load_apost_results, parse_cube, parse_fchk_wavefunction, CellScratch};
use glam::DVec3;

fn data() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../validation/apost3d/efo-cubes");
    dir.is_dir().then_some(dir).or_else(|| {
        eprintln!("validation/apost3d/efo-cubes not found; test skipped");
        None
    })
}

/// RMS and largest deviation relative to the cube's largest value.
#[derive(Default, Clone, Copy)]
struct Deviation {
    sum: f64,
    max: f64,
}

#[test]
fn unweighted_efos_times_their_weight_match_apost3d_cubes() {
    let Some(dir) = data() else { return };
    let mut worst = 0.0f64;
    let mut count = 0;
    for job in ["CH3F-EOS-CUBE", "LiH-32-FCI"] {
        let case = dir.join(job);
        let results = load_apost_results(&case.join(format!("{job}.apost.json"))).unwrap();
        let cells = results.atom_cells().expect("no atomic_weights").unwrap();
        let set = results.orbital_sets().into_iter().find(|s| s.analysis == "eos" || s.analysis == "geos").unwrap();
        let unweighted = parse_fchk_wavefunction(&case.join(set.unweighted_file.as_deref().expect("no orbital_file_unweighted"))).unwrap();
        let fitted = parse_fchk_wavefunction(&case.join(&set.file)).unwrap();
        let mut cubes: Vec<PathBuf> = std::fs::read_dir(&case).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "cube")).collect();
        cubes.sort();
        for cube_path in cubes {
            let name = cube_path.file_stem().unwrap().to_string_lossy().to_string();
            let fragment: u32 = name.rsplit_once("_FR").unwrap().1.split('_').next().unwrap().parse().unwrap();
            // The EFOs of that fragment (all in the file's alpha block here:
            // closed-shell EOS, GEOS paired and negative paired).
            let candidates: Vec<usize> = set.alpha.iter().enumerate().filter(|(_, l)| l.as_ref().is_some_and(|l| l.fragments == [fragment])).map(|(i, _)| i).collect();
            assert!(!candidates.is_empty(), "{name}: no EFO of fragment {fragment}");
            let atoms = results.fragment_atoms(&[fragment]);
            let grid = parse_cube(&cube_path).unwrap().grid;
            let (mut chi, mut scratch, mut weights) = (Vec::new(), CellScratch::default(), vec![0.0; cells.n_atoms()]);
            // Per candidate and sign: unweighted × w_F; per candidate: fit as stored, fit × w_F.
            let mut exact = vec![[Deviation::default(); 2]; candidates.len()];
            let mut stored = vec![[Deviation::default(); 2]; candidates.len()];
            let mut twice = vec![[Deviation::default(); 2]; candidates.len()];
            let (mut largest, mut points) = (0.0f64, 0usize);
            for i in 0..grid.dims[0] {
                for j in 0..grid.dims[1] {
                    for k in 0..grid.dims[2] {
                        let p = grid.world_position(i as f32, j as f32, k as f32);
                        let r = DVec3::new(p.x as f64, p.y as f64, p.z as f64) / ANGSTROM_PER_BOHR;
                        evaluate_basis_functions(&unweighted.basis, r, &mut chi).unwrap();
                        cells.atom_weights(r, &mut scratch, &mut weights);
                        let w: f64 = atoms.iter().map(|&a| weights[a]).sum();
                        let reference = grid.value_at(i, j, k) as f64;
                        largest = largest.max(reference.abs());
                        points += 1;
                        for (c, &mo) in candidates.iter().enumerate() {
                            let phi: f64 = unweighted.alpha.coefficients_for(mo).iter().zip(&chi).map(|(a, b)| a * b).sum();
                            let fit: f64 = fitted.alpha.coefficients_for(mo).iter().zip(&chi).map(|(a, b)| a * b).sum();
                            for (s, sign) in [1.0, -1.0].into_iter().enumerate() {
                                for (store, value) in [(&mut exact, w * phi), (&mut stored, fit), (&mut twice, w * fit)] {
                                    let d = sign * value - reference;
                                    store[c][s].sum += d * d;
                                    store[c][s].max = store[c][s].max.max(d.abs());
                                }
                            }
                        }
                    }
                }
            }
            let rms = |d: Deviation| (d.sum / points as f64).sqrt() / largest;
            let (c, s) = (0..candidates.len()).flat_map(|c| [(c, 0), (c, 1)]).min_by(|a, b| rms(exact[a.0][a.1]).total_cmp(&rms(exact[b.0][b.1]))).unwrap();
            let (e, st, tw) = (exact[c][s], stored[c][s], twice[c][s]);
            println!(
                "{name} (EFO {}): w_F·φ RMS {:.1e} max {:.1e} | fit as stored RMS {:.1e} max {:.1e} | fit × w_F RMS {:.1e} max {:.1e}",
                candidates[c] + 1,
                rms(e),
                e.max / largest,
                rms(st),
                st.max / largest,
                rms(tw),
                tw.max / largest
            );
            worst = worst.max(e.max / largest);
            count += 1;
        }
    }
    println!("{count} EFO cubes; largest deviation of w_F·φ {worst:.1e} of the cube's largest value");
    assert_eq!(count, 16);
    assert!(worst < 1e-4, "largest deviation {worst:e}");
}
