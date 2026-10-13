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
    harness.get_by_label_contains("Nothing open yet");
    assert!(toasts(&harness).is_empty());
}

#[test]
fn a_fchk_without_results_opens_quietly() {
    let harness = app_with(vec![repo_dir().join("TESTS-VISUALIZER/H2O-631G.fchk")]);
    assert_eq!(harness.state().structure_labels(), vec!["H2O-631G.fchk"]);
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
    assert_eq!(harness.state().structure_labels(), vec!["H2O-TPSS.fchk"]);
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
    assert_eq!(harness.state().structure_labels(), vec!["H2O-Dimer-RHF.apost.json"]);
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
    assert_eq!(harness.state().structure_labels(), vec!["CH3F.fchk"]);
    harness.get_by_label("CH3F.apost.json");
}

#[test]
fn fragments_are_listed_and_select_their_atoms() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    harness.get_by_label("Fragments (7)");
    harness.get_by_label("Color atoms by fragment");
    harness.get_by_label("1  Fe   Fe1");
    harness.get_by_label("7  NO   N12, O13").click();
    harness.run_steps(2);
    assert_eq!(harness.state().selected_atoms(), vec![11, 12]);
    // Cmd/Ctrl-click adds a fragment to the selection.
    harness.get_by_label("1  Fe   Fe1").click_modifiers(egui::Modifiers::COMMAND);
    harness.run_steps(2);
    assert_eq!(harness.state().selected_atoms(), vec![11, 12, 0]);
    harness.get_by_label("1  Fe   Fe1").click();
    harness.run_steps(2);
    assert_eq!(harness.state().selected_atoms(), vec![0]);
}

#[test]
fn atoms_in_no_fragment_are_called_out() {
    let Some(fixtures) = fixtures_dir() else { return };
    let dir = scratch_dir("unassigned");
    std::fs::copy(fixtures.join("CH3F.fchk"), dir.join("CH3F.fchk")).unwrap();
    let text = std::fs::read_to_string(fixtures.join("CH3F.apost.json")).unwrap().replace("\"atoms\": [1, 3, 4, 5]", "\"atoms\": [3]");
    std::fs::write(dir.join("CH3F.apost.json"), text).unwrap();
    let harness = app_with(vec![dir.join("CH3F.fchk")]);
    harness.get_by_label("In no fragment: C1, H4, H5");
}

#[test]
fn a_run_without_fragments_says_so() {
    let Some(dir) = fixtures_dir() else { return };
    let harness = app_with(vec![dir.join("C2H6-B3LYP.fchk")]);
    harness.get_by_label("Fragments (0)");
    harness.get_by_label("This run defines no fragments.");
}

fn testdata(name: &str) -> std::path::PathBuf {
    repo_dir().join("TESTS-VISUALIZER").join(name)
}

#[test]
fn cubes_of_one_molecule_become_surfaces_of_one_structure() {
    let mut harness = app_with(vec![testdata("H2O-ccpVDZ-HOMO.cube"), testdata("H2O-ccpVDZ-LUMO.cube")]);
    harness.get_by_label("Surfaces").click();
    harness.run_steps(3);
    harness.get_by_label("Surfaces (2)");
    // One structure, named after the first cube, which is the one shown.
    assert_eq!(harness.state().structure_labels(), vec!["H2O-ccpVDZ-HOMO.cube"]);
    assert_eq!(
        harness.state().surface_labels(),
        vec![("H2O-ccpVDZ-HOMO.cube".to_string(), true), ("H2O-ccpVDZ-LUMO.cube".to_string(), false)]
    );
}

#[test]
fn cubes_attach_to_the_fchk_they_belong_to() {
    let mut harness = app_with(vec![testdata("H2O-ccpVDZ.fchk"), testdata("H2O-ccpVDZ-HOMO.cube")]);
    harness.get_by_label("Surfaces").click();
    harness.run_steps(3);
    harness.get_by_label("Surfaces (1)");
    assert_eq!(harness.state().structure_labels(), vec!["H2O-ccpVDZ.fchk"]);
    assert_eq!(harness.state().surface_labels(), vec![("H2O-ccpVDZ-HOMO.cube".to_string(), true)]);
}

#[test]
fn generated_orbitals_are_surfaces_of_the_same_structure() {
    let mut harness = app_with(vec![testdata("H2O-631G.fchk")]);
    harness.get_by_label("Surfaces").click();
    harness.run_steps(2);
    harness.get_by_label("HOMO/LUMO").click();
    harness.run_steps(2);
    harness.get_by_label("Generate 2 orbital(s)").click();
    harness.run_steps(3);
    harness.get_by_label("Surfaces (2)");
    // Surfaces of the same structure, the first shown alone.
    assert_eq!(harness.state().structure_labels(), vec!["H2O-631G.fchk"]);
    assert_eq!(harness.state().surface_labels(), vec![("HOMO".to_string(), true), ("LUMO".to_string(), false)]);
    assert!(toasts(&harness).iter().any(|m| m == "2 orbitals generated"));
}

#[test]
fn structures_can_be_closed() {
    let mut harness = app_with(vec![testdata("H2O-631G.fchk"), testdata("H2O-ccpVDZ.fchk")]);
    harness.get_all_by_label("\u{d7}").next().unwrap().click();
    harness.run_steps(2);
    assert_eq!(harness.state().structure_labels(), vec!["H2O-ccpVDZ.fchk"]);
    harness.get_by_label("\u{d7}").click();
    harness.run_steps(2);
    assert!(harness.state().structure_labels().is_empty());
    harness.get_by_label_contains("Nothing open yet");
}

/// A selection made anywhere (here: a fragment in the APOST-3D window) is
/// acted on from the Molecule window, which sends a measurement to the
/// Measure window.
#[test]
fn a_selection_can_be_measured_from_the_molecule_window() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("CH3F.fchk")]);
    harness.get_by_label("2  CH3   C1, H3, H4, H5").click();
    harness.run_steps(2);
    harness.get_by_label("Selected: C1, H3, H4, H5");
    harness.get_by_label("Add measurement").click();
    harness.run_steps(3);
    harness.get_by_label_contains("C1-H3-H4-H5: ");
}
