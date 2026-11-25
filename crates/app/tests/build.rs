//! The Build window: edits go to a copy right under the original, which
//! stays as it was; every edit can be undone; a copy is saved as .xyz or
//! written into a session; the mode is marked; Build is offered once it is
//! switched on in Features.

mod common;

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use apost3dview::{App, StartupOptions};
use common::{app_with, example, scratch_dir, toasts};

type Ui = Harness<'static, App>;

const W: u32 = 320;
const H: u32 = 240;

fn ethane() -> PathBuf {
    example("C2H6-OSLO-2CH3").join("C2H6-OSLO-2CH3.fchk")
}

fn click(harness: &mut Ui, label: &str) {
    harness.query_all_by_label(label).next().unwrap_or_else(|| panic!("no \"{label}\" to click")).click_accesskit();
    harness.run_steps(3);
}

fn open_build(harness: &mut Ui) {
    if !harness.state().window_expanded(&harness.ctx, "Build") {
        click(harness, "Build");
    }
}

/// Switches to the structure named `label` from the Molecule window.
fn show(harness: &mut Ui, label: &str) {
    if !harness.state().window_expanded(&harness.ctx, "Molecule") {
        click(harness, "Molecule");
    }
    click(harness, label);
}

fn distance(p: &[[f32; 3]], a: usize, b: usize) -> f32 {
    let d = [p[a][0] - p[b][0], p[a][1] - p[b][1], p[a][2] - p[b][2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The dihedral a-b-c-d with the IUPAC sign, degrees.
fn dihedral(p: &[[f32; 3]], a: usize, b: usize, c: usize, d: usize) -> f64 {
    let v = |i: usize| glam::DVec3::new(p[i][0].into(), p[i][1].into(), p[i][2].into());
    let (b1, b2, b3) = (v(b) - v(a), v(c) - v(b), v(d) - v(c));
    (b2.length() * b1.dot(b2.cross(b3))).atan2(b1.cross(b2).dot(b2.cross(b3))).to_degrees()
}

/// The two carbons and a hydrogen on each, by element.
fn ethane_atoms(harness: &Ui) -> ([usize; 2], [usize; 2]) {
    let z = harness.state().atomic_numbers();
    let carbons: Vec<usize> = (0..z.len()).filter(|&i| z[i] == 6).collect();
    let p = harness.state().atom_positions();
    let h_on = |c: usize| (0..z.len()).find(|&i| z[i] == 1 && distance(&p, i, c) < 1.2).unwrap();
    ([carbons[0], carbons[1]], [h_on(carbons[0]), h_on(carbons[1])])
}

#[test]
fn edits_go_to_a_copy_and_can_be_undone() {
    let mut harness = app_with(vec![ethane()]);
    open_build(&mut harness);
    harness.query_by_label_contains("Edits go to a copy of").expect("the window says where edits go");
    let original_picture = harness.state_mut().render_image(W, H).unwrap();
    let original = harness.state().atom_positions();
    let ([c1, c2], [h1, h2]) = ethane_atoms(&harness);

    // The first edit makes the copy, right under the original, and shows it.
    harness.state_mut().select_atoms(&[c1, c2]);
    harness.run_steps(2);
    harness.state_mut().build_set_value(2.0);
    harness.run_steps(3);
    assert_eq!(harness.state().structure_labels(), vec!["C2H6-OSLO-2CH3.fchk".to_string(), "C2H6-OSLO-2CH3 (edited)".to_string()]);
    assert_eq!(harness.state().active_structure_label().as_deref(), Some("C2H6-OSLO-2CH3 (edited)"));
    assert!(toasts(&harness).iter().any(|t| t.contains("Edits go to a copy: C2H6-OSLO-2CH3 (edited)")));
    let edited = harness.state().atom_positions();
    assert!((distance(&edited, c1, c2) - 2.0).abs() < 1e-5);
    // C2's side moved with it: its hydrogen keeps its bond length.
    assert!((distance(&edited, c2, h2) - distance(&original, c2, h2)).abs() < 1e-5);
    assert_eq!(edited[c1], original[c1]);
    harness.query_by_label_contains("1 edit, not saved").expect("the edit is counted");
    harness.query_by_label_contains("Building: C2H6-OSLO-2CH3 (edited)").expect("the status bar names the copy");

    // A dihedral turns the whole methyl behind the C-C bond.
    harness.state_mut().select_atoms(&[h1, c1, c2, h2]);
    harness.run_steps(2);
    harness.state_mut().build_set_value(60.0);
    harness.run_steps(2);
    let turned = harness.state().atom_positions();
    assert!((dihedral(&turned, h1, c1, c2, h2) - 60.0).abs() < 1e-3);
    // Later edits go to the same copy.
    assert_eq!(harness.state().structure_labels().len(), 2);

    // The original is untouched, picture included (once the selection
    // made in it before the first edit is cleared).
    show(&mut harness, "C2H6-OSLO-2CH3.fchk");
    harness.state_mut().select_atoms(&[]);
    harness.run_steps(2);
    assert_eq!(harness.state().atom_positions(), original);
    assert_eq!(harness.state_mut().render_image(W, H).unwrap(), original_picture);

    // Undo twice gives the original geometry back in the copy; redo again.
    show(&mut harness, "C2H6-OSLO-2CH3 (edited)");
    open_build(&mut harness);
    click(&mut harness, "Undo");
    click(&mut harness, "Undo");
    assert_eq!(harness.state().atom_positions(), original);
    click(&mut harness, "Redo");
    assert!((distance(&harness.state().atom_positions(), c1, c2) - 2.0).abs() < 1e-5);

    // Change an element and delete atoms.
    harness.state_mut().select_atoms(&[c1]);
    harness.run_steps(2);
    click(&mut harness, "N");
    assert_eq!(harness.state().atomic_numbers()[c1], 7);
    harness.state_mut().select_atoms(&[h1]);
    harness.run_steps(2);
    click(&mut harness, "Delete selected atoms");
    assert_eq!(harness.state().atomic_numbers().len(), 7);
    click(&mut harness, "Undo");
    assert_eq!(harness.state().atomic_numbers().len(), 8);
}

#[test]
fn rings_move_the_last_atom_alone() {
    // Benzene-like ring of six carbons: a C-C distance can only move the
    // last atom, and the Moves choices that need sides are refused.
    let dir = scratch_dir("build-ring");
    let path = dir.join("ring.xyz");
    let mut text = String::from("6\nring\n");
    for k in 0..6 {
        let a = std::f32::consts::TAU * k as f32 / 6.0;
        text.push_str(&format!("C {:.6} {:.6} 0.0\n", 1.4 * a.cos(), 1.4 * a.sin()));
    }
    std::fs::write(&path, text).unwrap();
    let mut harness = app_with(vec![path]);
    open_build(&mut harness);
    let before = harness.state().atom_positions();
    harness.state_mut().select_atoms(&[0, 1]);
    harness.run_steps(2);
    // Through the window's Set, which falls back to the atom alone.
    harness.query_by_label_contains("alone will move").expect("the window says only the atom moves");
    click(&mut harness, "Set");
    let after = harness.state().atom_positions();
    let moved: Vec<usize> = (0..6).filter(|&i| after[i] != before[i]).collect();
    assert!(moved.is_empty() || moved == vec![1], "only atom 2 may move: {moved:?}");
}

#[test]
fn copies_are_saved_as_xyz_and_in_sessions() {
    let dir = scratch_dir("build-save");
    let mut harness = app_with(vec![ethane()]);
    open_build(&mut harness);
    let ([c1, c2], _) = ethane_atoms(&harness);
    harness.state_mut().select_atoms(&[c1, c2]);
    harness.run_steps(2);
    harness.state_mut().build_set_value(1.8);
    harness.run_steps(3);
    let edited = harness.state().atom_positions();
    // Selections are not saved in sessions.
    harness.state_mut().select_atoms(&[]);
    harness.run_steps(2);
    let picture = harness.state_mut().render_image(W, H).unwrap();

    // Not saved yet: the session holds the copy itself, as version 2.
    let session = dir.join("with-copy.a3dview");
    harness.state().save_session_to(&session).unwrap();
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("version = 2"), "{text}");
    assert!(text.contains("[structures.edited]") && text.contains("coordinates_angstrom"), "{text}");
    let mut reopened = app_with(vec![session.clone()]);
    reopened.run_steps(3);
    assert_eq!(reopened.state().structure_labels(), harness.state().structure_labels());
    assert_eq!(reopened.state().active_structure_label().as_deref(), Some("C2H6-OSLO-2CH3 (edited)"));
    assert_eq!(reopened.state().atom_positions(), edited);
    assert_eq!(reopened.state_mut().render_image(W, H).unwrap(), picture);

    // Renamed, then saved: the file takes the name, opens to the same atoms.
    harness.state_mut().rename_active_structure("stretched ethane");
    let xyz = dir.join("stretched ethane.xyz");
    assert!(harness.state_mut().build_save_xyz(&xyz));
    let opened = app_with(vec![xyz.clone()]);
    let saved = opened.state().atom_positions();
    assert_eq!(opened.state().atomic_numbers(), harness.state().atomic_numbers());
    assert!(saved.iter().zip(&edited).all(|(a, b)| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-6)));
    let first_line = std::fs::read_to_string(&xyz).unwrap().lines().nth(1).unwrap().to_string();
    assert_eq!(first_line, "stretched ethane, edited in APOST-3Dview from C2H6-OSLO-2CH3.fchk");

    // Saved: sessions refer to the file again and stay version 1.
    let session = dir.join("with-file.a3dview");
    harness.state().save_session_to(&session).unwrap();
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("version = 1") && !text.contains("coordinates_angstrom"), "{text}");
    let reopened = app_with(vec![session]);
    assert_eq!(reopened.state().structure_labels(), vec!["C2H6-OSLO-2CH3.fchk".to_string(), "stretched ethane".to_string()]);
}

