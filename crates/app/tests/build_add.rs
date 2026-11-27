//! The Build window's Add section, through real pointer events: what is
//! picked stays picked, the status bar says what a click would do before
//! it is done, a click does it (one edit, undone by Cmd+Z), Esc gives back
//! Select; elements bring their hydrogens; molecules go in empty space;
//! hydrogens, mirror images, new molecules, own groups and sessions.

mod common;

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use apost3dview::App;
use common::{app_with, example, scratch_dir};

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

fn hover(harness: &mut Ui, at: Pos2) {
    harness.event(Event::PointerMoved(at));
    harness.run_steps(2);
}

fn press(harness: &mut Ui, at: Pos2) {
    hover(harness, at);
    harness.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    harness.run_steps(1);
    harness.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    harness.run_steps(3);
}

/// The leftmost atom in the view of `element`, and where it is drawn.
fn leftmost(harness: &Ui, element: u32) -> (usize, Pos2) {
    let z = harness.state().atomic_numbers();
    let screen = harness.state().atom_window_positions();
    let (i, p) = screen.iter().enumerate().filter(|(i, _)| z[*i] == element).min_by(|a, b| a.1[0].total_cmp(&b.1[0])).unwrap();
    (i, Pos2::new(p[0], p[1]))
}

/// An atom of `element` that the pointer finds (not hidden behind another),
/// and where it is drawn.
fn visible(harness: &mut Ui, element: u32) -> (usize, Pos2) {
    let z = harness.state().atomic_numbers();
    let screen = harness.state().atom_window_positions();
    for (i, p) in screen.iter().enumerate().filter(|(i, _)| z[*i] == element) {
        let at = Pos2::new(p[0], p[1]);
        if harness.state().atom_at_window_point(at.x, at.y) == Some(i) {
            return (i, at);
        }
    }
    panic!("no visible atom of element {element}");
}

/// A point of empty space left of the molecule.
fn empty_space(harness: &Ui) -> Pos2 {
    let screen = harness.state().atom_window_positions();
    let left = screen.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let y = screen.iter().map(|p| p[1]).sum::<f32>() / screen.len() as f32;
    Pos2::new(left - 140.0, y)
}

fn status_has(harness: &Ui, text: &str) -> bool {
    harness.query_all_by_label_contains(text).next().is_some()
}

fn undo(harness: &mut Ui) {
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
    harness.run_steps(3);
}

#[test]
fn a_group_replaces_the_hydrogen_under_the_pointer() {
    let mut harness = ethane();
    let before = harness.state_mut().render_image(320, 240);
    open_build(&mut harness);
    click(&mut harness, "tBu");
    let (h, at) = leftmost(&harness, 1);
    hover(&mut harness, at);
    let hint = harness.state().build_add_hint().expect("a hint while hovering");
    assert_eq!(hint, format!("Click to replace H{} by tBu", h + 1));
    assert!(status_has(&harness, &hint), "the status bar says it");
    assert_eq!(harness.state().atomic_numbers().len(), 8, "nothing done before the click");
    press(&mut harness, at);
    let z = harness.state().atomic_numbers();
    assert_eq!(z.len(), 8 - 1 + 13);
    assert_eq!(z.iter().filter(|&&x| x == 6).count(), 6);
    assert_eq!(harness.state().selected_atoms().len(), 13, "the new atoms are selected");
    assert!(harness.state().structure_labels().iter().any(|l| l.ends_with("(edited)")));
    // Still picked: a second hydrogen too.
    let (_, at) = visible(&mut harness, 1);
    hover(&mut harness, at);
    assert!(harness.state().build_add_hint().unwrap().starts_with("Click to replace H"), "{:?}", harness.state().build_add_hint());
    press(&mut harness, at);
    assert_eq!(harness.state().atomic_numbers().len(), 20 - 1 + 13);
    undo(&mut harness);
    undo(&mut harness);
    assert_eq!(harness.state().atomic_numbers().len(), 8);
    // Esc gives back Select: then a click selects.
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    let (h, at) = leftmost(&harness, 1);
    press(&mut harness, at);
    assert_eq!(harness.state().build_add_hint(), None);
    assert_eq!(harness.state().selected_atoms(), vec![h]);
    assert_eq!(harness.state().atomic_numbers().len(), 8);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    // The copy, undone, looks as the original did.
    let mut closed = harness;
    click(&mut closed, "Build");
    assert_eq!(closed.state_mut().render_image(320, 240), before);
}

#[test]
fn elements_bring_and_adjust_their_hydrogens() {
    let mut harness = ethane();
    open_build(&mut harness);
    harness.state_mut().build_pick("element:C");
    let (h, at) = leftmost(&harness, 1);
    hover(&mut harness, at);
    assert_eq!(harness.state().build_add_hint().unwrap(), format!("Click to replace H{} by CH3", h + 1));
    press(&mut harness, at);
    assert_eq!(harness.state().atomic_numbers().len(), 11, "propane");
    undo(&mut harness);
    // N on a carbon of ethane: methylamine (one hydrogen goes).
    harness.state_mut().build_pick("element:N");
    let (c, at) = visible(&mut harness, 6);
    hover(&mut harness, at);
    assert_eq!(harness.state().build_add_hint().unwrap(), format!("Click to change C{} to N, now NH2", c + 1));
    press(&mut harness, at);
    let z = harness.state().atomic_numbers();
    assert_eq!((z.len(), z.iter().filter(|&&x| x == 1).count()), (7, 5));
    // H on a hydrogen: refused, said in amber, nothing done.
    harness.state_mut().build_pick("element:H");
    let (h, at) = leftmost(&harness, 1);
    hover(&mut harness, at);
    assert_eq!(harness.state().build_add_hint().unwrap(), format!("H{} is already a hydrogen", h + 1));
    press(&mut harness, at);
    assert_eq!(harness.state().atomic_numbers().len(), 7);
}

