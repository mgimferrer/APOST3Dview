//! The Build window's work: edited copies, the edits, undo and redo, and
//! saving as .xyz. The geometry itself is computed by `apost3dview_build`.
//!
//! Nothing here changes a file or a structure opened from one: the first
//! edit makes a copy ("name (edited)") right under the original, and every
//! later edit goes to that copy while it is the active structure.
//!
//! Area: Build (see `areas.rs`).

use std::path::{Path, PathBuf};

use apost3dview_build::{Deleted, Graph, Moves};
use apost3dview_core::{Bond, CoordinateFormat, LengthUnit, MeasurementKind, Molecule, format_coordinates, perceive_bonds};
use apost3dview_render::BondVisualStyle;
use glam::Vec3;

use crate::app::App;
use crate::state::{LoadedStructure, Measurement};

/// How many edits a copy can undo.
pub(crate) const UNDO_LIMIT: usize = 200;

/// The Build area's app-wide state: the window's fields.
#[derive(Default)]
pub(crate) struct BuildArea {
    /// What moves when a value is set.
    pub(crate) moves: Moves,
    /// The value field and what it was filled for (structure, selection,
    /// geometry), so it is filled again when any of those change.
    pub(crate) value_text: String,
    pub(crate) value_for: Option<(usize, Vec<usize>, u64)>,
    /// Turn about a bond, degrees.
    pub(crate) turn_text: String,
    /// Move by a vector, in the distance unit.
    pub(crate) shift_text: [String; 3],
    /// Another element, by symbol.
    pub(crate) element_text: String,
    /// Put the keyboard in the name field at the next frame (a new copy).
    pub(crate) focus_name: bool,
    /// The Drag atoms switch: off whenever the Build window is closed, so
    /// a drag outside Build mode always turns the view.
    pub(crate) drag_atoms: bool,
    /// What a drag moves.
    pub(crate) drag_moves: DragMoves,
    /// The drag under way, if any.
    pub(crate) drag: Option<AtomDrag>,
    /// The group chosen in the Symmetry list (index into the candidates
    /// of the geometry with this key), and the one under the pointer: their
    /// elements are drawn in the view.
    pub(crate) symmetry_choice: Option<(u64, usize)>,
    pub(crate) symmetry_hover: Option<usize>,
    /// How Clean works.
    pub(crate) clean: CleanSettings,
    /// The Add section: what clicks add, and what a click would do.
    pub(crate) add: crate::build_add::AddState,
}

/// The Clean part of the Build window.
pub(crate) struct CleanSettings {
    /// Keep the structure's symmetry (the highest group within 0.01 Å).
    pub(crate) keep_symmetry: bool,
    /// The selected atoms don't move.
    pub(crate) hold_selected: bool,
    /// Metals: bond lengths from covalent radii and ideal polyhedron angles
    /// (otherwise kept as they are).
    pub(crate) ideal_metals: bool,
    /// Single bonds turned toward staggered.
    pub(crate) staggered: bool,
}

impl Default for CleanSettings {
    fn default() -> Self {
        Self { keep_symmetry: true, hold_selected: false, ideal_metals: false, staggered: true }
    }
}

/// What a drag on an atom moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DragMoves {
    /// The atom grabbed, or the whole selection when it is selected.
    #[default]
    Selection,
    /// Everything bonded to the atom grabbed (the bonds drawn).
    Piece,
}

/// How a drag moves its atoms, decided when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DragKind {
    /// In the plane of the screen.
    Move,
    /// Turned about their center (Cmd/Ctrl).
    Turn,
    /// Toward or away from the viewer (Shift, vertical).
    Depth,
}

/// A drag of atoms under way, in the copy `structure`.
pub(crate) struct AtomDrag {
    structure: usize,
    /// The structure the drag started on, when the drag made the copy (to
    /// take the copy away again if the drag is cancelled).
    made_copy_of: Option<usize>,
    grabbed: usize,
    atoms: Vec<usize>,
    kind: DragKind,
    before: Snapshot,
    start: Vec<Vec3>,
    /// Where the pointer pressed, in points.
    origin: egui::Pos2,
    /// What the status bar says.
    pub(crate) text: String,
}

