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
        let delete = ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
        if delete && self.active_structure.is_some_and(|i| !self.structures[i].selected_atoms.is_empty()) {
            self.delete_selected_atoms();
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
        Edited::Deleted(Deleted { molecule, atom_map, bond_map }) => {
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
    }
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
