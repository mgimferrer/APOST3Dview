//! Molecule window: the open structures, how the active one is drawn
//! (atom and bond size, labels, hidden atoms), and what to do with the
//! current selection.

use apost3dview_core::Bond;
use apost3dview_render::BondVisualStyle;
use egui::{RichText, Slider};

use crate::app::App;
use crate::state::AtomLabelMode;
use crate::util::find_bond_between;

impl App {
    pub(crate) fn show_molecule_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_molecule;
        egui::Window::new("Molecule")
            .open(&mut open)
            .default_pos([16.0, 50.0])
            .default_width(290.0)
            .show(ctx, |ui| {
                self.show_structure_list(ui);
                let Some(active) = self.active_structure else { return };

                ui.add_space(6.0);
                self.show_links_to_other_windows(ui, active);

                ui.add_space(6.0);
                ui.separator();
                self.show_display_settings(ui, active);

                if !self.structures[active].selected_atoms.is_empty() || !self.structures[active].selected_bonds.is_empty() {
                    ui.add_space(6.0);
                    ui.separator();
                    self.show_selection_actions(ui, active);
                }
            });
        self.show_molecule = open;
    }

    fn show_structure_list(&mut self, ui: &mut egui::Ui) {
        if self.structures.is_empty() {
            ui.label(RichText::new("Nothing open yet. Use Open in the toolbar, or drag files onto the window.").weak());
            return;
        }
        let mut switch_to = None;
        let mut close = None;
        egui::ScrollArea::vertical().id_salt("structure_list").max_height(140.0).show(ui, |ui| {
            for (index, structure) in self.structures.iter().enumerate() {
                ui.horizontal(|ui| {
                    let selected = self.active_structure == Some(index);
                    let path = structure.source_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                    if ui.selectable_label(selected, &structure.label).on_hover_text(path).clicked() {
                        switch_to = Some(index);
                    }
                    let mut notes = Vec::new();
                    if !structure.surfaces.is_empty() {
                        let n = structure.surfaces.len();
                        notes.push(format!("{n} surface{}", if n == 1 { "" } else { "s" }));
                    }
                    if structure.apost.as_ref().is_some_and(|a| a.results.is_ok()) {
                        notes.push("APOST-3D".to_string());
                    }
                    if !notes.is_empty() {
                        ui.label(RichText::new(notes.join(", ")).small().weak());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("\u{d7}").on_hover_text("Close").clicked() {
                            close = Some(index);
                        }
                    });
                });
            }
        });
        if let Some(index) = switch_to {
            self.set_active(index);
        }
        if let Some(index) = close {
            self.close_structure(index);
        }
    }

    /// One-line pointers to what else this structure has, so a new user
    /// finds orbital generation and the APOST-3D results without knowing
    /// which window holds them.
    fn show_links_to_other_windows(&mut self, ui: &mut egui::Ui, active: usize) {
        let structure = &self.structures[active];
        let has_orbitals = structure.wavefunction.is_some();
        let has_results = structure.apost.is_some();
        ui.horizontal_wrapped(|ui| {
            if has_orbitals && ui.button("Orbitals...").on_hover_text("Generate and show orbitals (Surfaces window)").clicked() {
                self.show_surfaces = true;
            }
            if has_results && ui.button("APOST-3D results...").clicked() {
                self.show_apost = true;
            }
            if ui.button("Coordinates...").on_hover_text("Cartesian coordinates, to copy or save as .xyz").clicked() {
                self.show_coordinates = true;
            }
        });
    }

    fn show_display_settings(&mut self, ui: &mut egui::Ui, active: usize) {
        ui.add(Slider::new(&mut self.material.atom_scale, 0.1..=1.5).text("atom size"));
        ui.add(Slider::new(&mut self.material.bond_radius, 0.02..=0.5).text("bond radius"));
        ui.horizontal(|ui| {
            ui.label("Labels:");
            ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::None, "None");
            ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::Number, "Number");
            ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::Type, "Element");
            ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::NumberType, "Both");
        });
        if self.atom_label_mode != AtomLabelMode::None {
            ui.horizontal(|ui| {
                ui.add(Slider::new(&mut self.atom_label_style.relative_size, 0.2..=6.0).text("label size"));
                ui.color_edit_button_srgba(&mut self.atom_label_style.text_color);
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Hide hydrogens").clicked() {
                let structure = &mut self.structures[active];
                let hydrogens: Vec<usize> = structure.molecule.atomic_numbers.iter().enumerate().filter(|&(_, &z)| z == 1).map(|(i, _)| i).collect();
                structure.hidden_atoms.extend(hydrogens);
                self.rebuild_geometry();
                self.rebuild_highlights();
            }
            if ui.button("Show all").on_hover_text("Show hidden atoms and bonds again, and make all bonds solid").clicked() {
                let structure = &mut self.structures[active];
                structure.hidden_atoms.clear();
                structure.hidden_bonds.clear();
                structure.bond_styles.iter_mut().for_each(|s| *s = BondVisualStyle::Single);
                self.rebuild_geometry();
            }
        });
    }

    /// Shown only while something is selected (click atoms or bonds in the
    /// view): what can be done with it.
    fn show_selection_actions(&mut self, ui: &mut egui::Ui, active: usize) {
        let structure = &self.structures[active];
        let atoms = structure.selected_atoms.clone();
        let bonds = structure.selected_bonds.len();
        // What is selected is named in the status bar; this lists what can
        // be done with it.
        ui.label(RichText::new("With the selection").strong());

        ui.horizontal_wrapped(|ui| {
            if ui.button("Hide").on_hover_text("Hide the selected atoms and bonds").clicked() {
                let structure = &mut self.structures[active];
                let atoms = std::mem::take(&mut structure.selected_atoms);
                let bonds = std::mem::take(&mut structure.selected_bonds);
                structure.hidden_atoms.extend(atoms);
                structure.hidden_bonds.extend(bonds);
                self.rebuild_geometry();
                self.rebuild_highlights();
            }
            // Two atoms: create a bond between them (a forming/breaking
            // contact too long for automatic perception to catch), or hide
            // or show the one that exists.
            if let [a, b] = atoms[..] {
                match find_bond_between(&self.structures[active].molecule, a, b) {
                    None => {
                        if ui.button("Create bond").clicked() {
                            let structure = &mut self.structures[active];
                            structure.molecule.bonds.push(Bond { atom_a: a, atom_b: b });
                            structure.bond_styles.push(BondVisualStyle::Single);
                            self.rebuild_geometry();
                        }
                    }
                    Some(bond_index) => {
                        let hidden = self.structures[active].hidden_bonds.contains(&bond_index);
                        if ui.button(if hidden { "Show bond" } else { "Hide bond" }).clicked() {
                            let hidden_bonds = &mut self.structures[active].hidden_bonds;
                            if hidden {
                                hidden_bonds.remove(&bond_index);
                            } else {
                                hidden_bonds.insert(bond_index);
                            }
                            self.rebuild_geometry();
                        }
                    }
                }
            }
            if (2..=4).contains(&atoms.len()) && ui.button("Add measurement").on_hover_text("Add this distance, angle or dihedral to the Measure window").clicked() {
                self.add_measurement_from_selection();
                self.show_measure = true;
            }
            if ui.button("Clear (Esc)").clicked() {
                self.clear_selection();
            }
        });
        if bonds > 0 {
            ui.horizontal(|ui| {
                ui.label("Selected bonds:");
                if ui.button("Solid").clicked() {
                    self.apply_bond_style(BondVisualStyle::Single);
                }
                if ui.button("Dashed (TS)").on_hover_text("Thin dashed line, for a bond forming or breaking in a transition state").clicked() {
                    self.apply_bond_style(BondVisualStyle::TransitionState);
                }
            });
        }
    }

    pub(crate) fn apply_bond_style(&mut self, style: BondVisualStyle) {
        let Some(active) = self.active_structure else { return };
        let selected = self.structures[active].selected_bonds.clone();
        for index in selected {
            if let Some(entry) = self.structures[active].bond_styles.get_mut(index) {
                *entry = style;
            }
        }
        self.rebuild_geometry();
    }
}
