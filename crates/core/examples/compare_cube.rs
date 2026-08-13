//! Validation tool for Phase 3's from-scratch GTO evaluator: loads a real
//! reference `.cube` (from Chemcraft/cubegen/APOST-3D) and a `.fchk`,
//! evaluates the *same* MO ourselves at every point of the reference
//! grid, and reports quantitative agreement — RMS and max absolute
//! difference, not just "looks similar". MOs are only defined up to an
//! overall sign, so this checks both signs and reports whichever matches.

use std::env;
use std::path::PathBuf;

use apost3dview_core::units::ANGSTROM_PER_BOHR;
use apost3dview_core::{evaluate_mo, parse_cube, parse_fchk_wavefunction};
use glam::DVec3;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: compare_cube <path.fchk> <reference.cube> <HOMO|LUMO|#N (1-based)> [ALPHA|BETA]");
        std::process::exit(1);
    }
    let fchk_path = PathBuf::from(&args[1]);
    let cube_path = PathBuf::from(&args[2]);
    let orbital_spec = args[3].to_uppercase();
    let spin = args.get(4).map(|s| s.to_uppercase()).unwrap_or_else(|| "ALPHA".to_string());

    let wfn = parse_fchk_wavefunction(&fchk_path).expect("failed to parse .fchk wavefunction");
    let reference = parse_cube(&cube_path).expect("failed to parse reference .cube");

    let orbitals = match spin.as_str() {
        "ALPHA" => &wfn.alpha,
        "BETA" => wfn.beta.as_ref().expect("this .fchk has no beta orbitals (restricted calculation)"),
        other => panic!("spin should be ALPHA or BETA, got '{other}'"),
    };

    let mo_index_0based = match orbital_spec.as_str() {
        "HOMO" => orbitals.homo_index() - 1,
        "LUMO" => orbitals.lumo_index() - 1,
        spec => spec.parse::<usize>().expect("orbital spec should be HOMO, LUMO, or a 1-based MO number") - 1,
    };
    println!(
        "Evaluating {spin} MO #{} (0-based #{mo_index_0based}), energy = {:.6}",
        mo_index_0based + 1,
        orbitals.orbital_energies[mo_index_0based]
    );

    let grid = &reference.grid;
    let [nx, ny, nz] = grid.dims;
    println!("Reference grid: {nx}x{ny}x{nz} ({} points)", nx * ny * nz);

    let mut scratch = Vec::new();
    let mut sum_sq_same = 0.0f64;
    let mut sum_sq_flipped = 0.0f64;
    let mut max_abs_same = 0.0f64;
    let mut max_abs_flipped = 0.0f64;
    let mut max_ref_abs = 0.0f64;
    let mut count = 0usize;

    for i in 0..nx {
        for j in 0..ny {
            for k in 0..nz {
                let reference_value = grid.value_at(i, j, k) as f64;
                let position_angstrom = grid.world_position(i as f32, j as f32, k as f32);
                let position_bohr = DVec3::new(position_angstrom.x as f64, position_angstrom.y as f64, position_angstrom.z as f64) / ANGSTROM_PER_BOHR;

                let ours = evaluate_mo(&wfn.basis, orbitals, mo_index_0based, position_bohr, &mut scratch).expect("evaluation failed");

                let diff_same = ours - reference_value;
                let diff_flipped = -ours - reference_value;
                sum_sq_same += diff_same * diff_same;
                sum_sq_flipped += diff_flipped * diff_flipped;
                max_abs_same = max_abs_same.max(diff_same.abs());
                max_abs_flipped = max_abs_flipped.max(diff_flipped.abs());
                max_ref_abs = max_ref_abs.max(reference_value.abs());
                count += 1;
            }
        }
    }

    let rms_same = (sum_sq_same / count as f64).sqrt();
    let rms_flipped = (sum_sq_flipped / count as f64).sqrt();
    let (rms, max_abs, sign_note) =
        if rms_same <= rms_flipped { (rms_same, max_abs_same, "same sign as reference") } else { (rms_flipped, max_abs_flipped, "opposite sign from reference (expected — MOs are only defined up to an overall sign)") };

    println!("\nMax |reference value| across the grid: {max_ref_abs:.6}");
    println!("RMS difference: {rms:.8} ({sign_note})");
    println!("Max absolute difference: {max_abs:.8}");
    println!("RMS as % of max reference magnitude: {:.4}%", 100.0 * rms / max_ref_abs);

    if rms / max_ref_abs < 0.01 {
        println!("\n✅ EXCELLENT MATCH (RMS < 1% of peak value)");
    } else if rms / max_ref_abs < 0.05 {
        println!("\n⚠️  ROUGH MATCH (RMS < 5% of peak value) — worth investigating");
    } else {
        println!("\n❌ POOR MATCH — something is wrong");
    }
}
