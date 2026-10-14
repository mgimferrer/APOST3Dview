//! Sessions (`.a3dview`): a setup saved and opened in a fresh app gives the
//! same state and the same picture; a moved folder still opens; missing
//! or changed files are reported instead of applied.

mod common;

use std::path::{Path, PathBuf};

use apost3dview::App;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use common::{app_with, fixtures_dir, repo_dir, scratch_dir, toasts};

fn copy_into(dir: &Path, source: &Path) -> PathBuf {
    let target = dir.join(source.file_name().unwrap());
    std::fs::copy(source, &target).unwrap();
    target
}

fn image(harness: &mut Harness<'_, App>) -> Vec<u8> {
    harness.state_mut().render_image(320, 240).expect("render")
}

/// Rotates the view with the arrow keys for a few frames.
fn rotate(harness: &mut Harness<'_, App>) {
    harness.key_down(egui::Key::ArrowRight);
    harness.key_down(egui::Key::ArrowUp);
    harness.run_steps(5);
    harness.key_up(egui::Key::ArrowRight);
    harness.key_up(egui::Key::ArrowUp);
    harness.run_steps(2);
}

#[test]
fn a_session_reopens_to_the_same_picture() {
    let dir = scratch_dir("session-round-trip");
    let fchk = copy_into(&dir, &repo_dir().join("TESTS-VISUALIZER/H2O-631G.fchk"));
    let mut first = app_with(vec![fchk]);

    // Two generated orbitals, the first refined; element labels; no depth
    // of field; a turned camera.
    first.get_by_label("Surfaces").click();
    first.run_steps(2);
    first.get_by_label("HOMO/LUMO").click();
    first.run_steps(2);
    first.get_by_label("Generate 2 orbital(s)").click();
    first.run_steps(2);
    first.get_by_label("2x").click();
    first.get_by_label("Element").click();
    first.get_by_label("Appearance").click();
    first.run_steps(2);
    first.get_by_label("Depth of field").click();
    rotate(&mut first);
    first.run_steps(3);

    let session = dir.join("figure.a3dview");
    first.state().save_session_to(&session).unwrap();
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("file = \"H2O-631G.fchk\""), "paths are relative to the session:\n{text}");
    assert!(text.contains("kind = \"orbital\""), "orbitals are stored as a recipe:\n{text}");

    let mut second = app_with(vec![session]);
    second.run_steps(3);
    assert_eq!(second.state().structure_labels(), first.state().structure_labels());
    assert_eq!(second.state().surface_labels(), first.state().surface_labels());
    assert!(toasts(&second).iter().any(|m| m == "Session figure.a3dview opened"), "{:?}", toasts(&second));
    let (a, b) = (image(&mut first), image(&mut second));
    if let Some(dir) = std::env::var_os("APOST3DVIEW_SCREENSHOT_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, pixels) in [("session-saved", &a), ("session-reopened", &b)] {
            image::RgbaImage::from_raw(320, 240, pixels.clone()).unwrap().save(dir.join(format!("{name}.png"))).unwrap();
        }
    }
    let differing = a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count();
    assert_eq!(differing, 0, "the reopened session must render the same picture");
}

