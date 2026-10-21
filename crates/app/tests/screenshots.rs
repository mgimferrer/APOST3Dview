//! Renders the whole window (UI and 3D view) headlessly in a few typical
//! states. Fails on any wgpu validation error in the viewport's pipelines
//! or the UI. With `APOST3DVIEW_SCREENSHOT_DIR` set, also saves the images,
//! to look at the UI while working on it.

mod common;

use common::{app_with, fixtures_dir, repo_dir, screenshot};

/// Not a single flat color: something was drawn.
fn has_content(image: &image::RgbaImage) -> bool {
    let first = image.get_pixel(0, 0);
    image.pixels().filter(|p| *p != first).count() > 1000
}

#[test]
fn empty_window() {
    let mut harness = app_with(Vec::new());
    assert!(has_content(&screenshot(&mut harness, "empty")));
}

#[test]
fn about_window_tells_where_to_report_problems() {
    use egui_kittest::kittest::Queryable;
    let mut harness = app_with(Vec::new());
    harness.get_by_label("About").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Report a problem or suggest a feature");
    harness.get_by_label_contains(&format!("APOST3Dview {} · ", env!("CARGO_PKG_VERSION")));
    assert!(has_content(&screenshot(&mut harness, "about")));
}

#[test]
fn molecule_and_orbital() {
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-ccpVDZ-HOMO.cube")]);
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "cube")));
}

#[test]
fn apost3d_results() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.apost.json")]);
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "apost3d")));
    let mut harness = app_with(vec![dir.join("FeO4-2.apost.json")]);
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "apost3d-fragments")));
    let mut harness = app_with(vec![dir.join("H2O-TPSS.apost.json")]);
    assert!(has_content(&screenshot(&mut harness, "apost3d-stopped")));
}

/// Fragments are shown by envelopes only: hiding them gives back exactly
/// the plain molecule, atoms in their element colors, and showing them
/// again gives the same picture as before.
#[test]
fn fragment_envelopes_leave_the_atoms_alone() {
    use egui_kittest::kittest::Queryable;
    let Some(dir) = fixtures_dir() else { return };
    let scratch = common::scratch_dir("envelopes-plain");
    std::fs::copy(dir.join("FeCN5NO3--UBLYP.fchk"), scratch.join("plain.fchk")).unwrap();
    // The same molecule with and without results, in one app: switching
    // between structures of the same geometry keeps the camera.
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.apost.json"), scratch.join("plain.fchk")]);
    harness.run_steps(3);
    harness.get_all_by_label("FeCN5NO3--UBLYP.fchk").next().unwrap().click_accesskit();
    harness.run_steps(3);
    assert!(harness.state_mut().show_on_molecule("fragments"));
    harness.run_steps(3);
    let with_envelopes = harness.state_mut().render_image(480, 360).unwrap();
    harness.get_all_by_label("plain.fchk").next().unwrap().click_accesskit();
    harness.run_steps(3);
    let plain_image = harness.state_mut().render_image(480, 360).unwrap();
    harness.get_all_by_label("FeCN5NO3--UBLYP.fchk").next().unwrap().click_accesskit();
    harness.run_steps(3);
    assert!(harness.state_mut().show_on_molecule("nothing"));
    harness.run_steps(3);
    let hidden = harness.state_mut().render_image(480, 360).unwrap();
    assert!(harness.state_mut().show_on_molecule("fragments"));
    harness.run_steps(3);
    let shown_again = harness.state_mut().render_image(480, 360).unwrap();

    let differing = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
    assert!(differing(&with_envelopes, &hidden) > 10_000, "envelopes are drawn");
    assert_eq!(differing(&hidden, &plain_image), 0, "hidden: exactly the molecule without results");
    assert_eq!(differing(&with_envelopes, &shown_again), 0);
}

/// Surface settings take effect without an "Update" click, and switching
/// between surfaces shows each alone.
#[test]
fn surfaces_update_by_themselves() {
    use egui_kittest::kittest::Queryable;
    let path = |name: &str| repo_dir().join("validation/gaussian").join(name);
    let mut harness = app_with(vec![path("H2O-ccpVDZ-HOMO.cube"), path("H2O-ccpVDZ-LUMO.cube")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.remove_cursor();
    harness.run_steps(4);
    let homo = screenshot(&mut harness, "surface-homo");

    harness.get_by_label("Both signs (+/-)").click_accesskit();
    harness.remove_cursor();
    harness.run_steps(4);
    let one_sign = screenshot(&mut harness, "surface-homo-one-sign");

    harness.get_by_label("Both signs (+/-)").click_accesskit();
    harness.get_by_label("2x").click_accesskit();
    harness.remove_cursor();
    harness.run_steps(4);
    let refined = screenshot(&mut harness, "surface-homo-refined");

    harness.get_by_label("H2O-ccpVDZ-LUMO.cube").click_accesskit();
    harness.remove_cursor();
    harness.run_steps(4);
    let lumo = screenshot(&mut harness, "surface-lumo");

    let differing = |a: &image::RgbaImage, b: &image::RgbaImage| a.pixels().zip(b.pixels()).filter(|(p, q)| p != q).count();
    assert!(differing(&homo, &one_sign) > 5_000, "turning off the negative lobe changes the picture");
    assert!(differing(&homo, &refined) > 500, "2x refinement re-extracts");
    assert!(differing(&refined, &lumo) > 5_000, "clicking another surface shows it instead");
}

/// Every window open at once over a molecule with results and orbitals.
#[test]
fn every_window_open() {
    use egui_kittest::kittest::Queryable;
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("CH3F.apost.json")]);
    for name in ["Surfaces", "Measure", "Appearance", "Export"] {
        harness.get_by_label(name).click_accesskit();
        harness.run_steps(1);
    }
    // (Through accessibility: with every window open, these are scrolled
    // out of sight inside their windows.)
    harness.get_all(egui_kittest::kittest::By::new().role(egui::accesskit::Role::CheckBox).label_contains("Hartree")).find(|n| format!("{n:?}").contains("HOMO")).expect("the HOMO's row").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 1 orbital(s)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("2  CH3   C1, H3, H4, H5").click_accesskit();
    harness.remove_cursor();
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "every-window")));
}
