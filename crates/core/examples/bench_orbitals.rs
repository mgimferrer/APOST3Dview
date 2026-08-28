//! Timing check for `generate_mo_grids` against a real, large `.fchk` —
//! added after Martí reported "Generate orbitals" taking effectively
//! forever on `TESTS-VISUALIZER/Bi-dianion-OSD.fchk` (83 atoms, 698
//! contracted shells, 1844 basis functions), the first genuinely large/
//! heavy molecule the GTO evaluator had been run against (prior validation
//! was BiCl3 and H2O, both a handful of atoms). Not a correctness check —
//! `compare_cube` already covers that — just wall-clock, so a future
//! change that silently reintroduces an O(orbitals) or single-threaded
//! regression here shows up as a number, not a support ticket.

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use apost3dview_core::{generate_mo_grids, parse_fchk_wavefunction};

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("TESTS-VISUALIZER/Bi-dianion-OSD.fchk"));
    let spacing_bohr: f64 = env::args().nth(2).map(|s| s.parse().expect("spacing must be a number")).unwrap_or(0.28); // Medium preset, app.rs's ORBITAL_SPACING_MEDIUM_BOHR
    let padding_bohr = 4.0; // app.rs's ORBITAL_GRID_PADDING_BOHR

    println!("Parsing {}...", path.display());
    let parse_start = Instant::now();
    let wfn = parse_fchk_wavefunction(&path).expect("failed to parse wavefunction");
    println!(
        "Parsed in {:.2}s — {} shells, {} basis functions, {} alpha orbitals",
        parse_start.elapsed().as_secs_f64(),
        wfn.basis.shells.len(),
        wfn.alpha.num_basis_functions,
        wfn.alpha.num_orbitals()
    );

    let homo = wfn.alpha.homo_index() - 1;
    let lumo = wfn.alpha.lumo_index() - 1;
    let requests = [(&wfn.alpha, homo - 1), (&wfn.alpha, homo), (&wfn.alpha, lumo)];
    println!(
        "\nGenerating {} orbital grid(s) at {spacing_bohr:.2} Bohr spacing (HOMO-1, HOMO, LUMO)...",
        requests.len()
    );

    let gen_start = Instant::now();
    let grids = generate_mo_grids(&wfn.basis, &requests, spacing_bohr, padding_bohr).expect("grid generation failed");
    let elapsed = gen_start.elapsed();

    let dims = grids[0].dims;
    let total_points = dims[0] * dims[1] * dims[2];
    println!(
        "Done in {:.2}s — grid {}x{}x{} = {} points/orbital, {:.1}M point-evaluations/s combined",
        elapsed.as_secs_f64(),
        dims[0],
        dims[1],
        dims[2],
        total_points,
        (total_points * requests.len()) as f64 / elapsed.as_secs_f64() / 1e6
    );

    println!("\nALL CHECKS PASSED");
}
