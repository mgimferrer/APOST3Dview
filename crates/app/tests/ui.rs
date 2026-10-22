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
fn stays_dark_when_the_system_is_light() {
    let mut harness = egui_kittest::Harness::builder()
        .with_theme(egui::Theme::Light)
        .renderer(common::AppRenderer::new())
        .build_eframe(|cc| apost3dview::App::with_options(cc, apost3dview::StartupOptions { files: Vec::new(), splash: false }));
    harness.run_steps(2);
    assert_eq!(harness.ctx.theme(), egui::Theme::Dark);
}

#[test]
fn a_fchk_without_results_opens_quietly() {
    let harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
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
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.apost.json")]);
    assert!(toasts(&harness).iter().any(|m| m.contains("APOST-3D results loaded from FeCN5NO3--UBLYP.apost.json")));
    // The one-line summary; the details fold out under "Run".
    harness.get_by_label("LOWDIN  \u{b7}  KS-DFT, Unrestricted  \u{b7}  7 fragments");
    harness.get_by_label("Run").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("FeCN5NO3--UBLYP.apost.json");
    harness.get_by_label("Lowdin (LOWDIN), Hilbert space");
    harness.get_by_label("199");
    harness.get_by_label_contains("version 5.0 (20");
    harness.get_by_label_contains(" (UTC");
    harness.get_by_label("KS-DFT, Unrestricted");
}

#[test]
fn a_stopped_run_shows_its_stop_message() {
    let Some(dir) = fixtures_dir() else { return };
    let harness = app_with(vec![dir.join("H2O-WB97X.apost.json")]);
    assert_eq!(harness.state().structure_labels(), vec!["H2O-WB97X.fchk"]);
    harness.get_by_label("APOST-3D stopped before the end of this run");
    harness.get_by_label("RANGE-SEPARATED HYBRID NOT SUPPORTED. REVISE inp");
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
    let mut harness = app_with(vec![dir.join("dimer.fchk")]);
    harness.get_by_label("APOST-3D results...").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Open these results").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Results not loaded");
    // The reason stays in the APOST-3D window, and is also in the warning.
    assert_eq!(harness.query_all_by_label_contains("dimer.apost.json does not belong to dimer.fchk").count(), 2);
    assert!(toasts(&harness).iter().any(|m| m.contains("energy") && m.ends_with("The results were not loaded.")));
}

#[test]
fn opening_results_for_an_open_fchk_attaches_to_it() {
    let Some(dir) = fixtures_dir() else { return };
    // Opened together: the .fchk first, then its results file, which must
    // attach to the open .fchk rather than open it a second time.
    let mut harness = app_with(vec![dir.join("CH3F.fchk"), dir.join("CH3F.apost.json")]);
    assert_eq!(harness.state().structure_labels(), vec!["CH3F.fchk"]);
    harness.get_by_label("Run").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("CH3F.apost.json");
}

/// A .fchk opens as the calculation alone: results APOST-3D wrote next to
/// it are offered in the APOST-3D window, and loaded only on request.
#[test]
fn a_fchk_opens_without_its_results_which_are_offered() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("CH3F.fchk")]);
    assert!(toasts(&harness).is_empty(), "{:?}", toasts(&harness));
    assert!(!harness.state().window_expanded(&harness.ctx, "APOST-3D"), "the APOST-3D window stays closed");
    harness.get_by_label("APOST-3D results...").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("APOST-3D results of this calculation were found next to it:");
    harness.get_by_label("CH3F.apost.json");
    assert!(harness.query_by_label("Fragments (2)").is_none());
    harness.get_by_label("Open these results").click_accesskit();
    harness.run_steps(2);
    assert!(toasts(&harness).iter().any(|m| m == "APOST-3D results loaded from CH3F.apost.json"), "{:?}", toasts(&harness));
    harness.get_by_label("Fragments (2)");

    // Without results next to it: the window opens on a new input for it,
    // and its Results tab says how to see results.
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
    harness.get_by_label("APOST-3D").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Input for H2O-631G.fchk");
    harness.get_by_label("Results").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("No APOST-3D results loaded.");
}