/// A structure made in the Build window: where it came from, whether it is
/// saved, and its undo history.
pub(crate) struct StructureBuild {
    /// The file the first copy was made from (the original's file).
    pub(crate) from: Option<PathBuf>,
    /// Saved as .xyz since the last edit (then `source_path` is that file).
    pub(crate) saved: bool,
    pub(crate) undo: Vec<Snapshot>,
    pub(crate) redo: Vec<Snapshot>,
}

/// Everything an edit can change in a structure.
#[derive(Clone)]
pub(crate) struct Snapshot {
    molecule: Molecule,
    hidden_atoms: std::collections::HashSet<usize>,
    hidden_bonds: std::collections::HashSet<usize>,
    bond_styles: Vec<BondVisualStyle>,
    perceived_bonds: usize,
    selected_atoms: Vec<usize>,
    selected_bonds: Vec<usize>,
    measurements: Vec<Measurement>,
}

impl Snapshot {
    fn of(s: &LoadedStructure) -> Self {
        Self {
            molecule: s.molecule.clone(),
            hidden_atoms: s.hidden_atoms.clone(),
            hidden_bonds: s.hidden_bonds.clone(),
            bond_styles: s.bond_styles.clone(),
            perceived_bonds: s.perceived_bonds,
            selected_atoms: s.selected_atoms.clone(),
            selected_bonds: s.selected_bonds.clone(),
            measurements: s.measurements.clone(),
        }
    }

    fn restore(self, s: &mut LoadedStructure) {
        s.molecule = self.molecule;
        s.hidden_atoms = self.hidden_atoms;
        s.hidden_bonds = self.hidden_bonds;
        s.bond_styles = self.bond_styles;
        s.perceived_bonds = self.perceived_bonds;
        s.selected_atoms = self.selected_atoms;
        s.selected_bonds = self.selected_bonds;
        s.measurements = self.measurements;
    }
}

/// An edit computed on a structure, applied to its copy.
pub(crate) enum Edited {
    Positions(Vec<Vec3>),
    Deleted(Deleted),
    Elements(Molecule),
    Bonds(Vec<Bond>),
    /// Atoms added (and maybe one replaced): the new atoms end up selected.
    Changed(apost3dview_build::Placement),
}

/// The bonds drawn (the ones sides follow): hidden bonds cut, created
/// ones join.
pub(crate) fn drawn_graph(s: &LoadedStructure) -> Graph {
    let hidden = s.drawn_hidden_bonds();
    Graph::new(
        s.molecule.atomic_numbers.len(),
        s.molecule.bonds.iter().enumerate().filter(|(k, _)| !hidden.contains(k)).map(|(_, b)| (b.atom_a, b.atom_b)),
    )
}

/// `name` made safe as a file name: characters a file system refuses
/// become "-".
pub(crate) fn file_name_from(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() { "molecule".to_string() } else { cleaned }
}

/// The name of a copy of `label`: its file name without the extension,
/// then " (edited)".
fn copy_base(label: &str) -> String {
    let mut stem = label;
    if stem.ends_with(')')
        && let Some(k) = stem.rfind(" (edited")
    {
        stem = &stem[..k];
    }
    let stem = match stem.rsplit_once('.') {
        Some((name, extension)) if !name.is_empty() && (1..=8).contains(&extension.len()) && extension.chars().all(|c| c.is_ascii_alphanumeric()) => name,
        _ => stem,
    };
    format!("{stem} (edited)")
}


impl App {
    /// Whether the Build window is open (the mode is on).
    pub(crate) fn building(&self) -> bool {
        self.window_open(crate::windows::ToolWindow::Build)
    }

    /// A name not used by any open structure: `base`, else "base 2", ...
    fn unused_name(&self, base: &str) -> String {
        if !self.structures.iter().any(|s| s.label == base) {
            return base.to_string();
        }
        (2..).map(|k| format!("{} {k})", base.trim_end_matches(')'))).find(|name| !self.structures.iter().any(|s| &s.label == name)).unwrap()
    }

