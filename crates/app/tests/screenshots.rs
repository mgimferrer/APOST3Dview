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
fn molecule_and_orbital() {
    let mut harness = app_with(vec![repo_dir().join("TESTS-VISUALIZER/H2O-ccpVDZ-HOMO.cube")]);
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "cube")));
}

#[test]
fn apost3d_results() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "apost3d")));
    let mut harness = app_with(vec![dir.join("FeO4-2.fchk")]);
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "apost3d-fragments")));
    let mut harness = app_with(vec![dir.join("H2O-TPSS.apost.json")]);
    assert!(has_content(&screenshot(&mut harness, "apost3d-stopped")));
}

/// The fragment-color toggle changes the picture, and switching it back
/// restores it exactly.
#[test]
fn fragment_colors_switch_off_and_back_on() {
    use egui_kittest::kittest::Queryable;
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    harness.run_steps(4);
    let colored = screenshot(&mut harness, "fragments-on");
    harness.get_by_label("Color atoms by fragment").click();
    harness.remove_cursor();
    harness.run_steps(4);
    let element_colors = screenshot(&mut harness, "fragments-off");
    harness.get_by_label("Color atoms by fragment").click();
    harness.remove_cursor();
    harness.run_steps(4);
    let colored_again = screenshot(&mut harness, "fragments-on-again");
    let differing = |a: &image::RgbaImage, b: &image::RgbaImage| a.pixels().zip(b.pixels()).filter(|(p, q)| p != q).count();
    assert!(differing(&colored, &element_colors) > 10_000);
    assert_eq!(differing(&colored, &colored_again), 0);
}

/// Surface settings take effect without an "Update" click, and switching
/// between surfaces shows each alone.
#[test]
fn surfaces_update_by_themselves() {
    use egui_kittest::kittest::Queryable;
    let path = |name: &str| repo_dir().join("TESTS-VISUALIZER").join(name);
    let mut harness = app_with(vec![path("H2O-ccpVDZ-HOMO.cube"), path("H2O-ccpVDZ-LUMO.cube")]);
    harness.get_by_label("Surfaces").click();
    harness.remove_cursor();
    harness.run_steps(4);
    let homo = screenshot(&mut harness, "surface-homo");

    harness.get_by_label("Both signs (+/-)").click();
    harness.remove_cursor();
    harness.run_steps(4);
    let one_sign = screenshot(&mut harness, "surface-homo-one-sign");

    harness.get_by_label("Both signs (+/-)").click();
    harness.get_by_label("2x").click();
    harness.remove_cursor();
    harness.run_steps(4);
    let refined = screenshot(&mut harness, "surface-homo-refined");

    harness.get_by_label("H2O-ccpVDZ-LUMO.cube").click();
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
    let mut harness = app_with(vec![dir.join("CH3F.fchk")]);
    for name in ["Surfaces", "Measure", "Appearance", "Export"] {
        harness.get_by_label(name).click();
        harness.run_steps(1);
    }
    harness.get_by_label("Generate 1 orbital(s)").click();
    harness.run_steps(2);
    harness.get_by_label("2  CH3   C1, H3, H4, H5").click();
    harness.remove_cursor();
    harness.run_steps(4);
    assert!(has_content(&screenshot(&mut harness, "every-window")));
}
