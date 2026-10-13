//! Visualization window.

use apost3dview_core::{element_data, Bond};
use apost3dview_render::BondVisualStyle;
use egui::Slider;

use crate::app::App;
use crate::util::find_bond_between;

impl App {
    pub(crate) fn show_visualization_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_visualization;
        egui::Window::new("Visualization")
            .open(&mut open)
            .default_pos([40.0, 460.0])
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Click atoms/bonds to select them. Drag (or arrow keys) to rotate, shift+drag to pan, scroll or pinch to zoom.")
                        .small()
                        .italics(),
                );

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.auto_spin, "Auto-spin (Space)");
                    ui.add(Slider::new(&mut self.auto_spin_speed, 0.05..=3.0).text("speed"));
                });
                ui.label(egui::RichText::new("Spins in the direction of the last arrow key(s) pressed.").small().weak());

                ui.add_space(10.0);
                ui.separator();

                let Some(active) = self.active_structure else {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                };

                ui.label(egui::RichText::new("Atoms").strong());
                let summary = self.structures[active]
                    .selected_atoms
                    .iter()
                    .map(|&i| format!("{}{}", element_data(self.structures[active].molecule.atomic_numbers[i]).symbol, i + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(format!("Selected atoms: {}", self.structures[active].selected_atoms.len()));
                if !summary.is_empty() {
                    ui.label(egui::RichText::new(summary).small());
                }
                ui.horizontal(|ui| {
                    if ui.button("Hide atoms").clicked() {
                        if self.structures[active].selected_atoms.is_empty() {
                            self.show_warning("No atoms selected");
                        } else {
                            let selected = std::mem::take(&mut self.structures[active].selected_atoms);
                            self.structures[active].hidden_atoms.extend(selected);
                            self.rebuild_geometry();
                            self.rebuild_highlights();
                        }
                    }
                    if ui.button("Clear selection").clicked() {
                        self.clear_selection();
                    }
                });

                // Manual bond management: pick exactly two atoms and
                // either create a bond between them (useful for depicting
                // a forming/breaking contact too long for automatic
                // perception to catch) or toggle the visibility of one
                // that already exists.
                if self.structures[active].selected_atoms.len() == 2 {
                    let a = self.structures[active].selected_atoms[0];
                    let b = self.structures[active].selected_atoms[1];
                    let existing_bond = find_bond_between(&self.structures[active].molecule, a, b);

                    ui.add_space(6.0);
                    match existing_bond {
                        None => {
                            if ui.button("Create bond").clicked() {
                                self.structures[active].molecule.bonds.push(Bond { atom_a: a, atom_b: b });
                                self.structures[active].bond_styles.push(BondVisualStyle::Single);
                                self.rebuild_geometry();
                            }
                        }
                        Some(bond_index) => {
                            let is_hidden = self.structures[active].hidden_bonds.contains(&bond_index);
                            let label = if is_hidden { "Show bond" } else { "Hide bond" };
                            if ui.button(label).clicked() {
                                if is_hidden {
                                    self.structures[active].hidden_bonds.remove(&bond_index);
                                } else {
                                    self.structures[active].hidden_bonds.insert(bond_index);
                                }
                                self.rebuild_geometry();
                            }
                        }
                    }
                }

                ui.add_space(10.0);
                ui.separator();

                ui.label(egui::RichText::new("Bonds").strong());
                ui.label(format!("Selected bonds: {}", self.structures[active].selected_bonds.len()));
                ui.horizontal(|ui| {
                    if ui.button("Single").clicked() {
                        self.apply_bond_style(BondVisualStyle::Single);
                    }
                    if ui.button("TS").clicked() {
                        self.apply_bond_style(BondVisualStyle::TransitionState);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Hide bonds").clicked() {
                        if self.structures[active].selected_bonds.is_empty() {
                            self.show_warning("No bonds selected");
                        } else {
                            let selected = std::mem::take(&mut self.structures[active].selected_bonds);
                            self.structures[active].hidden_bonds.extend(selected);
                            self.rebuild_geometry();
                            self.rebuild_highlights();
                        }
                    }
                    if ui.button("Clear selection").clicked() {
                        self.clear_selection();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label("Global");
                ui.horizontal(|ui| {
                    if ui.button("Hide H-atoms").clicked() {
                        let hydrogens: Vec<usize> = self.structures[active]
                            .molecule
                            .atomic_numbers
                            .iter()
                            .enumerate()
                            .filter(|&(_, &z)| z == 1)
                            .map(|(i, _)| i)
                            .collect();
                        self.structures[active].hidden_atoms.extend(hydrogens);
                        self.rebuild_geometry();
                        self.rebuild_highlights();
                    }
                    if ui.button("Show all").clicked() {
                        let structure = &mut self.structures[active];
                        structure.hidden_atoms.clear();
                        structure.hidden_bonds.clear();
                        structure.bond_styles.iter_mut().for_each(|s| *s = BondVisualStyle::Single);
                        structure.selected_atoms.clear();
                        structure.selected_bonds.clear();
                        self.rebuild_geometry();
                        self.rebuild_highlights();
                    }
                });


            });
        self.show_visualization = open;
    }

    pub(crate) fn apply_bond_style(&mut self, style: BondVisualStyle) {
        let Some(active) = self.active_structure else { return };
        if self.structures[active].selected_bonds.is_empty() {
            self.show_warning("No bonds selected");
            return;
        }
        let selected = self.structures[active].selected_bonds.clone();
        for index in selected {
            if let Some(entry) = self.structures[active].bond_styles.get_mut(index) {
                *entry = style;
            }
        }
        self.rebuild_geometry();
    }
}