    /// Puts `structure` at `index`, keeping the panels and the active
    /// structure pointing where they did.
    fn insert_structure(&mut self, index: usize, structure: LoadedStructure) {
        self.structures.insert(index, structure);
        for panel in self.render_export.figure.panels.iter_mut().filter(|p| p.structure >= index) {
            panel.structure += 1;
        }
        if let Some(active) = self.active_structure
            && active >= index
        {
            self.active_structure = Some(active + 1);
        }
    }

    /// A copy of structure `index` as it is shown (geometry, bonds and how
    /// they are drawn, measurements), placed right under it and made the
    /// active structure. Orbitals, surfaces and results stay with the
    /// original. Returns the copy's index.
    fn make_copy(&mut self, index: usize, label: String) -> usize {
        let source = &self.structures[index];
        let mut copy = LoadedStructure::new(label, source.molecule.clone(), None);
        copy.hidden_atoms = source.hidden_atoms.clone();
        copy.hidden_bonds = source.hidden_bonds.clone();
        copy.bond_styles = source.bond_styles.clone();
        copy.perceived_bonds = source.perceived_bonds;
        copy.selected_atoms = source.selected_atoms.clone();
        copy.selected_bonds = source.selected_bonds.clone();
        copy.measurements = source.measurements.clone();
        let from = source.build.as_ref().map_or_else(|| source.source_path.clone(), |b| b.from.clone());
        copy.build = Some(StructureBuild { from, saved: false, undo: Vec::new(), redo: Vec::new() });
        self.insert_structure(index + 1, copy);
        self.set_active(index + 1);
        index + 1
    }

    /// The structure an edit goes to: the active one when it is a copy,
    /// else a new copy of it.
    fn edit_target(&mut self, active: usize) -> usize {
        if self.structures[active].build.is_some() {
            return active;
        }
        let name = self.unused_name(&copy_base(&self.structures[active].label));
        let index = self.make_copy(active, name.clone());
        self.show_status(format!("Edits go to a copy: {name}. The original is unchanged."));
        index
    }

