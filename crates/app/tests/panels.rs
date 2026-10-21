//! Panel export: a figure of several surfaces (or structures) with the
//! same view, each with a caption, in one PNG; the single image with a
//! caption; the session beside the figure; and the views structures keep
//! ("Keep the view when switching structures").

mod common;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

use apost3dview::App;
use common::{app_with, example, repo_dir, scratch_dir, screenshot};

type Ui = Harness<'static, App>;

fn click(harness: &mut Ui, label: &str) {
    harness.query_all_by_label(label).next().unwrap_or_else(|| panic!("no \"{label}\" to click")).click_accesskit();
    harness.run_steps(3);
}

/// Picks `option` in the combo box currently showing `value`.
fn choose(harness: &mut Ui, value: &str, option: &str) {
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    harness.get_all(By::new().role(Role::ComboBox)).find(|n| n.value().as_deref() == Some(value)).unwrap_or_else(|| panic!("no menu showing \"{value}\"")).click_accesskit();
    harness.run_steps(2);
    click(harness, option);
}

/// The ethane example with `oslos` (0-based) generated, the Export window
/// open on Panels.
fn ethane_panels(oslos: &[usize]) -> Ui {
    let dir = example("C2H6-OSLO-2CH3");
    let mut harness = app_with(vec![dir.join("C2H6-OSLO-2CH3.apost.json")]);
    click(&mut harness, "Surfaces");
    choose(&mut harness, "SCF orbitals", "OSLOs");
    harness.state_mut().select_set_orbitals(oslos, &[]);
    harness.run_steps(2);
    click(&mut harness, &format!("Generate {} orbital(s)", oslos.len()));
    click(&mut harness, "Export");
    click(&mut harness, "Panels");
    harness
}

#[test]
fn the_oslos_of_ethane_as_a_captioned_grid() {
    let mut harness = ethane_panels(&(0..9).collect::<Vec<_>>());
    // Every surface becomes a panel, in list order, captioned from the
    // results tables.
    harness.get_by_label("Panels (9)");
    harness.get_by_label("3 x 3");
    let captions = harness.state().panel_captions();
    assert_eq!(captions.len(), 9);
    assert_eq!(captions[0], "OSLO 1 \u{b7} 1 CH3 \u{b7} FOLI 1.000");
    assert_eq!(captions[8], "OSLO 9 \u{b7} 1 CH3 + 2 CH3 \u{b7} FOLI 2.001");
    click(&mut harness, "Value and fragment");
    let captions = harness.state().panel_captions();
    assert_eq!(captions[0], "1.000 (CH3)");
    assert_eq!(captions[8], "2.001 (CH3 + CH3)");
    // A caption of one's own, and back to automatic when emptied.
    harness.state_mut().set_panel_caption(0, "C 1s");
    assert_eq!(harness.state().panel_captions()[0], "C 1s");
    harness.state_mut().set_panel_caption(0, "");
    assert_eq!(harness.state().panel_captions()[0], "1.000 (CH3)");
    // Rows and columns as set, swapped.
    click(&mut harness, "Rows and columns");
    click(&mut harness, "Swap");
    click(&mut harness, "Automatic");
    click(&mut harness, "Full");
    // The preview is there, as wide as the window allows.
    harness.run_steps(4);
    let preview = harness.state().figure_preview_size().expect("a preview");
    assert!(preview[0] > 300 && preview[1] > 300, "{preview:?}");
    harness.key_press(egui::Key::Escape);
    for window in ["Molecule", "Surfaces", "APOST-3D"] {
        let ctx = harness.ctx.clone();
        harness.state().set_window_collapsed(&ctx, window, true);
    }
    harness.run_steps(4);
    screenshot(&mut harness, "panels-oslos");
    // Moving the first panel down.
    harness.query_all_by_label("\u{2b07}").next().expect("a move-down button").click_accesskit();
    harness.run_steps(3);
    let captions = harness.state().panel_captions();
    assert!(captions[0].starts_with("OSLO 2 ") && captions[1].starts_with("OSLO 1 "), "{captions:?}");
}

