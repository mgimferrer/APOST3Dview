//! Electron and spin densities as surfaces: offered next to the orbitals,
//! the spin density only for an open shell, generated with their own
//! isovalues, and kept by sessions.

mod common;

use egui_kittest::kittest::Queryable;

use common::{app_with, fixtures_dir, repo_dir, scratch_dir, screenshot, toasts};

fn testdata(name: &str) -> std::path::PathBuf {
    repo_dir().join("TESTS-VISUALIZER").join(name)
}

#[test]
fn total_and_spin_density_of_a_triplet() {
    let scratch = scratch_dir("densities-session");
    let session = scratch.join("triplet.a3dview");
    {
        let mut harness = app_with(vec![testdata("H2O-T-ccpVTZ.fchk")]);
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        // The HOMO is ticked on opening: only the densities.
        harness.get_by_label("Clear").click_accesskit();
        harness.get_by_label("Total density").click_accesskit();
        harness.get_by_label("Spin density").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Generate 2 surface(s)").click_accesskit();
        harness.run_steps(4);
        assert_eq!(harness.state().surface_labels(), vec![("Total density".to_string(), true), ("Spin density".to_string(), false)]);
        assert!(toasts(&harness).iter().any(|m| m == "The total density and the spin density generated"), "{:?}", toasts(&harness));
        screenshot(&mut harness, "density-total");
        // The spin density alone.
        harness.get_all_by_label("Spin density").last().unwrap().click_accesskit();
        harness.run_steps(4);
        assert_eq!(harness.state().surface_labels(), vec![("Total density".to_string(), false), ("Spin density".to_string(), true)]);
        screenshot(&mut harness, "density-spin");
        harness.state_mut().save_session_to(&session).unwrap();
    }
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("kind = \"density\"") && text.contains("spin_density = true"), "{text}");
    let harness = app_with(vec![session]);
    assert_eq!(harness.state().surface_labels(), vec![("Total density".to_string(), false), ("Spin density".to_string(), true)]);
}

/// The total density starts translucent, the spin density opaque; the
/// Translucent checkbox switches either, Default goes back to the
/// density's own look, and none of it carries over to the next orbitals,
/// which start opaque.
#[test]
fn densities_start_translucent_and_switch_with_one_checkbox() {
    let mut harness = app_with(vec![testdata("H2O-T-ccpVTZ.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Clear").click_accesskit();
    harness.get_by_label("Total density").click_accesskit();
    harness.get_by_label("Spin density").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 2 surface(s)").click_accesskit();
    harness.run_steps(4);
    assert_eq!(harness.state().surface_opacities(), vec![0.35, 1.0]);

    // The total density (selected) made opaque, then translucent again.
    harness.get_by_label("Translucent").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().surface_opacities(), vec![1.0, 1.0]);
    harness.get_by_label("Translucent").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().surface_opacities(), vec![0.35, 1.0]);
    // Opaque again; Default brings back the density's translucency.
    harness.get_by_label("Translucent").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Default").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().surface_opacities(), vec![0.35, 1.0]);

    // The spin density made translucent the same way.
    harness.get_all_by_label("Spin density").last().unwrap().click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Translucent").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().surface_opacities(), vec![0.35, 0.35]);

    // Orbitals generated afterwards are opaque, and translucent at 0.5.
    // (The generation checkboxes come first; the surface list repeats the names.)
    harness.get_all_by_label("Total density").next().unwrap().click_accesskit();
    harness.get_all_by_label("Spin density").next().unwrap().click_accesskit();
    harness.get_by_label("HOMO/LUMO").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label_contains("Generate 4").click_accesskit();
    harness.run_steps(4);
    let opacities = harness.state().surface_opacities();
    assert_eq!(opacities.len(), 6, "{:?}", harness.state().surface_labels());
    assert_eq!(&opacities[2..], &[1.0, 1.0, 1.0, 1.0]);
    harness.get_by_label("Translucent").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().surface_opacities()[2], 0.5);
}

#[test]
fn a_closed_shell_offers_no_spin_density() {
    let mut harness = app_with(vec![testdata("H2O-631G.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Total density");
    assert_eq!(harness.query_all_by_label("Spin density").count(), 0);
    harness.get_by_label("Clear").click_accesskit();
    harness.get_by_label("Total density").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 1 surface(s)").click_accesskit();
    harness.run_steps(3);
    assert!(toasts(&harness).iter().any(|m| m == "The total density generated"), "{:?}", toasts(&harness));
}

#[test]
fn a_correlated_wavefunction_is_noted() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-CAS44.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label_contains("For this CASSCF wavefunction, the density is that of the occupied orbitals");
}