    /// Why the active structure can't be edited, if it can't.
    pub(crate) fn not_editable(&self) -> Option<&'static str> {
        let structure = self.active_structure.and_then(|i| self.structures.get(i))?;
        structure
            .frames
            .is_some()
            .then_some("A path's frames can't be edited. Open each frame as a structure (Molecule window) to edit one.")
    }

    /// Computes an edit on the active structure and, if it can be made,
    /// applies it to the copy (made at the first edit), as one undoable
    /// step. Shows the reason when it can't be made.
    pub(crate) fn edit(&mut self, compute: impl FnOnce(&LoadedStructure) -> Result<Edited, String>) -> bool {
        let Some(active) = self.active_structure else { return false };
        if let Some(reason) = self.not_editable() {
            self.show_warning(reason);
            return false;
        }
        let edited = match compute(&self.structures[active]) {
            Ok(edited) => edited,
            Err(reason) => {
                self.show_warning(reason);
                return false;
            }
        };
        let index = self.edit_target(active);
        let structure = &mut self.structures[index];
        let before = Snapshot::of(structure);
        apply(structure, edited);
        let build = structure.build.as_mut().expect("an edit target is a copy");
        build.undo.push(before);
        if build.undo.len() > UNDO_LIMIT {
            build.undo.remove(0);
        }
        build.redo.clear();
        build.saved = false;
        self.after_edit();
        true
    }

    /// Redraws everything an edit can change.
    fn after_edit(&mut self) {
        self.rebuild_geometry();
        self.rebuild_highlights();
        self.rebuild_measurements();
        self.build.value_for = None;
    }

    /// Takes back the last edit of the active copy.
    pub(crate) fn undo_edit(&mut self) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let Some(build) = structure.build.as_mut() else { return };
        let Some(previous) = build.undo.pop() else { return };
        let now = Snapshot::of(structure);
        previous.restore(structure);
        let build = structure.build.as_mut().unwrap();
        build.redo.push(now);
        build.saved = false;
        self.after_edit();
    }

    /// Makes again the last edit taken back.
    pub(crate) fn redo_edit(&mut self) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let Some(build) = structure.build.as_mut() else { return };
        let Some(next) = build.redo.pop() else { return };
        let now = Snapshot::of(structure);
        next.restore(structure);
        let build = structure.build.as_mut().unwrap();
        build.undo.push(now);
        build.saved = false;
        self.after_edit();
    }

    /// A new copy of the active copy as it is now, made the active
    /// structure, with the keyboard in its name: to branch. The copy left
    /// behind keeps its name and history.
    pub(crate) fn continue_in_new_copy(&mut self) {
        let Some(active) = self.active_structure else { return };
        if self.structures[active].build.is_none() {
            return;
        }
        let name = self.unused_name(&copy_base(&self.structures[active].label));
        self.make_copy(active, name.clone());
        self.build.focus_name = true;
        self.show_status(format!("Continuing in a new copy: {name}"));
    }

    /// Writes the active copy to `path` as .xyz; the copy then refers to
    /// that file (sessions point to it).
    pub(crate) fn save_copy_as_xyz(&mut self, path: &Path) -> bool {
        let Some(active) = self.active_structure else { return false };
        let structure = &self.structures[active];
        let from = structure.build.as_ref().and_then(|b| b.from.as_ref()).map(|p| crate::util::file_display_name(p));
        let comment = match from {
            Some(from) => format!("{}, edited in APOST-3Dview from {from}", structure.label),
            None => format!("{}, made in APOST-3Dview", structure.label),
        };
        let text = format_coordinates(&structure.molecule, LengthUnit::Angstrom, CoordinateFormat::XyzFile, &comment);
        match std::fs::write(path, text) {
            Ok(()) => {
                let structure = &mut self.structures[active];
                structure.source_path = Some(path.to_path_buf());
                if let Some(build) = structure.build.as_mut() {
                    build.saved = true;
                }
                self.recent.add(path);
                self.show_status(format!("Saved as {}", crate::util::file_display_name(path)));
                true
            }
            Err(err) => {
                self.show_warning(format!("Could not save {}: {err}", path.display()));
                false
            }
        }
    }

    /// Save as .xyz...: asks where, offering the copy's name.
    pub(crate) fn save_copy_dialog(&mut self) {
        let Some(active) = self.active_structure else { return };
        let structure = &self.structures[active];
        let name = format!("{}.xyz", file_name_from(&structure.label));
        let folder = structure.source_path.as_ref().or_else(|| structure.build.as_ref().and_then(|b| b.from.as_ref())).and_then(|p| p.parent()).map(Path::to_path_buf);
        let mut dialog = rfd::FileDialog::new().add_filter("XYZ coordinates", &["xyz"]).set_file_name(name);
        if let Some(folder) = folder {
            dialog = dialog.set_directory(folder);
        }
        if let Some(path) = dialog.save_file() {
            self.save_copy_as_xyz(&path);
        }
    }

    /// The Build window's keys, while it is open and no field has the
    /// keyboard: undo, redo, delete the selected atoms.
    pub(crate) fn handle_build_keys(&mut self, ctx: &egui::Context) {
        if !self.building() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let redo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
        let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
        if ctx.input_mut(|i| i.consume_shortcut(&redo)) {
            self.redo_edit();
        } else if ctx.input_mut(|i| i.consume_shortcut(&undo)) {
            self.undo_edit();
        }
        let find = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F);
        if ctx.input_mut(|i| i.consume_shortcut(&find)) {
            self.build.add.focus_find = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::D) && i.modifiers.is_none()) {
            self.build.drag_atoms = !self.build.drag_atoms;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.is_none()) {
            self.clean_structure();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.shift_only()) {
            self.rebuild_structure();
        }
        let delete = ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
        if delete && self.active_structure.is_some_and(|i| !self.structures[i].selected_atoms.is_empty()) {
            self.delete_selected_atoms();
        }
    }

    /// Clean: the active structure tidied (one edit). See
    /// `apost3dview_build::clean`.
    pub(crate) fn clean_structure(&mut self) {
        let Some(active) = self.active_structure else { return };
        let settings = &self.build.clean;
        let symmetry = if settings.keep_symmetry {
            self.symmetry(active).and_then(|analysis| {
                let exact = analysis.group_within(apost3dview_build::PROGRAM_TOLERANCE);
                let group = analysis.highest_above(exact, crate::symmetry::ORIENTATION_TOLERANCE).unwrap_or(exact);
                (!group.operations.is_empty() && group.order > 1).then(|| (group.label.clone(), apost3dview_build::KeptSymmetry {
                    center: analysis.center,
                    operations: group.operations.clone(),
                    permutations: group.permutations.clone(),
                }))
            })
        } else {
            None
        };
        let held = if settings.hold_selected { self.structures[active].selected_atoms.clone() } else { Vec::new() };
        let options = apost3dview_build::CleanOptions { held, ideal_metals: settings.ideal_metals, twist: settings.staggered, symmetry: symmetry.as_ref().map(|(_, s)| s.clone()) };
        let mut moved = 0.0;
        let done = self.edit(|s| {
            let hidden = s.drawn_hidden_bonds();
            let bonds: Vec<(usize, usize)> = s.molecule.bonds.iter().enumerate().filter(|(k, _)| !hidden.contains(k)).map(|(_, b)| (b.atom_a, b.atom_b)).collect();
            let result = apost3dview_build::clean(&s.molecule.atomic_numbers, &s.molecule.positions, &bonds, &options);
            moved = result.largest_move;
            Ok(Edited::Positions(result.positions))
        });
        if done {
            let kept = symmetry.map(|(label, _)| format!(", {label} kept")).unwrap_or_default();
            let moved = self.length_text(moved);
            self.show_status(format!("Cleaned{kept}: atoms moved up to {moved}"));
        }
    }

    /// Rebuild: a sensible structure from the bonds alone, for drawings too
    /// poor for Clean (one edit). See `apost3dview_build::rebuild`.
    pub(crate) fn rebuild_structure(&mut self) {
        let done = self.edit(|s| {
            let hidden = s.drawn_hidden_bonds();
            let bonds: Vec<(usize, usize)> = s.molecule.bonds.iter().enumerate().filter(|(k, _)| !hidden.contains(k)).map(|(_, b)| (b.atom_a, b.atom_b)).collect();
            if bonds.is_empty() {
                return Err("There are no bonds to rebuild from: create them first (or open the structure again).".into());
            }
            Ok(Edited::Positions(apost3dview_build::rebuild(&s.molecule.atomic_numbers, &s.molecule.positions, &bonds, 16)))
        });
        if done {
            self.show_status("Rebuilt from the bonds: check them, and the result, before a calculation");
        }
    }

    pub(crate) fn delete_selected_atoms(&mut self) {
        self.edit(|s| {
            if s.selected_atoms.is_empty() {
                return Err("Select the atoms to delete first.".into());
            }
            Ok(Edited::Deleted(apost3dview_build::delete_atoms(&s.molecule, &s.selected_atoms)))
        });
    }

    pub(crate) fn change_selected_elements(&mut self, atomic_number: u32) {
        self.edit(|s| {
            if s.selected_atoms.is_empty() {
                return Err("Select the atoms to change first.".into());
            }
            Ok(Edited::Elements(apost3dview_build::change_element(&s.molecule, &s.selected_atoms, atomic_number)))
        });
    }

    pub(crate) fn bonds_from_distances(&mut self) {
        self.edit(|s| Ok(Edited::Bonds(perceive_bonds(&s.molecule.atomic_numbers, &s.molecule.positions))));
    }

    /// Sets the measurement of the selected atoms (2: distance in the
    /// distance unit, 3: angle, 4: dihedral, degrees) to `value`.
    pub(crate) fn set_selected_value(&mut self, value: f64) {
        let unit = self.coordinate_unit;
        let moves = self.build.moves;
        self.edit(|s| {
            let graph = drawn_graph(s);
            let p = &s.molecule.positions;
            let result = match *s.selected_atoms.as_slice() {
                [a, b] => apost3dview_build::set_distance(p, &graph, a, b, unit.to_angstrom(value), moves),
                [a, b, c] => apost3dview_build::set_angle(p, &graph, a, b, c, value, moves),
                [a, b, c, d] => apost3dview_build::set_dihedral(p, &graph, [a, b, c, d], value, moves),
                _ => return Err("Select 2, 3 or 4 atoms: a distance, an angle or a dihedral.".into()),
            };
            result.map(Edited::Positions).map_err(|e| format!("Could not set it: {e}."))
        });
    }

    /// Turns the side of the second selected atom about the bond from the
    /// first by `degrees`.
    pub(crate) fn turn_about_selected_bond(&mut self, degrees: f64) {
        let moves = self.build.moves;
        self.edit(|s| {
            let &[a, b] = s.selected_atoms.as_slice() else { return Err("Select the two atoms of the bond to turn about.".into()) };
            apost3dview_build::rotate_about_bond(&s.molecule.positions, &drawn_graph(s), a, b, degrees, moves)
                .map(Edited::Positions)
                .map_err(|e| format!("Could not turn it: {e}."))
        });
    }

    /// Moves the selected atoms by `shift`, in the distance unit.
    pub(crate) fn move_selected_atoms(&mut self, shift: [f64; 3]) {
        let unit = self.coordinate_unit;
        self.edit(|s| {
            if s.selected_atoms.is_empty() {
                return Err("Select the atoms to move first.".into());
            }
            let shift = shift.map(|v| unit.to_angstrom(v));
            apost3dview_build::translate(&s.molecule.positions, &s.selected_atoms, shift).map(Edited::Positions).map_err(|e| format!("Could not move them: {e}."))
        });
    }
}