#[test]
fn the_figure_is_the_same_every_time_and_each_panel_is_the_single_export() {
    let mut harness = ethane_panels(&[0, 2, 4, 8]);
    harness.state_mut().set_panel_figure(160, false, false);
    let (w, h, first) = harness.state_mut().render_panel_figure().unwrap();
    let gap = 3; // 2% of 160
    assert_eq!((w, h), (2 * 160 + 3 * gap, 2 * 160 + 3 * gap), "a 2 x 2 grid");
    let (_, _, again) = harness.state_mut().render_panel_figure().unwrap();
    assert!(first == again, "the same figure twice");

    // Panel 4 (OSLO 9) is exactly what a single export shows with OSLO 9
    // alone: same view, same pixels.
    let before = harness.state_mut().render_image_supersampled(160, 160, 2).unwrap();
    // The Surfaces list's entry (the Export window's panel list names it too).
    harness.query_all_by_label("OSLO 9 \u{b7} 1 CH3 + 2 CH3").find(|n| !format!("{n:?}").contains("role: Label")).expect("OSLO 9 in the list").click_accesskit();
    harness.run_steps(4);
    let single = harness.state_mut().render_image_supersampled(160, 160, 2).unwrap();
    let (x0, y0) = (gap + 160 + gap, gap + 160 + gap);
    for row in 0..160u32 {
        let start = (((y0 + row) * w + x0) * 4) as usize;
        let expected = &single[(row * 160 * 4) as usize..((row + 1) * 160 * 4) as usize];
        assert!(&first[start..start + 160 * 4] == expected, "row {row} of the panel differs from the single export");
    }
    // Rendering the figure left the live view as it was.
    assert!(before != single, "showing OSLO 9 alone changed the picture");
    let (_, _, third) = harness.state_mut().render_panel_figure().unwrap();
    assert!(third == first);
    assert!(harness.state_mut().render_image_supersampled(160, 160, 2).unwrap() == single, "the live view is put back");
}

#[test]
fn the_session_beside_the_figure_makes_it_again() {
    let dir = scratch_dir("panels-session");
    let session = dir.join("figure.a3dview");
    let (figure, captions) = {
        let mut harness = ethane_panels(&[0, 4, 8]);
        harness.state_mut().set_panel_figure(150, true, true);
        click(&mut harness, "Value and fragment");
        harness.state_mut().set_panel_caption(2, "C-C bond");
        let figure = harness.state_mut().render_panel_figure().unwrap();
        harness.state().save_session_to(&session).unwrap();
        (figure, harness.state().panel_captions())
    };
    assert_eq!(captions, ["1.000 (CH3)", "1.039 (CH3)", "C-C bond"]);
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("[figure.settings]") && text.contains("[[figure.panels]]") && text.contains("caption = \"C-C bond\""), "{text}");

    let mut harness = app_with(vec![session]);
    harness.run_steps(6);
    assert_eq!(harness.state().panel_captions(), captions);
    let again = harness.state_mut().render_panel_figure().unwrap();
    assert_eq!((again.0, again.1), (figure.0, figure.1));
    assert!(again.2 == figure.2, "the reopened session gives the same figure, pixel for pixel");
}

#[test]
fn a_single_image_can_have_a_caption() {
    let mut harness = ethane_panels(&[8]);
    click(&mut harness, "Single image");
    click(&mut harness, "Caption");
    harness.run_steps(4);
    assert!(harness.state().figure_preview_size().is_some(), "a preview with the caption");
    click(&mut harness, "Custom (pixels)");
    let (w, h, pixels) = harness.state_mut().render_panel_figure().unwrap();
    assert!(h > w * 1080 / 1920, "room for the caption below the image: {w} x {h}");
    // Dark caption text below the picture.
    let below = &pixels[((h - h / 12) * w * 4) as usize..];
    assert!(below.chunks(4).any(|p| p[0] < 100 && p[1] < 100 && p[2] < 100));
}