#[test]
fn fragments_are_listed_and_select_their_atoms() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.apost.json")]);
    harness.get_by_label("Fragments (7)");
    harness.get_by_label("1  Fe   Fe1");
    harness.get_by_label("7  NO   N12, O13").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().selected_atoms(), vec![11, 12]);
    // Cmd/Ctrl-click adds a fragment to the selection.
    harness.get_by_label("1  Fe   Fe1").click_modifiers(egui::Modifiers::COMMAND);
    harness.run_steps(2);
    assert_eq!(harness.state().selected_atoms(), vec![11, 12, 0]);
    harness.get_by_label("1  Fe   Fe1").click_accesskit();
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
    let harness = app_with(vec![dir.join("CH3F.apost.json")]);
    harness.get_by_label("In no fragment: C1, H4, H5");
}

#[test]
fn a_run_without_fragments_says_so() {
    let Some(dir) = fixtures_dir() else { return };
    let harness = app_with(vec![dir.join("C2H6-B3LYP.apost.json")]);
    harness.get_by_label("Fragments (0)");
    harness.get_by_label("This run defines no fragments.");
}

fn testdata(name: &str) -> std::path::PathBuf {
    repo_dir().join("validation/gaussian").join(name)
}

#[test]
fn cubes_of_one_molecule_become_surfaces_of_one_structure() {
    let mut harness = app_with(vec![testdata("H2O-ccpVDZ-HOMO.cube"), testdata("H2O-ccpVDZ-LUMO.cube")]);
    harness.get_by_label("Surfaces").click_accesskit();
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
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(3);
    harness.get_by_label("Surfaces (1)");
    assert_eq!(harness.state().structure_labels(), vec!["H2O-ccpVDZ.fchk"]);
    assert_eq!(harness.state().surface_labels(), vec![("H2O-ccpVDZ-HOMO.cube".to_string(), true)]);
}