/// Degrees of turn per point of pointer movement, as the view turns.
const TURN_PER_POINT: f64 = 0.005;

impl App {
    /// Starts dragging from atom `atom` of the active structure (pressed at
    /// `origin`): makes the copy if needed and picks the atoms that move.
    pub(crate) fn start_atom_drag(&mut self, atom: usize, kind: DragKind, origin: egui::Pos2) {
        let Some(active) = self.active_structure else { return };
        if self.not_editable().is_some() {
            return;
        }
        let made_copy_of = self.structures[active].build.is_none().then_some(active);
        let index = self.edit_target(active);
        let structure = &self.structures[index];
        let atoms = match self.build.drag_moves {
            DragMoves::Selection if structure.selected_atoms.contains(&atom) => structure.selected_atoms.clone(),
            DragMoves::Selection => vec![atom],
            DragMoves::Piece => drawn_graph(structure).piece(atom),
        };
        self.build.drag = Some(AtomDrag {
            structure: index,
            made_copy_of,
            grabbed: atom,
            atoms,
            kind,
            before: Snapshot::of(structure),
            start: structure.molecule.positions.clone(),
            origin,
            text: String::new(),
        });
    }

    /// Moves the dragged atoms for the pointer at `now` (points), in a view
    /// `view_height` points tall.
    pub(crate) fn update_atom_drag(&mut self, now: egui::Pos2, view_height: f32) {
        let Some(drag) = &self.build.drag else { return };
        let delta = now - drag.origin;
        let (dx, dy) = (f64::from(delta.x), f64::from(delta.y));
        let camera = self.camera;
        let (right, up) = camera.screen_basis();
        let (right, up, forward) = (right.as_dvec3(), up.as_dvec3(), camera.forward().as_dvec3());
        let grabbed = drag.start[drag.grabbed].as_dvec3();
        let depth = (grabbed - camera.eye().as_dvec3()).dot(forward) as f32;
        let per_point = f64::from(camera.world_units_per_pixel(depth, view_height));
        let pivot = apost3dview_build::center(&drag.start, &drag.atoms);
        let unit = self.coordinate_unit;
        let unit_text = if unit == LengthUnit::Angstrom { "\u{c5}" } else { "bohr" };
        let names = {
            let z = &self.structures[drag.structure].molecule.atomic_numbers;
            let first = crate::names::atom_name(z, drag.grabbed);
            match drag.atoms.len() {
                1 => first,
                n => format!("{first} and {} atom{}", n - 1, if n == 2 { "" } else { "s" }),
            }
        };
        let (rotation, shift, text) = match drag.kind {
            DragKind::Move => {
                let shift = (right * dx - up * dy) * per_point;
                (glam::DQuat::IDENTITY, shift, format!("Moving {names}: {:.2} {unit_text}", unit.from_angstrom(shift.length())))
            }
            DragKind::Depth => {
                let shift = -forward * dy * per_point;
                let toward = if dy > 0.0 { "toward you" } else { "away" };
                (glam::DQuat::IDENTITY, shift, format!("Moving {names} {toward}: {:.2} {unit_text}", unit.from_angstrom(shift.length())))
            }
            DragKind::Turn => {
                let rotation = glam::DQuat::from_axis_angle(up, dx * TURN_PER_POINT) * glam::DQuat::from_axis_angle(right, dy * TURN_PER_POINT);
                (rotation, glam::DVec3::ZERO, format!("Turning {names}: {:.0}\u{b0}", rotation.to_axis_angle().1.to_degrees()))
            }
        };
        let positions = apost3dview_build::transform_atoms(&drag.start, &drag.atoms, rotation, pivot, shift).unwrap_or_else(|_| drag.start.clone());
        let index = drag.structure;
        if let Some(drag) = self.build.drag.as_mut() {
            drag.text = text;
        }
        if self.structures[index].molecule.positions != positions {
            self.structures[index].molecule.positions = positions;
            self.rebuild_geometry();
            self.rebuild_highlights();
            self.rebuild_measurements();
        }
    }

