//! The Build window: edit the active structure's geometry. Edits go to a
//! copy (`build.rs`), and act on the selection.
//!
//! Area: Build (see `areas.rs`).

use apost3dview_build::{moving_atoms, Moves, Which};
use apost3dview_core::{atomic_number_from_symbol, element_data, LengthUnit};
use egui::RichText;

use crate::app::App;
use crate::build::drawn_graph;
use crate::windows::ToolWindow;

/// The elements offered as buttons; the others by symbol.
const COMMON_ELEMENTS: [u32; 10] = [1, 6, 7, 8, 9, 15, 16, 17, 35, 53];

/// The blue that marks Build mode: the view's frame and the status bar.
pub(crate) const BUILD_BLUE: egui::Color32 = egui::Color32::from_rgb(40, 130, 200);

/// Parses a number typed in a field (a decimal comma is accepted).
fn number(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok().filter(|v: &f64| v.is_finite())
}

impl App {
    pub(crate) fn show_build_window(&mut self, ctx: &egui::Context) {
        let mut open = self.window_open(ToolWindow::Build);
        let response = self.column_window(ctx, ToolWindow::Build, &mut open).show(ctx, |ui| {
            let Some(active) = self.active_structure else {
                ui.label(RichText::new("Open a structure first.").weak());
                return;
            };
            if let Some(reason) = self.not_editable() {
                ui.label(RichText::new(reason).color(ui.visuals().warn_fg_color));
                return;
            }
            self.show_build_header(ui, active);
            ui.separator();
            self.show_drag_switch(ui);
            ui.separator();
            self.show_build_edits(ui, active);
            ui.separator();
            self.show_symmetry(ui, active);
        });
        self.record_column_window(ctx, ToolWindow::Build, response.map(|r| r.response));
        self.set_window_open(ToolWindow::Build, open);
    }