#[test]
fn per_atom_settings_survive_and_a_moved_folder_still_opens() {
    let Some(fixtures) = fixtures_dir() else { return };
    let dir = scratch_dir("session-atoms");
    let fchk = copy_into(&dir, &fixtures.join("CH3F.fchk"));
    copy_into(&dir, &fixtures.join("CH3F.apost.json"));
    let mut first = app_with(vec![fchk]);

    // Select the CH3 fragment, measure it, then hide it; fragment colors off.
    first.get_by_label("2  CH3   C1, H3, H4, H5").click();
    first.run_steps(2);
    first.get_by_label("Add measurement").click();
    first.run_steps(2);
    first.get_by_label("Hide selected").click();
    first.run_steps(2);
    // Atomic charges on the molecule, with a fixed color range.
    assert!(first.state_mut().show_on_molecule("populations.atoms.charges"));
    first.run_steps(2);
    first.get_by_label("Fixed").click();
    first.run_steps(3);
    first.state().save_session_to(&dir.join("ch3f.a3dview")).unwrap();
    let before = image(&mut first);

    // Move the whole folder: the relative paths still lead to the files.
    let moved = scratch_dir("session-atoms-moved");
    for entry in std::fs::read_dir(&dir).unwrap() {
        copy_into(&moved, &entry.unwrap().path());
    }
    std::fs::remove_dir_all(&dir).unwrap();
    let mut second = app_with(vec![moved.join("ch3f.a3dview")]);
    second.run_steps(3);
    assert!(toasts(&second).iter().all(|m| !m.contains("but")), "{:?}", toasts(&second));
    second.get_by_label("Measure").click();
    second.run_steps(2);
    second.get_by_label_contains("C1-H3-H4-H5: ");
    assert_eq!(second.state().legend_title().as_deref(), Some("Atomic charges (TFVC)"));
    assert!(before == image(&mut second), "hidden atoms, measurement, results view and color range restored");
}

#[test]
fn missing_and_changed_files_are_reported() {
    let Some(fixtures) = fixtures_dir() else { return };
    let dir = scratch_dir("session-problems");
    let fchk = copy_into(&dir, &fixtures.join("CH3F.fchk"));
    let cube = copy_into(&dir, &repo_dir().join("TESTS-VISUALIZER/H2O-ccpVDZ-HOMO.cube"));
    let mut first = app_with(vec![fchk.clone(), cube.clone()]);
    first.get_by_label("Hide hydrogens").click();
    first.run_steps(2);
    first.state().save_session_to(&dir.join("s.a3dview")).unwrap();

    // The cube disappears; the .fchk is replaced by another molecule.
    std::fs::remove_file(&cube).unwrap();
    std::fs::copy(repo_dir().join("TESTS-VISUALIZER/H2O-631G.fchk"), &fchk).unwrap();
    let second = app_with(vec![dir.join("s.a3dview")]);
    let messages = toasts(&second).join("\n");
    assert!(messages.contains("H2O-ccpVDZ-HOMO.cube was not found"), "{messages}");
    assert!(messages.contains("CH3F.fchk has changed since the session was saved"), "{messages}");
    assert_eq!(second.state().structure_labels(), vec!["CH3F.fchk"]);
}

#[test]
fn a_session_is_opened_from_the_open_menu_like_any_file() {
    let dir = scratch_dir("session-kind");
    let fchk = copy_into(&dir, &repo_dir().join("TESTS-VISUALIZER/H2O-631G.fchk"));
    let first = app_with(vec![fchk]);
    first.state().save_session_to(&dir.join("x.a3dview")).unwrap();
    let mut harness = app_with(Vec::new());
    harness.get_by_label("Open").click();
    harness.run_steps(1);
    harness.get_by_label("APOST3Dview session (.a3dview)...");
    // Not a session at all: a clear message.
    std::fs::write(dir.join("bad.a3dview"), "format = \"something else\"\nversion = 1\n").unwrap();
    let bad = app_with(vec![dir.join("bad.a3dview")]);
    assert!(toasts(&bad).iter().any(|m| m.starts_with("Could not open the session bad.a3dview")), "{:?}", toasts(&bad));
}

/// Extracting the same surface again gives the same picture to the bit,
/// which is what lets a reopened session reproduce a figure exactly.
#[test]
fn surfaces_are_extracted_identically_every_time() {
    let fchk = repo_dir().join("TESTS-VISUALIZER/H2O-631G.fchk");
    let generated = || {
        let mut harness = app_with(vec![fchk.clone()]);
        harness.get_by_label("Surfaces").click();
        harness.run_steps(2);
        harness.get_by_label("Generate 1 orbital(s)").click();
        harness.run_steps(5);
        harness
    };
    let (mut a, mut b) = (generated(), generated());
    let first = image(&mut a);
    assert!(first == image(&mut b), "two apps, same orbital");
    a.get_by_label("2x").click();
    a.run_steps(3);
    a.get_by_label("1x").click();
    a.run_steps(3);
    assert!(first == image(&mut a), "extracted again in the same app");
}
