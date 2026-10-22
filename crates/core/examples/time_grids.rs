//! Timings of the expensive steps on a large and a mid-size molecule, for
//! the record (backlog item 6, 2026-10-20; numbers in CLAUDE.md): opening
//! a `.fchk`, densities and orbitals at the three accuracy presets,
//! isosurface extraction, an envelope, natural orbitals, orbitals times
//! a fragment's atomic weight. Needs
//! `fixtures/bi-dianion-osd/Bi-dianion-OSD.fchk` and the APOST-3D fixtures; run
//! in release:
//!
//!     cargo run --release -p apost3dview-core --example time_grids
use std::path::Path;
use std::time::Instant;

use apost3dview_core::*;

fn time<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{what:60} {:8.2} s", start.elapsed().as_secs_f64());
    out
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    println!("threads: {}", rayon::current_num_threads());
    for (name, spin) in [("fixtures/bi-dianion-osd/Bi-dianion-OSD.fchk", false), ("fixtures/apost3d/FeCN5NO3--UBLYP.fchk", true)] {
        let path = root.join(name);
        println!("== {name}");
        let molecule = time("open: geometry", || Molecule::from_fchk(&path).unwrap());
        let wfn = time("open: wavefunction", || parse_fchk_wavefunction(&path).unwrap());
        let densities = time("open: density-matrix scan", || scan_fchk_densities(&path).unwrap());
        println!("  atoms {}, basis functions {}, occupied alpha {}, shells {}, densities {:?}", molecule.atomic_numbers.len(), wfn.alpha.num_basis_functions, wfn.alpha.num_occupied, wfn.basis.shells.len(), densities.kinds);
        let total = wfn.total_density_weights();
        for (label, spacing) in [("Low 0.40", 0.40), ("Medium 0.28", 0.28), ("High 0.15", 0.15)] {
            let grid = time(&format!("total density, {label}"), || generate_density_grid(&wfn.basis, &total, spacing, 4.0).unwrap());
            println!("  grid {:?} = {} points", grid.dims, grid.values.len());
            if spacing == 0.28 {
                let mesh = time("  extract at 0.1 (1x)", || extract_isosurface(&grid, 0.1));
                println!("  triangles {}", mesh.positions.len() / 3);
                let refined = time("  refine 2x", || refine_grid(&grid, 2));
                let mesh = time("  extract at 0.1 (2x)", || extract_isosurface(&refined, 0.1));
                println!("  triangles {}", mesh.positions.len() / 3);
            }
        }
        if spin {
            let weights = wfn.spin_density_weights().unwrap();
            time("spin density, Medium", || generate_density_grid(&wfn.basis, &weights, 0.28, 4.0).unwrap());
        }
        let homo = wfn.alpha.homo_index() - 1;
        time("1 orbital (HOMO), Medium", || generate_mo_grids(&wfn.basis, &[(&wfn.alpha, homo)], 0.28, 4.0).unwrap());
        let ten: Vec<_> = (homo - 9..=homo).map(|i| (&wfn.alpha, i)).collect();
        time("10 orbitals, Medium", || generate_mo_grids(&wfn.basis, &ten, 0.28, 4.0).unwrap());
        time("1 orbital (HOMO), High", || generate_mo_grids(&wfn.basis, &[(&wfn.alpha, homo)], 0.15, 4.0).unwrap());
        // Times a fragment's atomic weight (TFVC, equal radii: the cost
        // doesn't depend on the ratios), the fragment being the first atom.
        let coordinates: Vec<[f64; 3]> = wfn.basis.shells.iter().fold(Vec::new(), |mut centers, shell| {
            if centers.last() != Some(&shell.center.to_array()) {
                centers.push(shell.center.to_array());
            }
            centers
        });
        let n = coordinates.len();
        let cells = AtomCells::new(&AtomicWeights { profile: "tfvc".to_string(), stiffness: 4, radius_ratios: vec![vec![1.0; n]; n] }, &coordinates).unwrap();
        time("1 orbital (HOMO) times a fragment weight, Medium", || generate_weighted_mo_grids(&wfn.basis, &[(&wfn.alpha, homo)], &cells, &[Some(vec![0])], 0.28, 4.0).unwrap());
        let groups: Vec<_> = ten.iter().map(|_| Some(vec![0])).collect();
        time("10 orbitals times a fragment weight, Medium", || generate_weighted_mo_grids(&wfn.basis, &ten, &cells, &groups, 0.28, 4.0).unwrap());
        let radii: Vec<f32> = molecule.atomic_numbers.iter().map(|&z| element_data(z).vdw_radius * 0.24 + 0.5).collect();
        time("envelope, all atoms as one fragment", || envelope_mesh(&molecule.positions, &radii));
        let matrix = time("read density matrix (SCF)", || read_fchk_density(&path, "SCF").unwrap());
        time("natural orbitals (diagonalization)", || natural_orbitals(&wfn.alpha, &matrix.total).unwrap());
    }
}