    /// The copy's name, undo and redo, saving; or, before the first edit,
    /// where edits will go.
    fn show_build_header(&mut self, ui: &mut egui::Ui, active: usize) {
        let structure = &self.structures[active];
        let Some(build) = &structure.build else {
            let what = if structure.surfaces.wavefunction.is_some() || !structure.surfaces.list.is_empty() || structure.apost.results.is_some() {
                " The copy has the geometry only: its orbitals, surfaces and APOST-3D results stay with the original."
            } else {
                ""
            };
            ui.label(RichText::new(format!("Edits go to a copy of {}.{what}", structure.label)).small().weak());
            return;
        };
        let (undo, redo, saved) = (build.undo.len(), build.redo.len(), build.saved);
        ui.horizontal(|ui| {
            ui.label("Name:");
            let field = ui.add(egui::TextEdit::singleline(&mut self.structures[active].label).desired_width(f32::INFINITY));
            if std::mem::take(&mut self.build.focus_name) {
                field.request_focus();
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(undo > 0, egui::Button::new("Undo")).on_hover_text("Cmd/Ctrl+Z").clicked() {
                self.undo_edit();
            }
            if ui.add_enabled(redo > 0, egui::Button::new("Redo")).on_hover_text("Cmd/Ctrl+Shift+Z").clicked() {
                self.redo_edit();
            }
            let state = if saved { "saved" } else { "not saved" };
            ui.label(RichText::new(format!("{undo} edit{}, {state}", if undo == 1 { "" } else { "s" })).small().weak());
        });
        ui.horizontal(|ui| {
            if ui.button("Save as .xyz...").on_hover_text("Writes the coordinates; the name above is offered as the file name").clicked() {
                self.save_copy_dialog();
            }
            if ui.button("Continue in a new copy").on_hover_text("Keeps this structure as it is now and goes on editing a new copy of it").clicked() {
                self.continue_in_new_copy();
            }
        });
    }

    /// The Drag atoms switch and what a drag moves.
    fn show_drag_switch(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let button = egui::Button::new("Drag atoms (D)").selected(self.build.drag_atoms);
            if ui
                .add(button)
                .on_hover_text("While on, a drag that starts on an atom moves atoms instead of turning the view. A drag from empty space still turns it.")
                .clicked()
            {
                self.build.drag_atoms = !self.build.drag_atoms;
            }
            if self.build.drag_atoms {
                ui.label(RichText::new("Drag an atom to move it, Cmd/Ctrl+drag to turn, Shift+drag up and down for depth.").small().weak());
            }
        });
        if self.build.drag_atoms {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Drag moves:").weak());
                ui.radio_value(&mut self.build.drag_moves, crate::build::DragMoves::Selection, "The atom (or the selection)")
                    .on_hover_text("The atom grabbed, or every selected atom when the one grabbed is selected");
                ui.radio_value(&mut self.build.drag_moves, crate::build::DragMoves::Piece, "Its piece")
                    .on_hover_text("Everything bonded to the atom grabbed, such as a whole ligand. Hide a bond to cut a piece off.");
            });
        }
    }

    /// The point group, the groups the structure is close to (each with
    /// its largest atom move), and Symmetrize. The group under the pointer,
    /// or the one chosen, has its elements drawn in the view.
    fn show_symmetry(&mut self, ui: &mut egui::Ui, active: usize) {
        self.build.symmetry_hover = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Symmetry").strong());
            ui.label(RichText::new(format!("point group {}", self.point_group_text(active))).weak());
        });
        let Some(analysis) = self.symmetry(active) else { return };
        let key = crate::symmetry::geometry_key(&self.structures[active].molecule);
        let chosen = self.build.symmetry_choice.filter(|(k, _)| *k == key).map(|(_, i)| i);
        let current = analysis.group_within(apost3dview_build::PROGRAM_TOLERANCE).label.clone();
        ui.label(RichText::new("Groups it is close to, with how far atoms would move. Point at one to see its elements, click to choose it.").small().weak());
        let mut choose = None;
        egui::Grid::new("symmetry_candidates").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
            for (index, candidate) in analysis.candidates.iter().enumerate().take(14) {
                let response = ui.selectable_label(chosen == Some(index), &candidate.label).on_hover_text(candidate.describe());
                if response.hovered() {
                    self.build.symmetry_hover = Some(index);
                }
                if response.clicked() {
                    choose = Some(index);
                }
                let note = if candidate.displacement <= apost3dview_build::PROGRAM_TOLERANCE {
                    if candidate.label == current { "as it is".to_string() } else { "as it is (a lower group)".to_string() }
                } else {
                    format!("atoms move up to {}", self.length_text(candidate.displacement))
                };
                ui.label(RichText::new(note).small());
                ui.end_row();
            }
        });
        if let Some(index) = choose {
            self.build.symmetry_choice = if chosen == Some(index) { None } else { Some((key, index)) };
        }
        let chosen = self.build.symmetry_choice.filter(|(k, _)| *k == key).map(|(_, i)| i);
        if ui
            .button("Standard orientation")
            .on_hover_text("Moves the coordinates: the center of nuclear charge at the origin, the main axis of the structure's symmetry (within 0.01 \u{c5}) along z, as Gaussian does. The view turns with the molecule, so the picture stays.")
            .clicked()
        {
            self.standard_orientation();
        }
        if let Some(candidate) = chosen.and_then(|i| analysis.candidates.get(i)) {
            let moves = candidate.displacement > apost3dview_build::PROGRAM_TOLERANCE;
            if ui
                .add_enabled(moves, egui::Button::new(format!("Symmetrize to {}", candidate.label)))
                .on_hover_text("Moves the atoms so this group is exact (to about 1e-6 \u{c5}), keeping the center and the orientation")
                .on_disabled_hover_text("The structure already has this group.")
                .clicked()
            {
                let candidate = candidate.clone();
                self.symmetrize_to(&candidate);
                self.build.symmetry_choice = None;
            }
        }
    }

    /// The edits on the selection.
    fn show_build_edits(&mut self, ui: &mut egui::Ui, active: usize) {
        let selected = self.structures[active].selected_atoms.clone();
        match selected.len() {
            0 => {
                ui.label(RichText::new("Select atoms to edit: 2 for a distance, 3 for an angle, 4 for a dihedral, any to move, delete or change.").small().weak());
            }
            2..=4 => self.show_value_edit(ui, active, &selected),
            _ => {}
        }
        if selected.len() == 2 {
            self.show_turn_edit(ui, active, &selected);
        }
        if !selected.is_empty() {
            ui.separator();
            self.show_move_edit(ui);
            ui.horizontal(|ui| {
                if ui.button("Delete selected atoms").on_hover_text("Delete key").clicked() {
                    self.delete_selected_atoms();
                }
            });
            self.show_element_edit(ui);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Bonds from distances").on_hover_text("Perceive the bonds again from the distances, as when a file is opened. After an edit, bonds stay as they were.").clicked() {
                self.bonds_from_distances();
            }
        });
        ui.label(RichText::new("What moves follows the bonds drawn: hide a bond to cut there, create one to join.").small().weak());
    }

    /// The selection's distance, angle or dihedral, to type a new one.
    fn show_value_edit(&mut self, ui: &mut egui::Ui, active: usize, atoms: &[usize]) {
        let structure = &self.structures[active];
        let p = &structure.molecule.positions;
        let unit = self.coordinate_unit;
        // A digit more than measurement labels for distances: a value to set.
        let decimals = self.measurement_style.decimal_places.max(2) + usize::from(atoms.len() == 2);
        let (current, unit_text) = match *atoms {
            [a, b] => (unit.from_angstrom(apost3dview_build::distance(p, a, b)), if unit == LengthUnit::Angstrom { "\u{c5}" } else { "bohr" }),
            [a, b, c] => (apost3dview_build::angle_degrees(p, a, b, c), "\u{b0}"),
            [a, b, c, d] => (apost3dview_build::dihedral_degrees(p, a, b, c, d), "\u{b0}"),
            _ => return,
        };
        let key = (active, atoms.to_vec(), self.geometry_version.get());
        if self.build.value_for.as_ref() != Some(&key) {
            self.build.value_text = format!("{current:.decimals$}");
            self.build.value_for = Some(key);
        }
        let names: Vec<String> = atoms.iter().map(|&i| crate::names::atom_name(&structure.molecule.atomic_numbers, i)).collect();
        let graph = drawn_graph(structure);
        let side = moving_atoms(&graph, atoms, Which::Last).is_some();
        let halves = side && moving_atoms(&graph, atoms, Which::First).is_some_and(|first| {
            moving_atoms(&graph, atoms, Which::Last).is_some_and(|last| first.iter().all(|a| last.binary_search(a).is_err()))
        });
        let last = names.last().cloned().unwrap_or_default();
        let mut set = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(names.join(" - ")).strong());
            let field = ui.add(egui::TextEdit::singleline(&mut self.build.value_text).desired_width(70.0));
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                set = true;
            }
            ui.label(unit_text);
            if ui.button("Set").clicked() {
                set = true;
            }
        });
        self.show_moves(ui, &last, side, halves, true);
        if set {
            match number(&self.build.value_text) {
                Some(value) => {
                    let saved = self.build.moves;
                    if !self.moves_available(side, halves) {
                        self.build.moves = Moves::Alone;
                    }
                    self.set_selected_value(value);
                    self.build.moves = saved;
                }
                None => self.show_warning("Type a number to set."),
            }
        }
    }

    fn moves_available(&self, side: bool, halves: bool) -> bool {
        match self.build.moves {
            Moves::Side => side,
            Moves::Halves => halves,
            Moves::Alone => true,
        }
    }

    /// "Moves:" with the three choices, the unavailable ones greyed with
    /// the reason.
    fn show_moves(&mut self, ui: &mut egui::Ui, last: &str, side: bool, halves: bool, alone: bool) {
        let ring = "These atoms are in one ring (or joined another way), so the sides can't move apart. Hide a bond to cut there.";
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Moves:").weak());
            ui.add_enabled_ui(side, |ui| ui.radio_value(&mut self.build.moves, Moves::Side, format!("{last} and its side")))
                .response
                .on_disabled_hover_text(ring);
            if alone {
                ui.radio_value(&mut self.build.moves, Moves::Alone, format!("{last} alone"));
            }
            ui.add_enabled_ui(halves, |ui| ui.radio_value(&mut self.build.moves, Moves::Halves, "Both sides, half each"))
                .response
                .on_disabled_hover_text(ring);
        });
        if !self.moves_available(side, halves) && alone {
            ui.label(RichText::new(format!("{last} alone will move: {}", ring.split(". ").next().unwrap_or(ring).to_lowercase())).small().color(ui.visuals().warn_fg_color));
        }
    }

    /// Turn the second atom's side about the bond from the first.
    fn show_turn_edit(&mut self, ui: &mut egui::Ui, active: usize, atoms: &[usize]) {
        let structure = &self.structures[active];
        let names: Vec<String> = atoms.iter().map(|&i| crate::names::atom_name(&structure.molecule.atomic_numbers, i)).collect();
        let graph = drawn_graph(structure);
        let side = moving_atoms(&graph, atoms, Which::Last).is_some();
        let mut turn = false;
        ui.horizontal(|ui| {
            ui.label(format!("Turn about {} - {} by", names[0], names[1]));
            let field = ui.add(egui::TextEdit::singleline(&mut self.build.turn_text).desired_width(50.0).hint_text("120"));
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                turn = true;
            }
            ui.label("\u{b0}");
            if ui.add_enabled(side, egui::Button::new("Turn")).on_disabled_hover_text("The two atoms are in one ring: nothing can turn about this bond. Hide a bond to cut the ring.").clicked() {
                turn = true;
            }
        });
        if turn && side {
            match number(&self.build.turn_text) {
                Some(degrees) => {
                    let saved = self.build.moves;
                    if self.build.moves == Moves::Alone {
                        self.build.moves = Moves::Side;
                    }
                    self.turn_about_selected_bond(degrees);
                    self.build.moves = saved;
                }
                None => self.show_warning("Type an angle in degrees to turn by."),
            }
        }
    }

    /// Move the selected atoms by a vector.
    fn show_move_edit(&mut self, ui: &mut egui::Ui) {
        let unit = if self.coordinate_unit == LengthUnit::Angstrom { "\u{c5}" } else { "bohr" };
        let mut go = false;
        ui.horizontal(|ui| {
            ui.label("Move by");
            for (axis, text) in ["x", "y", "z"].into_iter().zip(self.build.shift_text.iter_mut()) {
                ui.add(egui::TextEdit::singleline(text).desired_width(42.0).hint_text(axis));
            }
            ui.label(unit);
            if ui.button("Move").on_hover_text("Moves the selected atoms along the molecule's x, y and z axes (those of its file)").clicked() {
                go = true;
            }
        });
        if go {
            let values: Vec<f64> = self.build.shift_text.iter().map(|t| if t.trim().is_empty() { Some(0.0) } else { number(t) }).collect::<Option<_>>().unwrap_or_default();
            if values.len() == 3 {
                self.move_selected_atoms([values[0], values[1], values[2]]);
            } else {
                self.show_warning("Type numbers to move by (empty means 0).");
            }
        }
    }

    /// Change the selected atoms' element.
    fn show_element_edit(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("Element:");
            for z in COMMON_ELEMENTS {
                if ui.small_button(element_data(z).symbol).clicked() {
                    chosen = Some(z);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Other element:");
            let field = ui.add(egui::TextEdit::singleline(&mut self.build.element_text).desired_width(32.0).hint_text("Fe"));
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.small_button("Change").on_hover_text("To the element typed").clicked() || entered) && !self.build.element_text.trim().is_empty() {
                match atomic_number_from_symbol(self.build.element_text.trim()) {
                    Some(z) if z > 0 => chosen = Some(z),
                    _ => self.show_warning(format!("{} is not an element symbol.", self.build.element_text.trim())),
                }
            }
        });
        if let Some(z) = chosen {
            self.change_selected_elements(z);
        }
    }

    /// The thin blue frame around the view while Build is open.
    pub(crate) fn paint_build_frame(&self, ui: &egui::Ui, rect: egui::Rect) {
        if self.building() {
            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(3.0, BUILD_BLUE), egui::StrokeKind::Inside);
        }
    }
}