    /// Ends the drag: one undoable edit, if anything moved.
    pub(crate) fn finish_atom_drag(&mut self) {
        let Some(drag) = self.build.drag.take() else { return };
        let structure = &mut self.structures[drag.structure];
        if structure.molecule.positions == drag.start {
            return;
        }
        if let Some(build) = structure.build.as_mut() {
            build.undo.push(drag.before);
            if build.undo.len() > UNDO_LIMIT {
                build.undo.remove(0);
            }
            build.redo.clear();
            build.saved = false;
        }
        self.build.value_for = None;
    }

    /// Esc during a drag: the atoms go back, and a copy made by this drag
    /// goes away again.
    pub(crate) fn cancel_atom_drag(&mut self) {
        let Some(drag) = self.build.drag.take() else { return };
        self.structures[drag.structure].molecule.positions = drag.start;
        match drag.made_copy_of {
            Some(original) => {
                self.close_structure(drag.structure);
                self.set_active(original);
            }
            None => {
                self.rebuild_geometry();
                self.rebuild_highlights();
                self.rebuild_measurements();
            }
        }
    }
}

/// Applies a computed edit to a structure, carrying its per-atom and
/// per-bond state along.
fn apply(s: &mut LoadedStructure, edited: Edited) {
    match edited {
        Edited::Positions(positions) => s.molecule.positions = positions,
        Edited::Elements(molecule) => s.molecule = molecule,
        Edited::Bonds(bonds) => {
            s.bond_styles = vec![BondVisualStyle::Single; bonds.len()];
            s.perceived_bonds = bonds.len();
            s.hidden_bonds.clear();
            s.selected_bonds.clear();
            s.molecule.bonds = bonds;
        }
        Edited::Deleted(Deleted { molecule, atom_map, bond_map }) => remap(s, molecule, &atom_map, &bond_map),
        Edited::Changed(placement) => {
            remap(s, placement.molecule, &placement.atom_map, &placement.bond_map);
            // New bonds (after the old ones) are drawn plainly.
            s.bond_styles.resize(s.molecule.bonds.len(), BondVisualStyle::Single);
            if !placement.added.is_empty() {
                s.selected_atoms = placement.added;
                s.selected_bonds.clear();
            }
        }
    }
}

