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
