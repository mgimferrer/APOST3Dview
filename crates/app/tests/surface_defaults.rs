//! New surfaces start from the default settings: an isovalue edited on
//! one surface doesn't carry over to the next ones unless asked for
//! ("Use for new surfaces"); each surface's isovalue is in the list; EFOs
//! the orbital file reproduces poorly are marked approximate (APOST-3D
//! session, 2026-10-19).

mod common;

use egui::accesskit::{Action, ActionData, ActionRequest, Role};
use egui_kittest::kittest::{By, NodeT, Queryable};

use common::{app_with, repo_dir, screenshot};

/// Sets the selected surface's isovalue field to `value`, as a screen
/// reader would (accessibility SetValue: a DragValue can't be typed into
/// headlessly).
fn set_isovalue(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, value: f64) {
    let field = harness.get_all(By::new().role(Role::SpinButton)).find(|n| n.value().as_deref() == Some("0.075")).expect("isovalue field");
    let (target_node, target_tree) = field.accesskit_node().locate();
    harness.event(egui::Event::AccessKitActionRequest(ActionRequest { action: Action::SetValue, target_node, target_tree, data: Some(ActionData::NumericValue(value)) }));
    harness.run_steps(3);
}

#[test]
fn an_edited_isovalue_stays_with_its_surface() {
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    // Tick the HOMO (nothing is ticked on opening).
    harness.get_all(egui_kittest::kittest::By::new().role(egui::accesskit::Role::CheckBox).label_contains("Hartree")).find(|n| format!("{n:?}").contains("HOMO")).expect("the HOMO's row").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 1 orbital(s)").click_accesskit();
    harness.run_steps(3);
    harness.get_by_label("\u{b1}0.075");
    set_isovalue(&mut harness, 0.02);
    harness.get_by_label("\u{b1}0.02");

    // The LUMO starts at the default, not at the HOMO's 0.02.
    harness.get_by_label("Clear").click_accesskit();
    harness.get_by_label("HOMO/LUMO").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.query_all_by_label_contains("New orbitals and cubes start at").count(), 0);
    let labels = harness.state().surface_labels();
    assert_eq!(labels.len(), 2, "{labels:?}");
    // HOMO regenerated keeps its own 0.02; the LUMO has 0.075.
    harness.get_by_label("\u{b1}0.02");
    harness.get_by_label("\u{b1}0.075");

    // On request, the next surfaces start from a surface's settings.
    harness.get_all_by_label("HOMO").last().unwrap().click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Use for new surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("New orbitals and cubes start at isovalue 0.02.");
    harness.get_by_label("Reset").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.query_all_by_label_contains("New orbitals and cubes start at").count(), 0);
}

#[test]
fn efos_say_how_closely_the_file_reproduces_them() {
    let dir = repo_dir().join("fixtures/apost3d-orbitals");
    if !dir.is_dir() {
        eprintln!("fixtures/apost3d-orbitals/ not found; test skipped");
        return;
    }
    let mut harness = app_with(vec![dir.join("NaBH3--B3LYP-GEOS.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    let combo = harness.get_all(By::new().role(Role::ComboBox)).find(|n| n.value().as_deref() == Some("SCF orbitals")).expect("Orbitals menu");
    combo.click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("GEOS EFOs").click_accesskit();
    harness.run_steps(2);
    assert!(harness.get_all_by_label_contains("fit 100%").count() >= 1);
    // EFO 1 (fit 100%) and EFO 14 (fit 52%, approximate).
    harness.state_mut().select_set_orbitals(&[0, 13], &[]);
    harness.run_steps(2);
    harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
    harness.run_steps(3);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels.len(), 2, "{labels:?}");
    harness.get_all_by_label(&labels[0]).next().unwrap().click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.query_all_by_label_contains("Approximate shape").count(), 0);
    harness.get_all_by_label(&labels[1]).next().unwrap().click_accesskit();
    harness.run_steps(2);
    // With APOST-3D's unweighted file (0.17) the EFO is drawn exactly: no
    // note until the fitted file is drawn as stored.
    if harness.query_by_label("Advanced").is_some() {
        assert_eq!(harness.query_all_by_label_contains("Approximate shape").count(), 0, "drawn exactly");
        harness.get_by_label("Advanced").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Draw as stored in the .fchk").click_accesskit();
        harness.run_steps(4);
    }
    harness.get_by_label_contains("reproduces this EFO to 52% only");
    screenshot(&mut harness, "efo-approximate");
}

/// A surface cut flat by the edge of its grid's box says so: a cube at a
/// low isovalue reaches the box; at the default it doesn't.
#[test]
fn a_surface_reaching_the_edge_of_its_box_says_so() {
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-ccpVDZ-LUMO.cube")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.query_all_by_label_contains("reaches the edge").count(), 0, "not at the default isovalue");
    set_isovalue(&mut harness, 0.0005);
    harness.get_by_label_contains("reaches the edge of the cube file's box");
}