/// Puts `molecule` in place of the structure's, carrying every per-atom and
/// per-bond state through the maps (`None`: that atom or bond is gone).
fn remap(s: &mut LoadedStructure, molecule: Molecule, atom_map: &[Option<usize>], bond_map: &[Option<usize>]) {
    let atom = |a: usize| atom_map.get(a).copied().flatten();
    let bond = |b: usize| bond_map.get(b).copied().flatten();
    s.hidden_atoms = s.hidden_atoms.iter().filter_map(|&a| atom(a)).collect();
    s.hidden_bonds = s.hidden_bonds.iter().filter_map(|&b| bond(b)).collect();
    s.bond_styles = s.bond_styles.iter().enumerate().filter(|(k, _)| bond(*k).is_some()).map(|(_, style)| *style).collect();
    s.perceived_bonds = (0..s.perceived_bonds).filter(|&k| bond(k).is_some()).count();
    s.selected_atoms = s.selected_atoms.iter().filter_map(|&a| atom(a)).collect();
    s.selected_bonds = s.selected_bonds.iter().filter_map(|&b| bond(b)).collect();
    s.measurements = s
        .measurements
        .iter()
        .filter_map(|m| {
            let atoms: Option<Vec<usize>> = m.kind.atoms().into_iter().map(atom).collect();
            let kind = MeasurementKind::from_picks(&atoms?)?;
            Some(Measurement { kind, label_offset: m.label_offset })
        })
        .collect();
    s.molecule = molecule;
}