#[test]
fn continuing_in_a_new_copy_keeps_the_first() {
    let mut harness = app_with(vec![ethane()]);
    open_build(&mut harness);
    let ([c1, c2], _) = ethane_atoms(&harness);
    harness.state_mut().select_atoms(&[c1, c2]);
    harness.run_steps(2);
    harness.state_mut().build_set_value(1.7);
    harness.run_steps(2);
    click(&mut harness, "Continue in a new copy");
    assert_eq!(
        harness.state().structure_labels(),
        vec!["C2H6-OSLO-2CH3.fchk".to_string(), "C2H6-OSLO-2CH3 (edited)".to_string(), "C2H6-OSLO-2CH3 (edited 2)".to_string()]
    );
    harness.state_mut().select_atoms(&[c1, c2]);
    harness.run_steps(2);
    harness.state_mut().build_set_value(1.9);
    harness.run_steps(2);
    show(&mut harness, "C2H6-OSLO-2CH3 (edited)");
    assert!((distance(&harness.state().atom_positions(), c1, c2) - 1.7).abs() < 1e-5);
}

#[test]
fn paths_are_not_edited() {
    let mut harness = app_with(vec![common::repo_dir().join("examples/reaction-path/reaction-path.xyz")]);
    open_build(&mut harness);
    harness.query_by_label_contains("A path's frames can't be edited").expect("the window says why");
}

