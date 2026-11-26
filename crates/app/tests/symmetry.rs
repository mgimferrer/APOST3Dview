//! Point groups: shown in the Molecule window for everyone; in the Build
//! window the groups a structure is close to, their elements drawn in the
//! view, and Symmetrize as an edit.

mod common;

use std::path::PathBuf;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use apost3dview::App;
use common::{app_with, example, repo_dir, scratch_dir};

type Ui = Harness<'static, App>;

fn click(harness: &mut Ui, label: &str) {
    harness.query_all_by_label(label).next().unwrap_or_else(|| panic!("no \"{label}\" to click")).click_accesskit();
    harness.run_steps(3);
}

/// Ammonia with its hydrogens pulled a little off C3v (up to ~0.02 Å).
fn distorted_ammonia() -> PathBuf {
    let path = scratch_dir("symmetry-ammonia").join("ammonia.xyz");
    let mut text = String::from("4\nammonia, a little off C3v\nN 0.0 0.0 0.10\n");
    for (k, (dr, dz)) in [(0.0, 0.0), (0.015, -0.01), (-0.01, 0.012)].iter().enumerate() {
        let a = (120.0 * k as f64).to_radians();
        text.push_str(&format!("H {:.6} {:.6} {:.6}\n", (0.94 + dr) * a.cos(), (0.94 + dr) * a.sin(), -0.27 + dz));
    }
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_point_group_shows_in_the_molecule_window() {
    let harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
    harness.query_by_label("Point group:").expect("the line is there");
    harness.query_by_label("C2v").expect("water is C2v");
    let harness = app_with(vec![example("C2H6-OSLO-2CH3").join("C2H6-OSLO-2CH3.fchk")]);
    harness.query_by_label("D3d").expect("staggered ethane is D3d");
    // A structure close to a higher group says how close.
    let harness = app_with(vec![distorted_ammonia()]);
    harness.query_by_label_contains("C1 (C3v within").expect("C1, and the nearby group named");
}

#[test]
fn symmetrize_makes_the_group_exact_in_a_copy() {
    let mut harness = app_with(vec![distorted_ammonia()]);
    let before = harness.state().atom_positions();
    click(&mut harness, "Build");
    harness.query_by_label("Symmetry").expect("the section is there");
    let plain = harness.render().expect("render");
    // Choosing C3v draws its axis and planes over the view.
    click(&mut harness, "C3v");
    harness.remove_cursor();
    harness.run_steps(2);
    let chosen = harness.render().expect("render");
    assert_ne!(plain.as_raw(), chosen.as_raw(), "the elements are drawn");
    click(&mut harness, "Symmetrize to C3v");
    assert_eq!(harness.state().structure_labels(), vec!["ammonia.xyz".to_string(), "ammonia (edited)".to_string()]);
    assert!(harness.query_all_by_label("C3v").next().is_some(), "the copy is C3v");
    assert!(harness.query_by_label_contains("(C3v within").is_none(), "no longer close: exact");
    // Atoms moved by about the distortion, not more.
    let after = harness.state().atom_positions();
    let moved = before.iter().zip(&after).map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()).fold(0.0, f32::max);
    assert!(moved > 0.001 && moved < 0.03, "moved {moved}");
    click(&mut harness, "Undo");
    assert_eq!(harness.state().atom_positions(), before);
}

/// The view itself, left of the window column (egui shades the title bar
/// of the window used last, which says nothing about the view).
fn view_pixels(harness: &mut Ui) -> Vec<u8> {
    harness.remove_cursor();
    harness.run_steps(2);
    let image = harness.render().expect("render");
    image::imageops::crop_imm(&image, 0, 30, 980, 840).to_image().into_raw()
}

#[test]
fn symmetry_elements_are_drawn_only_in_build_mode() {
    let mut harness = app_with(vec![distorted_ammonia()]);
    let closed = view_pixels(&mut harness);
    click(&mut harness, "Build");
    click(&mut harness, "C3v");
    assert_ne!(view_pixels(&mut harness), closed, "the elements are drawn while Build is open");
    click(&mut harness, "Build");
    assert_eq!(view_pixels(&mut harness), closed, "Build closed: the view as before");
}

#[test]
fn standard_orientation_moves_the_coordinates_not_the_picture() {
    let mut harness = app_with(vec![example("CH3F-GEOS").join("CH3F-GEOS.fchk")]);
    let picture = harness.state_mut().render_image(320, 240).unwrap();
    click(&mut harness, "Build");
    let screen = harness.state().atom_screen_positions(320.0, 240.0);
    click(&mut harness, "Standard orientation");
    // Every atom where it was on screen.
    let moved = screen.iter().zip(harness.state().atom_screen_positions(320.0, 240.0)).map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1])).fold(0.0, f32::max);
    assert!(moved < 0.01, "atoms moved {moved} px on screen");
    // A copy, with the C-F axis along z and the center of charge at the origin.
    assert_eq!(harness.state().active_structure_label().as_deref(), Some("CH3F-GEOS (edited)"));
    let z = harness.state().atomic_numbers();
    let p = harness.state().atom_positions();
    let (c, f) = ((0..z.len()).find(|&i| z[i] == 6).unwrap(), (0..z.len()).find(|&i| z[i] == 9).unwrap());
    assert!(p[c][0].abs() < 1e-4 && p[c][1].abs() < 1e-4 && p[f][0].abs() < 1e-4 && p[f][1].abs() < 1e-4, "{:?} {:?}", p[c], p[f]);
    let charge: f32 = z.iter().map(|&k| k as f32).sum();
    let center: Vec<f32> = (0..3).map(|k| p.iter().zip(&z).map(|(v, &q)| v[k] * q as f32).sum::<f32>() / charge).collect();
    assert!(center.iter().all(|v| v.abs() < 1e-4), "{center:?}");
    // The picture is the same (to rounding).
    let after = harness.state_mut().render_image(320, 240).unwrap();
    let differing = picture.chunks(4).zip(after.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8)).count();
    assert!(differing < 200, "{differing} pixels changed");
}