/// Test hooks of the Build area.
impl App {
    /// Sets the selected atoms' distance, angle or dihedral, as typing it
    /// in the Build window and pressing Set would — for tests.
    pub fn build_set_value(&mut self, value: f64) {
        self.set_selected_value(value);
    }

    /// Chooses what moves: "side", "alone" or "halves" — for tests.
    pub fn build_set_moves(&mut self, moves: &str) {
        self.build.moves = match moves {
            "alone" => Moves::Alone,
            "halves" => Moves::Halves,
            _ => Moves::Side,
        };
    }

    /// Saves the active copy as .xyz at `path`, as Save as .xyz... would
    /// after choosing it — for tests.
    pub fn build_save_xyz(&mut self, path: &Path) -> bool {
        self.save_copy_as_xyz(path)
    }

    /// Where the active structure's atoms are in the window (points), as
    /// last drawn — for tests that point at them.
    pub fn atom_window_positions(&self) -> Vec<[f32; 2]> {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return Vec::new() };
        let rect = self.last_viewport_rect;
        let aspect_ratio = rect.width() / rect.height();
        structure
            .molecule
            .positions
            .iter()
            .map(|p| crate::util::project_to_screen(&self.camera, aspect_ratio, rect, *p).map_or([f32::NAN; 2], |q| [q.x, q.y]))
            .collect()
    }

    /// Whether the Drag atoms switch is on — for tests.
    pub fn drag_atoms_on(&self) -> bool {
        self.build.drag_atoms
    }

    /// Renames the active copy, as typing in the Build window's Name field
    /// would — for tests.
    pub fn rename_active_structure(&mut self, name: &str) {
        if let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i))
            && structure.build.is_some()
        {
            structure.label = name.to_string();
        }
    }

    /// The active structure's atomic numbers — for tests.
    pub fn atomic_numbers(&self) -> Vec<u32> {
        self.active_structure.and_then(|i| self.structures.get(i)).map(|s| s.molecule.atomic_numbers.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_base, file_name_from};

    #[test]
    fn copies_are_named_after_their_original() {
        assert_eq!(copy_base("ethane.fchk"), "ethane (edited)");
        assert_eq!(copy_base("FeCN5NO3--UBLYP.fchk"), "FeCN5NO3--UBLYP (edited)");
        assert_eq!(copy_base("ethane (edited)"), "ethane (edited)");
        assert_eq!(copy_base("ethane (edited 2)"), "ethane (edited)");
        assert_eq!(copy_base("staggered ethane"), "staggered ethane (edited)");
        assert_eq!(file_name_from("Fe(CO)5: trans / axial?"), "Fe(CO)5- trans - axial-");
    }
}
