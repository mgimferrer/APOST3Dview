//! Electron and spin densities as surfaces: offered next to the orbitals,
//! the spin density only for an open shell, generated with their own
//! isovalues, and kept by sessions.

mod common;

use egui_kittest::kittest::{By, Queryable};

use common::{app_with, fixtures_dir, repo_dir, scratch_dir, screenshot, toasts};

fn testdata(name: &str) -> std::path::PathBuf {
    repo_dir().join("validation/gaussian").join(name)
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

/// Opens the menu showing `value` and picks `option`.
fn choose(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, value: &str, option: &str) {
    harness
        .get_all(By::new().role(egui::accesskit::Role::ComboBox))
        .find(|n| n.value().as_deref() == Some(value))
        .unwrap_or_else(|| panic!("no menu showing {value}"))
        .click_accesskit();
    harness.run_steps(2);
    harness.get_all_by_label(option).last().unwrap().click_accesskit();
    harness.run_steps(2);
}

/// A CASSCF 1-RDM written by apost3d.py into "Total SCF Density": the
/// density comes from it (through its natural orbitals), not from the
/// file's orbitals; no spin density for the singlet (its spin block is
/// zero); the natural orbitals are a set of their own in the Orbitals
/// menu, the two either side of the occupied ones ticked; sessions keep
/// both.
#[test]
fn a_correlated_density_and_its_natural_orbitals() {
    let Some(dir) = fixtures_dir() else { return };
    let scratch = scratch_dir("natural-orbitals-session");
    let session = scratch.join("cas.a3dview");
    {
        let mut harness = app_with(vec![dir.join("H2O-CAS44.fchk")]);
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("The CASSCF density of the .fchk, through its natural orbitals.");
        harness.get_by_label_contains("This file stores no orbital energies");
        assert_eq!(harness.query_all_by_label("Spin density").count(), 0);
        harness.get_by_label("Clear").click_accesskit();
        harness.get_by_label("Total density").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Generate 1 surface(s)").click_accesskit();
        harness.run_steps(4);
        assert_eq!(harness.state().surface_labels(), vec![("Total density (CASSCF)".to_string(), true)]);
        screenshot(&mut harness, "density-casscf");

        choose(&mut harness, "Orbitals of the file", "Natural orbitals (CASSCF)");
        harness.get_by_label_contains("NO 1  occupation 2.0000");
        harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
        harness.run_steps(4);
        let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec!["Total density (CASSCF)", "NO 5 (CASSCF)", "NO 6 (CASSCF)"]);
        assert!(toasts(&harness).iter().any(|m| m == "2 CASSCF natural orbitals generated"), "{:?}", toasts(&harness));
        screenshot(&mut harness, "natural-orbital-casscf");
        harness.state_mut().save_session_to(&session).unwrap();
    }
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("density_matrix = \"SCF\"") && text.contains("orbital_set = \"natural-SCF\""), "{text}");
    let harness = app_with(vec![session]);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["Total density (CASSCF)", "NO 5 (CASSCF)", "NO 6 (CASSCF)"]);
    assert_eq!(toasts(&harness), vec!["Session cas.a3dview opened".to_string()]);
}

/// Gaussian's MP2 density ("Total MP2 Density") is the default; the SCF
/// one (from the occupied orbitals) is a choice away, and both can be
/// shown together.
#[test]
fn mp2_or_scf_density() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-MP2-DENS2.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Clear").click_accesskit();
    harness.get_by_label("Total density").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 1 surface(s)").click_accesskit();
    harness.run_steps(4);
    choose(&mut harness, "MP2", "SCF");
    harness.get_by_label("Generate 1 surface(s)").click_accesskit();
    harness.run_steps(4);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["Total density (MP2)", "Total density"]);
    // The SCF orbitals stay SCF orbitals.
    harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).find(|n| n.value().as_deref() == Some("SCF orbitals")).expect("Orbitals menu");
}

/// A mean-field file offers no density choice and no natural orbitals.
#[test]
fn a_mean_field_file_has_no_density_choice() {
    let mut harness = app_with(vec![testdata("H2O-T-ccpVTZ.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.query_all(By::new().role(egui::accesskit::Role::ComboBox)).filter(|n| n.value().as_deref() == Some("SCF")).count(), 0);
    assert_eq!(harness.query_all_by_label_contains("Natural orbitals").count(), 0);
}

/// With APOST-3D results attached, the density drawn is the one APOST-3D
/// analysed: the block the results file names (`wavefunction.density`,
/// schema 0.17), or, in an older file, the `DENS n` of its input. A Gaussian
/// MP2 file analysed without `DENS 2` shows its SCF density.
#[test]
fn the_density_apost3d_analysed_is_drawn() {
    let Some(dir) = fixtures_dir() else { return };
    let job = "H2O-MP2-DENS2";
    let original: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{job}.apost.json"))).unwrap()).unwrap();
    if original.pointer("/wavefunction/density").is_none() {
        eprintln!("{job} predates APOST-3D 0.17 (no wavefunction.density): test skipped");
        return;
    }
    let mut scf = original.clone();
    scf["wavefunction"]["density"] = serde_json::json!({"fchk_block": "Total SCF Density", "index": 1, "source": "fchk"});
    let mut older = original.clone();
    older["wavefunction"].as_object_mut().unwrap().remove("density");
    for (case, results, expected) in [("mp2", &original, "Total density (MP2)"), ("scf", &scf, "Total density"), ("dens-2", &older, "Total density (MP2)")] {
        let folder = scratch_dir(&format!("analysed-density-{case}"));
        std::fs::copy(dir.join(format!("{job}.fchk")), folder.join(format!("{job}.fchk"))).unwrap();
        std::fs::write(folder.join(format!("{job}.apost.json")), serde_json::to_vec(results).unwrap()).unwrap();
        let mut harness = app_with(vec![folder.join(format!("{job}.apost.json"))]);
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Clear").click_accesskit();
        harness.get_by_label("Total density").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Generate 1 surface(s)").click_accesskit();
        harness.run_steps(4);
        let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec![expected.to_string()], "{case}");
    }
}
