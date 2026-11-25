//! Dragging atoms: only in Build mode with Drag atoms switched on. A drag
//! that starts on an atom then moves it (in the copy, one undo step), one
//! from empty space still turns the view, Esc during a drag puts everything
//! back; otherwise a drag turns the view as always.

mod common;

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use apost3dview::App;
use common::{app_with, example};

type Ui = Harness<'static, App>;

fn ethane() -> Ui {
    app_with(vec![example("C2H6-OSLO-2CH3").join("C2H6-OSLO-2CH3.fchk")])
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

/// Presses at `from`, moves to `to` in ten steps; releases unless `hold`.
fn drag(harness: &mut Ui, from: Pos2, to: Pos2, modifiers: Modifiers, hold: bool) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(Event::PointerMoved(from));
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers });
    harness.run_steps(1);
    for step in 1..=10 {
        harness.event(Event::PointerMoved(from + (to - from) * step as f32 / 10.0));
        harness.run_steps(1);
    }
    if !hold {
        release(harness, to, modifiers);
    }
}

fn release(harness: &mut Ui, at: Pos2, modifiers: Modifiers) {
    harness.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers });
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.run_steps(3);
}

/// The atom drawn nearest the viewer's left, well inside the view: a
/// hydrogen in front, clear of the windows on the right.
fn grab_point(harness: &Ui) -> (usize, Pos2) {
    let screen = harness.state().atom_window_positions();
    let (index, p) = screen.iter().enumerate().min_by(|a, b| a.1[0].total_cmp(&b.1[0])).unwrap();
    (index, Pos2::new(p[0], p[1]))
}

fn moved_atoms(before: &[[f32; 3]], after: &[[f32; 3]]) -> Vec<usize> {
    (0..before.len()).filter(|&i| before[i] != after[i]).collect()
}

#[test]
fn without_build_a_drag_turns_the_view() {
    let mut harness = ethane();
    let (_, at) = grab_point(&harness);
    let camera = harness.state().camera_state();
    let atoms = harness.state().atom_positions();
    drag(&mut harness, at, at + egui::vec2(60.0, 0.0), Modifiers::NONE, false);
    assert_ne!(harness.state().camera_state(), camera, "the view turned");
    assert_eq!(harness.state().atom_positions(), atoms);
    // D does nothing outside Build mode.
    harness.key_press(egui::Key::D);
    harness.run_steps(2);
    assert!(!harness.state().drag_atoms_on());

    // Build open, switch off: the same.
    open_build(&mut harness);
    let (_, at) = grab_point(&harness);
    let camera = harness.state().camera_state();
    drag(&mut harness, at, at + egui::vec2(60.0, 0.0), Modifiers::NONE, false);
    assert_ne!(harness.state().camera_state(), camera);
    assert_eq!(harness.state().atom_positions(), atoms);
    assert_eq!(harness.state().structure_labels().len(), 1, "no copy");
}

#[test]
fn a_drag_from_an_atom_moves_it_in_a_copy() {
    let mut harness = ethane();
    open_build(&mut harness);
    harness.key_press(egui::Key::D);
    harness.run_steps(2);
    assert!(harness.state().drag_atoms_on());
    harness.query_by_label("Drag moves:").expect("the choice shows with the switch on");

    let (atom, at) = grab_point(&harness);
    let camera = harness.state().camera_state();
    let before = harness.state().atom_positions();
    drag(&mut harness, at, at + egui::vec2(50.0, -30.0), Modifiers::NONE, false);
    assert_eq!(harness.state().camera_state(), camera, "the view didn't turn");
    assert_eq!(harness.state().active_structure_label().as_deref(), Some("C2H6-OSLO-2CH3 (edited)"));
    let after = harness.state().atom_positions();
    assert_eq!(moved_atoms(&before, &after), vec![atom]);
    // It followed the pointer on screen.
    let screen = harness.state().atom_window_positions()[atom];
    assert!((screen[0] - (at.x + 50.0)).abs() < 1.0 && (screen[1] - (at.y - 30.0)).abs() < 1.0, "{screen:?} vs {at:?}");

    // One drag, one undo step.
    harness.query_by_label_contains("1 edit, not saved").expect("one step");
    click(&mut harness, "Undo");
    assert_eq!(harness.state().atom_positions(), before);

    // From empty space, the view still turns.
    let corner = harness.state().atom_window_positions().iter().fold(Pos2::new(f32::MAX, f32::MAX), |m, p| Pos2::new(m.x.min(p[0]), m.y.min(p[1]))) - egui::vec2(60.0, 60.0);
    drag(&mut harness, corner, corner + egui::vec2(40.0, 0.0), Modifiers::NONE, false);
    assert_ne!(harness.state().camera_state(), camera);
    assert_eq!(harness.state().atom_positions(), before);
}

