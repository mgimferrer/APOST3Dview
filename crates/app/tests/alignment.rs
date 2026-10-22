//! Aligning a structure onto another on selected atoms: a copy of a
//! molecule turned and moved away is seen exactly like the original once
//! aligned, and the alignment survives a session.

mod common;

use std::path::{Path, PathBuf};

use apost3dview_core::{element_data, Molecule};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use glam::{Quat, Vec3};

use apost3dview::App;
use common::{app_with, example, scratch_dir};

type Ui = Harness<'static, App>;

const W: u32 = 320;
const H: u32 = 240;

fn click(harness: &mut Ui, label: &str) {
    harness.query_all_by_label(label).next().unwrap_or_else(|| panic!("no \"{label}\" to click")).click_accesskit();
    harness.run_steps(3);
}

/// The molecule of `fchk`, turned and moved far away, as an `.xyz`.
fn moved_copy(fchk: &Path, dir: &Path) -> PathBuf {
    let molecule = Molecule::from_fchk(fchk).unwrap();
    let rotation = Quat::from_axis_angle(Vec3::new(0.4, 1.0, -0.3).normalize(), 1.9);
    let shift = Vec3::new(12.0, -7.0, 3.5);
    let mut text = format!("{}\nturned and moved\n", molecule.positions.len());
    for (z, p) in molecule.atomic_numbers.iter().zip(&molecule.positions) {
        let q = rotation * *p + shift;
        text.push_str(&format!("{} {:.8} {:.8} {:.8}\n", element_data(*z).symbol, q.x, q.y, q.z));
    }
    let path = dir.join("moved.xyz");
    std::fs::write(&path, text).unwrap();
    path
}

fn show(harness: &mut Ui, label: &str) {
    if harness.query_by_label("Keep the view when switching structures").is_none() {
        click(harness, "Molecule");
    }
    click(harness, label);
}

/// Largest distance between matching screen positions, in pixels.
fn farthest(a: &[[f32; 2]], b: &[[f32; 2]]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(p, q)| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()).fold(0.0, f32::max)
}

/// The mean difference per pixel (largest channel, 0-255) and how many
/// pixels differ by more than 8.
fn difference(a: &[u8], b: &[u8]) -> (f64, usize) {
    let d: Vec<u8> = a.chunks(4).zip(b.chunks(4)).map(|(p, q)| p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap()).collect();
    (d.iter().map(|&x| f64::from(x)).sum::<f64>() / d.len() as f64, d.iter().filter(|&&x| x > 8).count())
}

#[test]
fn an_aligned_copy_is_seen_exactly_like_the_original() {
    let dir = scratch_dir("alignment");
    let fchk = example("CH3F-GEOS").join("CH3F-GEOS.fchk");
    let moved = moved_copy(&fchk, &dir);
    let mut harness = app_with(vec![fchk, moved]);
    let labels = harness.state().structure_labels();
    let (original, copy) = (labels[0].clone(), labels[1].clone());

    // The original, turned, with four atoms selected.
    show(&mut harness, &original);
    harness.state_mut().turn_view(0.5, 0.3);
    harness.state_mut().select_atoms(&[0, 1, 2, 3]);
    harness.run_steps(2);
    let original_atoms = harness.state().atom_screen_positions(W as f32, H as f32);
    harness.state_mut().select_atoms(&[]);
    let original_picture = harness.state_mut().render_image(W, H).unwrap();
    harness.state_mut().select_atoms(&[0, 1, 2, 3]);

    // The copy: nothing to align onto until as many atoms are selected.
    show(&mut harness, &copy);
    assert!(harness.query_by_label(&format!("Align onto {original}")).is_none());
    harness.state_mut().select_atoms(&[0, 1, 2, 3]);
    harness.run_steps(2);
    click(&mut harness, &format!("Align onto {original}"));
    let toast = harness.state().toast_texts().into_iter().find(|t| t.contains("aligned onto")).expect("a message");
    assert!(toast.contains("on 4 atoms (RMSD 0.000"), "{toast}");
    harness.get_by_label_contains("Aligned onto");

    // The same picture: atoms on the same spots, lit the same way.
    let copy_atoms = harness.state().atom_screen_positions(W as f32, H as f32);
    assert!(farthest(&original_atoms, &copy_atoms) < 0.01, "{original_atoms:?} vs {copy_atoms:?}");
    harness.state_mut().select_atoms(&[]);
    let copy_picture = harness.state_mut().render_image(W, H).unwrap();
    // Only rounding noise from the turned coordinates: on average under
    // 0.3/255 and fewer than 100 pixels off by more than 8/255. (With the
    // fill light's "up" left at +Y, the copy's shading differs: 0.68 on
    // average and 281 pixels, measured 2026-10-22.)
    let (mean, large) = difference(&original_picture, &copy_picture);
    assert!(mean < 0.3 && large < 100, "mean difference {mean}, {large} pixels off by more than 8");

    // Keep the view is on: back to the original, same picture as before.
    show(&mut harness, &original);
    harness.state_mut().select_atoms(&[]);
    assert!(harness.state_mut().render_image(W, H).unwrap() == original_picture);

    // Undoing keeps what is on screen, in the copy's own coordinates.
    show(&mut harness, &copy);
    let before = harness.state().atom_screen_positions(W as f32, H as f32);
    click(&mut harness, "Undo alignment");
    assert!(farthest(&before, &harness.state().atom_screen_positions(W as f32, H as f32)) < 0.01);
    assert!(harness.query_by_label_contains("Aligned onto").is_none());
}

#[test]
fn the_alignment_is_kept_in_the_session() {
    let dir = scratch_dir("alignment-session");
    let fchk = example("CH3F-GEOS").join("CH3F-GEOS.fchk");
    let moved = moved_copy(&fchk, &dir);
    let session = dir.join("aligned.a3dview");
    let (picture, atoms) = {
        let mut harness = app_with(vec![fchk, moved]);
        let labels = harness.state().structure_labels();
        show(&mut harness, &labels[0]);
        harness.state_mut().turn_view(-0.4, 0.6);
        harness.state_mut().select_atoms(&[4, 0, 2]);
        show(&mut harness, &labels[1]);
        harness.state_mut().select_atoms(&[4, 0, 2]);
        harness.run_steps(2);
        click(&mut harness, &format!("Align onto {}", labels[0]));
        harness.state_mut().select_atoms(&[]);
        harness.state().save_session_to(&session).unwrap();
        (harness.state_mut().render_image(W, H).unwrap(), harness.state().atom_screen_positions(W as f32, H as f32))
    };
    assert!(std::fs::read_to_string(&session).unwrap().contains("[structures.alignment]"));
    let mut harness = app_with(vec![session]);
    harness.run_steps(6);
    if harness.query_by_label("Keep the view when switching structures").is_none() {
        click(&mut harness, "Molecule");
    }
    harness.get_by_label_contains("Aligned onto");
    assert_eq!(harness.state().atom_screen_positions(W as f32, H as f32), atoms);
    assert!(harness.state_mut().render_image(W, H).unwrap() == picture, "the aligned view comes back pixel for pixel");
}
