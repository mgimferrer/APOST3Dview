//! Labels over the molecule (item 4 of the improvement backlog): never
//! overlapping, always whole; measurements always shown; an APOST-3D
//! value with its atom's or fragment's name above it, or alone; an atom
//! mostly visible keeps its label even when a hydrogen covers its center.

mod common;

use egui_kittest::kittest::Queryable;

use common::{app_with, fixtures_dir, repo_dir};

const SIZE: (f32, f32) = (1200.0, 900.0);

/// No two placed labels share any area.
fn assert_no_overlap(labels: &[(String, egui::Rect)]) {
    for (i, (a, ra)) in labels.iter().enumerate() {
        for (b, rb) in &labels[i + 1..] {
            let overlap = ra.min.x < rb.max.x && rb.min.x < ra.max.x && ra.min.y < rb.max.y && rb.min.y < ra.max.y;
            assert!(!overlap, "{a:?} {ra:?} overlaps {b:?} {rb:?}");
        }
    }
}

#[test]
fn measurements_are_always_shown() {
    let mut harness = app_with(vec![repo_dir().join("TESTS-VISUALIZER/BiCl3-def2TZVPP.fchk")]);
    for atoms in [&[0, 1][..], &[0, 2], &[1, 0, 2], &[2, 0, 3]] {
        harness.state_mut().select_atoms(atoms);
        harness.run_steps(2);
        harness.get_by_label("Add measurement").click_accesskit();
        harness.run_steps(2);
    }
    let labels = harness.state().placed_labels(SIZE.0, SIZE.1);
    assert_eq!(labels.iter().filter(|(t, _)| t.ends_with("\u{c5}")).count(), 2, "{labels:?}");
    assert_eq!(labels.iter().filter(|(t, _)| t.ends_with('\u{b0}')).count(), 2, "{labels:?}");
}

#[test]
fn atom_names_never_overlap() {
    let path = repo_dir().join("TESTS-VISUALIZER/Bi-dianion-OSD.fchk");
    if !path.is_file() {
        eprintln!("Bi-dianion-OSD.fchk not found; test skipped");
        return;
    }
    let mut harness = app_with(vec![path]);
    harness.get_by_label("Both").click_accesskit();
    harness.run_steps(2);
    let labels = harness.state().placed_labels(SIZE.0, SIZE.1);
    assert_no_overlap(&labels);
    assert!(labels.len() > 40 && labels.len() < 83, "most atoms named, crowded ones left out: {}", labels.len());
    assert!(labels.iter().any(|(t, _)| t == "Bi1"));
}

#[test]
fn values_with_their_names_or_alone() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("CH3F.apost.json")]);
    assert!(harness.state_mut().show_on_molecule("populations.atoms.charges"));
    harness.run_steps(3);
    let labels = harness.state().placed_labels(SIZE.0, SIZE.1);
    assert_no_overlap(&labels);
    // The carbon, behind a hydrogen in this view but mostly visible,
    // keeps its value; each value has its atom's name above it.
    assert!(labels.iter().any(|(t, _)| t == "C1\n+0.87"), "{labels:?}");
    assert!(labels.iter().any(|(t, _)| t == "F2\n\u{2212}0.72"), "{labels:?}");

    harness.get_by_label("with names").click_accesskit();
    harness.run_steps(3);
    let labels = harness.state().placed_labels(SIZE.0, SIZE.1);
    assert!(labels.iter().any(|(t, _)| t == "+0.87"), "{labels:?}");
    assert!(labels.iter().all(|(t, _)| !t.contains('\n')), "values alone: {labels:?}");
}