#[test]
fn turning_and_whole_pieces() {
    let mut harness = ethane();
    open_build(&mut harness);
    click(&mut harness, "Drag atoms (D)");
    click(&mut harness, "Its piece");
    let before = harness.state().atom_positions();
    let (_, at) = grab_point(&harness);
    // The whole molecule is one piece: every atom moves, rigidly.
    drag(&mut harness, at, at + egui::vec2(30.0, 0.0), Modifiers::NONE, false);
    let after = harness.state().atom_positions();
    assert_eq!(moved_atoms(&before, &after).len(), before.len());
    let d = |p: &[[f32; 3]], a: usize, b: usize| ((0..3).map(|k| (p[a][k] - p[b][k]).powi(2)).sum::<f32>()).sqrt();
    assert!((d(&before, 0, 5) - d(&after, 0, 5)).abs() < 1e-4);

    // Cmd/Ctrl+drag turns it about its center, which stays put.
    let center = |p: &[[f32; 3]]| (0..3).map(|k| p.iter().map(|q| q[k]).sum::<f32>() / p.len() as f32).collect::<Vec<_>>();
    let (_, at) = grab_point(&harness);
    drag(&mut harness, at, at + egui::vec2(0.0, 60.0), Modifiers::COMMAND, false);
    let turned = harness.state().atom_positions();
    assert_ne!(turned, after);
    let (c0, c1) = (center(&after), center(&turned));
    assert!((0..3).all(|k| (c0[k] - c1[k]).abs() < 1e-4), "{c0:?} {c1:?}");
    assert!((d(&after, 0, 5) - d(&turned, 0, 5)).abs() < 1e-4);
    harness.query_by_label_contains("2 edits").expect("two drags, two steps");
}

#[test]
fn escape_during_a_drag_puts_everything_back() {
    let mut harness = ethane();
    open_build(&mut harness);
    harness.key_press(egui::Key::D);
    harness.run_steps(2);
    let before = harness.state().atom_positions();
    let (_, at) = grab_point(&harness);
    drag(&mut harness, at, at + egui::vec2(40.0, 40.0), Modifiers::NONE, true);
    assert_ne!(harness.state().atom_positions(), before, "moving while held");
    harness.query_by_label_contains("Moving").expect("the status bar says what moves");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    release(&mut harness, at + egui::vec2(40.0, 40.0), Modifiers::NONE);
    // The copy this drag made is gone again, and nothing moved.
    assert_eq!(harness.state().structure_labels(), vec!["C2H6-OSLO-2CH3.fchk".to_string()]);
    assert_eq!(harness.state().atom_positions(), before);
}

#[test]
fn closing_build_switches_dragging_off() {
    let mut harness = ethane();
    open_build(&mut harness);
    harness.key_press(egui::Key::D);
    harness.run_steps(2);
    assert!(harness.state().drag_atoms_on());
    click(&mut harness, "Build");
    assert!(!harness.state().drag_atoms_on());
    open_build(&mut harness);
    assert!(!harness.state().drag_atoms_on(), "off when Build opens again");
}
