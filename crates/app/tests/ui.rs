//! Drives the real `App` headlessly (egui_kittest, with a real wgpu device)
//! and checks what the windows show after opening files: opening and
//! recognizing each kind of file, and attaching APOST-3D results.
//!
//! Tests that need the APOST-3D fixtures (`fixtures/apost3d/`, copied by
//! `scripts/sync-apost3d-fixtures.sh`, not tracked by git) print a notice
//! and pass when they are missing.

mod common;

use egui_kittest::kittest::Queryable;

use common::{app_with, fixtures_dir, repo_dir, scratch_dir, toasts};

#[test]
fn starts_with_an_open_menu_and_no_structures() {
    let harness = app_with(Vec::new());
    harness.get_by_label("Open");
    harness.get_by_label_contains("No structures loaded yet");
    assert!(toasts(&harness).is_empty());
}

#[test]
fn a_fchk_without_results_opens_quietly() {
    let harness = app_with(vec![repo_dir().join("TESTS-VISUALIZER/H2O-631G.fchk")]);
    harness.get_by_label("H2O-631G.fchk");
    assert!(toasts(&harness).is_empty(), "no warning for a .fchk without results: {:?}", toasts(&harness));
}

#[test]
fn unsupported_files_are_explained() {
    let dir = scratch_dir("unsupported");
    let input = dir.join("job.inp");
    std::fs::write(&input, "# METHOD\nTFVC\n#\n").unwrap();
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "hello").unwrap();
    let harness = app_with(vec![input, notes]);
    let messages = toasts(&harness);
    assert!(messages.iter().any(|m| m.contains("job.inp is not a Molden file")), "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("notes.txt: not a supported file type")), "{messages:?}");
}

#[test]
fn a_fchk_picks_up_its_results_and_shows_the_run() {
    let Some(dir) = fixtures_dir() else { return };
    let harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    assert!(toasts(&harness).iter().any(|m| m.contains("APOST-3D results loaded from FeCN5NO3--UBLYP.apost.json")));
    harness.get_by_label("FeCN5NO3--UBLYP.apost.json");
    harness.get_by_label("Lowdin (LOWDIN), Hilbert space");
    harness.get_by_label("199");
    harness.get_by_label_contains("Unrestricted");
}

#[test]
fn a_stopped_run_shows_its_stop_message() {
    let Some(dir) = fixtures_dir() else { return };
    let harness = app_with(vec![dir.join("H2O-TPSS.apost.json")]);
    harness.get_by_label("H2O-TPSS.fchk");
    harness.get_by_label("APOST-3D stopped before the end of this run");
    harness.get_by_label("META-GGA FUNCTIONALS NOT YET SUPPORTED");
}

#[test]
fn results_without_their_fchk_show_the_stored_geometry_with_a_warning() {
    let Some(fixtures) = fixtures_dir() else { return };
    let dir = scratch_dir("no-fchk");
    let json = dir.join("H2O-Dimer-RHF.apost.json");
    std::fs::copy(fixtures.join("H2O-Dimer-RHF.apost.json"), &json).unwrap();
    let harness = app_with(vec![json]);
    assert!(toasts(&harness).iter().any(|m| m.starts_with("No .fchk found for H2O-Dimer-RHF.apost.json")), "{:?}", toasts(&harness));
    harness.get_by_label("No .fchk found next to the results file");
    // The structure is listed under the results file's name.
    assert_eq!(harness.query_all_by_label("H2O-Dimer-RHF.apost.json").count(), 2);
}

#[test]
fn results_from_another_calculation_are_refused_with_the_reason() {
    let Some(fixtures) = fixtures_dir() else { return };
    // Same geometry and basis set, different method: only the energy differs.
    let dir = scratch_dir("mismatch");
    std::fs::copy(fixtures.join("H2O-Dimer-B3LYP.fchk"), dir.join("dimer.fchk")).unwrap();
    std::fs::copy(fixtures.join("H2O-Dimer-RHF.apost.json"), dir.join("dimer.apost.json")).unwrap();
    let harness = app_with(vec![dir.join("dimer.fchk")]);
    harness.get_by_label("Results not loaded");
    // The reason stays in the APOST-3D window, and is also in the warning.
    assert_eq!(harness.query_all_by_label_contains("dimer.apost.json does not belong to dimer.fchk").count(), 2);
    assert!(toasts(&harness).iter().any(|m| m.contains("energy") && m.ends_with("The results were not loaded.")));
}

#[test]
fn opening_results_for_an_open_fchk_attaches_to_it() {
    let Some(dir) = fixtures_dir() else { return };
    // Opened together: the .fchk first (it finds its results itself), then
    // the results file, which must not open the .fchk a second time.
    let harness = app_with(vec![dir.join("CH3F.fchk"), dir.join("CH3F.apost.json")]);
    assert_eq!(harness.query_all_by_label("CH3F.fchk").count(), 1);
    harness.get_by_label("CH3F.apost.json");
}