#[test]
fn generated_orbitals_are_surfaces_of_the_same_structure() {
    let mut harness = app_with(vec![testdata("H2O-631G.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("HOMO/LUMO").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
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
    harness.get_all_by_label("\u{d7}").next().unwrap().click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().structure_labels(), vec!["H2O-ccpVDZ.fchk"]);
    harness.get_by_label("\u{d7}").click_accesskit();
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
    let mut harness = app_with(vec![dir.join("CH3F.apost.json")]);
    harness.get_by_label("2  CH3   C1, H3, H4, H5").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Selected: C1, H3, H4, H5");
    harness.get_by_label("Add measurement").click_accesskit();
    harness.run_steps(3);
    harness.get_by_label_contains("C1-H3-H4-H5: ");
}

/// Asking for a window again (toolbar button or a link in the Molecule
/// window) brings it back expanded, even if it had been collapsed; a click
/// on the toolbar button of an open window closes it.
#[test]
fn windows_asked_for_again_come_back_expanded() {
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
    let collapse = |harness: &mut egui_kittest::Harness<'_, apost3dview::App>| {
        harness.state().set_window_collapsed(&harness.ctx, "Surfaces", true);
        harness.run_steps(2);
        assert!(!harness.state().window_expanded(&harness.ctx, "Surfaces"));
    };
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    assert!(harness.state().window_expanded(&harness.ctx, "Surfaces"));

    // Collapsed, then asked for from the Molecule window's link.
    collapse(&mut harness);
    harness.get_by_label("Orbitals...").click_accesskit();
    harness.run_steps(2);
    assert!(harness.state().window_expanded(&harness.ctx, "Surfaces"));

    // Collapsed, closed from the toolbar, opened again: expanded.
    collapse(&mut harness);
    harness.get_all_by_label("Surfaces").next().unwrap().click_accesskit();
    harness.run_steps(2);
    assert!(!harness.state().window_expanded(&harness.ctx, "Surfaces"));
    harness.get_all_by_label("Surfaces").next().unwrap().click_accesskit();
    harness.run_steps(2);
    assert!(harness.state().window_expanded(&harness.ctx, "Surfaces"));
}

/// Tool windows stack at the top right in toolbar order, one below the
/// other; collapsing one moves the ones below up; opening a long section
/// (the text report) does not change a window's size; dragging a window by
/// its title takes it out of the column, and "Arrange windows" puts it
/// back.
#[test]
fn tool_windows_stack_in_a_column() {
    use egui::{Event, PointerButton};
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.apost.json")]);
    harness.get_by_label("Measure").click_accesskit();
    harness.run_steps(4);
    let rect = |harness: &egui_kittest::Harness<'_, apost3dview::App>, title: &str| harness.state().window_rect(title).unwrap();
    let screen_right = harness.ctx.content_rect().right();
    assert!(harness.query_by_label("Arrange windows").is_none(), "the default layout");
    let (molecule, measure, apost) = (rect(&harness, "Molecule"), rect(&harness, "Measure"), rect(&harness, "APOST-3D"));
    for r in [molecule, measure, apost] {
        assert!((r.width() - 400.0).abs() < 1.0, "all the same width: {r:?}");
    }
    for r in [molecule, measure, apost] {
        assert!((r.right() - (screen_right - 8.0)).abs() < 1.5, "right-aligned: {r:?}");
    }
    assert!(molecule.bottom() <= measure.top() && measure.bottom() <= apost.top(), "in order, no overlap: {molecule:?} {measure:?} {apost:?}");

    // Collapsing Molecule moves the others up.
    harness.state().set_window_collapsed(&harness.ctx, "Molecule", true);
    harness.run_steps(4);
    assert!(rect(&harness, "Measure").top() < measure.top() - 50.0, "moved up");

    // The text report opens inside the window, which keeps its size.
    let before = rect(&harness, "APOST-3D").size();
    harness.get_by_label_contains("Text report").click_accesskit();
    harness.run_steps(4);
    assert_eq!(rect(&harness, "APOST-3D").size(), before);

    // Dragged by its title: out of the column, until "Arrange windows".
    let title = rect(&harness, "Measure").center_top() + egui::vec2(0.0, 12.0);
    harness.event(Event::PointerMoved(title));
    harness.event(Event::PointerButton { pos: title, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
    harness.run_steps(1);
    for step in 1..=10 {
        harness.event(Event::PointerMoved(title + egui::vec2(-30.0 * step as f32, 20.0 * step as f32)));
        harness.run_steps(1);
    }
    let end = title + egui::vec2(-300.0, 200.0);
    harness.event(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
    harness.run_steps(3);
    let moved = rect(&harness, "Measure");
    assert!(moved.left() < screen_right - 600.0, "the window followed the drag: {moved:?}");
    harness.run_steps(3);
    assert_eq!(rect(&harness, "Measure"), moved, "and stays where it was put");
    harness.get_by_label("Arrange windows").click_accesskit();
    harness.run_steps(4);
    assert!((rect(&harness, "Measure").right() - (screen_right - 8.0)).abs() < 1.5, "back in the column");
    assert!(harness.query_by_label("Arrange windows").is_none());

    // Widened by its left border: still in the column, wider; Arrange
    // brings back the default width.
    let edge = rect(&harness, "Measure").left_center();
    harness.event(Event::PointerMoved(edge));
    harness.event(Event::PointerButton { pos: edge, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
    harness.run_steps(1);
    for step in 1..=10 {
        harness.event(Event::PointerMoved(edge - egui::vec2(10.0 * step as f32, 0.0)));
        harness.run_steps(1);
    }
    harness.event(Event::PointerButton { pos: edge - egui::vec2(100.0, 0.0), button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
    harness.run_steps(3);
    let widened = rect(&harness, "Measure");
    assert!(widened.width() > 450.0, "wider: {widened:?}");
    assert!((widened.right() - (screen_right - 8.0)).abs() < 1.5, "still in the column: {widened:?}");
    harness.get_by_label("Arrange windows").click_accesskit();
    harness.run_steps(4);
    assert!((rect(&harness, "Measure").width() - 400.0).abs() < 1.0, "default width again");
}