/// The app with its Features kept in `store`.
fn app_with_features(store: &Path) -> Ui {
    let store = store.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(common::AppRenderer::new())
        .build_eframe(move |cc| App::with_options(cc, StartupOptions { files: vec![ethane()], splash: false, recent_file: None, features_file: Some(store) }));
    harness.run_steps(4);
    harness
}

#[test]
fn build_is_offered_once_switched_on_at_the_first_start() {
    let dir = scratch_dir("build-features");
    let store = dir.join("settings/features.toml");
    let mut harness = app_with_features(&store);
    harness.query_by_label("Welcome to APOST-3Dview").expect("the first start asks");
    assert!(harness.query_by_label("Build").is_none(), "Build is off until chosen");
    click(&mut harness, "Building and editing molecules");
    click(&mut harness, "Start");
    assert!(harness.query_by_label("Welcome to APOST-3Dview").is_none());
    harness.query_by_label("Build").expect("Build is offered");
    assert!(store.is_file());

    // Asked once per computer.
    let again = app_with_features(&store);
    assert!(again.query_by_label("Welcome to APOST-3Dview").is_none());
    again.query_by_label("Build").expect("still offered");
}

#[test]
fn closed_build_leaves_the_app_as_before() {
    // With Build never opened, no frame and the usual status bar.
    let mut harness = app_with(vec![ethane()]);
    assert!(harness.query_by_label_contains("Building:").is_none());
    open_build(&mut harness);
    harness.query_by_label_contains("Building: C2H6-OSLO-2CH3.fchk").expect("the mode shows in the status bar");
    click(&mut harness, "Build");
    assert!(harness.query_by_label_contains("Building:").is_none());
}