#[test]
fn molecules_go_in_empty_space() {
    let mut harness = ethane();
    open_build(&mut harness);
    harness.state_mut().build_pick("molecule-water");
    let (_, on_atom) = leftmost(&harness, 1);
    hover(&mut harness, on_atom);
    assert_eq!(harness.state().build_add_hint().unwrap(), "H2O is a whole molecule: click in empty space to add it");
    let at = empty_space(&harness);
    hover(&mut harness, at);
    assert_eq!(harness.state().build_add_hint().unwrap(), "Click to add H2O here");
    press(&mut harness, at);
    let z = harness.state().atomic_numbers();
    assert_eq!(z.len(), 11);
    // Centred under the pointer, apart from the ethane.
    let screen = harness.state().atom_window_positions();
    let centre = screen[8..].iter().fold([0.0f32; 2], |c, p| [c[0] + p[0] / 3.0, c[1] + p[1] / 3.0]);
    assert!((centre[0] - at.x).abs() < 2.0 && (centre[1] - at.y).abs() < 2.0, "{centre:?} vs {at:?}");
    // A group in empty space gets a hydrogen on its open bond.
    harness.state_mut().build_pick("tert-butyl");
    let at = empty_space(&harness);
    hover(&mut harness, at);
    assert_eq!(harness.state().build_add_hint().unwrap(), "Click to add tBu here (as C4H10)");
}

#[test]
fn hydrogens_mirror_images_and_new_molecules() {
    let mut harness = ethane();
    open_build(&mut harness);
    let start = harness.state().atom_positions();
    click(&mut harness, "Remove hydrogens");
    assert_eq!(harness.state().atomic_numbers(), vec![6, 6]);
    click(&mut harness, "Add hydrogens");
    assert_eq!(harness.state().atomic_numbers().len(), 8);
    undo(&mut harness);
    undo(&mut harness);
    assert_eq!(harness.state().atom_positions(), start);
    click(&mut harness, "Mirror");
    assert_ne!(harness.state().atom_positions(), start);
    click(&mut harness, "Mirror");
    for (a, b) in harness.state().atom_positions().iter().zip(&start) {
        assert!((0..3).all(|k| (a[k] - b[k]).abs() < 1e-4), "{a:?} vs {b:?}");
    }
    // A new molecule from what is picked: tBu gives isobutane.
    harness.state_mut().build_pick("tert-butyl");
    click(&mut harness, "New molecule");
    assert_eq!(harness.state().active_structure_label().as_deref(), Some("new molecule"));
    assert_eq!(harness.state().atomic_numbers().len(), 14);
}

#[test]
fn a_new_molecule_comes_back_from_its_session() {
    let mut harness = app_with(Vec::new());
    click(&mut harness, "Build");
    harness.state_mut().build_pick("element:C");
    click(&mut harness, "New molecule");
    assert_eq!(harness.state().atomic_numbers(), vec![6, 1, 1, 1, 1]);
    harness.state_mut().build_pick("phenyl");
    let (_, at) = leftmost(&harness, 1);
    press(&mut harness, at);
    assert_eq!(harness.state().atomic_numbers().len(), 15, "toluene");
    // Selections aren't saved: clear it (Esc: Select, then the selection).
    harness.key_press(egui::Key::Escape);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    let dir = scratch_dir("build-add-session");
    let session = dir.join("toluene.a3dview");
    harness.state().save_session_to(&session).unwrap();
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("version = 2"));
    let picture = harness.state_mut().render_image(320, 240);
    let positions = harness.state().atom_positions();
    let mut reopened = app_with(vec![session]);
    assert_eq!(reopened.state().atom_positions(), positions);
    assert_eq!(reopened.state_mut().render_image(320, 240), picture);
}

#[test]
fn own_groups_are_saved_and_offered() {
    let mut harness = ethane();
    let dir = scratch_dir("build-add-groups");
    let _ = std::fs::remove_dir_all(&dir);
    harness.state_mut().set_groups_folder(dir.clone());
    open_build(&mut harness);
    click(&mut harness, "Mine");
    assert!(status_has(&harness, "No groups of your own yet"));
    // One carbon and its three hydrogens: a methyl, bonded by one bond.
    let z = harness.state().atomic_numbers();
    let c = z.iter().position(|&x| x == 6).unwrap();
    let mut methyl = vec![c];
    // (drawn_bonds gives 1-based pairs)
    let bonds: Vec<(usize, usize)> = harness.state().drawn_bonds().iter().map(|&[a, b]| (a - 1, b - 1)).collect();
    methyl.extend(bonds.iter().filter_map(|&(a, b)| if a == c && z[b] == 1 { Some(b) } else if b == c && z[a] == 1 { Some(a) } else { None }));
    harness.state_mut().select_atoms(&methyl);
    harness.run_steps(2);
    harness.state_mut().build_set_group_name("my methyl");
    click(&mut harness, "Save selection as a group");
    assert!(dir.join("my methyl.xyz").exists(), "{:?}", harness.state().toast_texts());
    click(&mut harness, "my methyl");
    let (h, at) = leftmost(&harness, 1);
    hover(&mut harness, at);
    assert_eq!(harness.state().build_add_hint().unwrap(), format!("Click to replace H{} by my methyl", h + 1));
    press(&mut harness, at);
    assert_eq!(harness.state().atomic_numbers().len(), 11, "propane from an own methyl");
}