#[test]
fn structures_keep_their_views_or_share_one() {
    let h2o = repo_dir().join("validation/gaussian/H2O-ccpVTZ.fchk");
    let bicl3 = repo_dir().join("validation/gaussian/BiCl3-def2TZVPP.fchk");
    let mut harness = app_with(vec![h2o, bicl3]);
    if harness.query_by_label("Keep the view when switching structures").is_none() {
        click(&mut harness, "Molecule");
    }
    let active = harness.state().active_structure_label().unwrap();
    let other = harness.state().structure_labels().into_iter().find(|l| *l != active).unwrap();
    // Turn the active structure, go to the other and back: its view is
    // given back.
    harness.state_mut().turn_view(0.6, 0.3);
    harness.run_steps(2);
    let turned = harness.state().camera_state();
    let picture = harness.state_mut().render_image(200, 150).unwrap();
    click(&mut harness, &other);
    let elsewhere = harness.state().camera_state();
    assert_ne!(elsewhere.0, turned.0, "the other structure is framed on itself");
    click(&mut harness, &active);
    assert_eq!(harness.state().camera_state(), turned, "the turned structure got its view back");
    assert!(harness.state_mut().render_image(200, 150).unwrap() == picture);
    click(&mut harness, &other);
    assert_eq!(harness.state().camera_state(), elsewhere, "and the other one its own");

    // Keep the view: the turned structure's orientation and zoom carry
    // over, centered on the other molecule.
    click(&mut harness, &active);
    click(&mut harness, "Keep the view when switching structures");
    click(&mut harness, &other);
    let switched = harness.state().camera_state();
    assert_eq!((switched.1, switched.2), (turned.1, turned.2), "same distance and orientation");
    assert_eq!(switched.0, elsewhere.0, "centered on the other molecule");
}

#[test]
fn panels_of_two_structures_and_their_session() {
    let h2o = repo_dir().join("validation/gaussian/H2O-ccpVTZ.fchk");
    let bicl3 = repo_dir().join("validation/gaussian/BiCl3-def2TZVPP.fchk");
    let dir = scratch_dir("panels-two-structures");
    let session = dir.join("two.a3dview");
    let (figure, captions, picture) = {
        let mut harness = app_with(vec![h2o, bicl3]);
        harness.state_mut().set_keep_view(true);
        harness.state_mut().turn_view(0.4, 0.2);
        click(&mut harness, "Export");
        click(&mut harness, "Panels");
        // The active structure as shown; add the other one.
        assert_eq!(harness.state().panel_captions().len(), 1);
        let other = harness.state().structure_labels().into_iter().find(|l| !harness.state().panel_captions()[0].starts_with(l.trim_end_matches(".fchk"))).unwrap();
        choose(&mut harness, "Add a panel", &format!("{other} (as shown)"));
        let captions = harness.state().panel_captions();
        assert_eq!(captions.len(), 2);
        let mut sorted = captions.clone();
        sorted.sort();
        assert_eq!(sorted, ["BiCl3-def2TZVPP", "H2O-ccpVTZ"]);
        harness.state_mut().set_panel_figure(140, true, true);
        let camera = harness.state().camera_state();
        let picture = harness.state_mut().render_image(200, 150).unwrap();
        let figure = harness.state_mut().render_panel_figure().unwrap();
        assert_eq!(harness.state().camera_state(), camera, "the live camera is put back");
        assert!(harness.state_mut().render_image(200, 150).unwrap() == picture, "the live view is put back");
        harness.state().save_session_to(&session).unwrap();
        (figure, captions, picture)
    };
    let mut harness = app_with(vec![session]);
    harness.run_steps(6);
    assert_eq!(harness.state().panel_captions(), captions);
    assert!(harness.state_mut().render_image(200, 150).unwrap() == picture);
    let again = harness.state_mut().render_panel_figure().unwrap();
    assert!(again.2 == figure.2, "the same two-structure figure from the session");
}
