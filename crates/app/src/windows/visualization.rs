//! Visualization window.

use apost3dview_core::{element_data, Bond};
use apost3dview_render::BondVisualStyle;
use egui::Slider;

use crate::app::App;
use crate::orbitals::{OrbitalAccuracyPreset, show_mo_checklist};
use crate::state::LoadedStructure;
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

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Generate orbitals").strong());

                if self.structures[active].wavefunction.is_some() {
                    let num_orbitals = self.structures[active].wavefunction.as_ref().unwrap().alpha.num_orbitals();
                    let is_unrestricted = self.structures[active].wavefunction.as_ref().unwrap().beta.is_some();
                    self.orbital_generation.range_start = self.orbital_generation.range_start.clamp(1, num_orbitals.max(1));
                    self.orbital_generation.range_end = self.orbital_generation.range_end.clamp(1, num_orbitals.max(1));

                    ui.horizontal(|ui| {
                        if ui.button("HOMO/LUMO").clicked() {
                            let wfn = self.structures[active].wavefunction.as_ref().unwrap();
                            let (alpha_homo, alpha_lumo, alpha_num) = (wfn.alpha.homo_index(), wfn.alpha.lumo_index(), wfn.alpha.num_orbitals());
                            let beta_homo_lumo_num = wfn.beta.as_ref().map(|b| (b.homo_index(), b.lumo_index(), b.num_orbitals()));
                            let structure = &mut self.structures[active];
                            structure.selected_alpha_mos.clear();
                            structure.selected_alpha_mos.insert(alpha_homo - 1);
                            if alpha_lumo <= alpha_num {
                                structure.selected_alpha_mos.insert(alpha_lumo - 1);
                            }
                            structure.selected_beta_mos.clear();
                            if let Some((beta_homo, beta_lumo, beta_num)) = beta_homo_lumo_num {
                                structure.selected_beta_mos.insert(beta_homo - 1);
                                if beta_lumo <= beta_num {
                                    structure.selected_beta_mos.insert(beta_lumo - 1);
                                }
                            }
                        }
                        if ui.button("Clear").clicked() {
                            let structure = &mut self.structures[active];
                            structure.selected_alpha_mos.clear();
                            structure.selected_beta_mos.clear();
                        }
                        ui.label("Isovalue:");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.isovalue).speed(0.001).range(0.0..=f32::MAX));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Range:");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.range_start).range(1..=num_orbitals.max(1)));
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.range_end).range(1..=num_orbitals.max(1)));
                        if ui.button("Select range").clicked() {
                            let (lo, hi) = (self.orbital_generation.range_start.min(self.orbital_generation.range_end), self.orbital_generation.range_start.max(self.orbital_generation.range_end));
                            let structure = &mut self.structures[active];
                            for mo_number in lo..=hi {
                                structure.selected_alpha_mos.insert(mo_number - 1);
                                if is_unrestricted {
                                    structure.selected_beta_mos.insert(mo_number - 1);
                                }
                            }
                        }
                    });

                    let list_height = if is_unrestricted { 120.0 } else { 160.0 };
                    // Destructuring through the `&mut LoadedStructure`
                    // borrows `wavefunction`/`selected_alpha_mos`/
                    // `selected_beta_mos` as separate, disjoint fields —
                    // lets `wfn` (immutable) and the selection sets
                    // (mutable) coexist without needing two separate
                    // indexing expressions into `self.structures`, which
                    // the borrow checker can't prove disjoint on its own.
                    let LoadedStructure { wavefunction, selected_alpha_mos, selected_beta_mos, .. } = &mut self.structures[active];
                    let wfn = wavefunction.as_ref().unwrap();
                    show_mo_checklist(ui, &wfn.alpha, selected_alpha_mos, "alpha", list_height);
                    if let Some(beta) = &wfn.beta {
                        show_mo_checklist(ui, beta, selected_beta_mos, "beta", list_height);
                    }

                    ui.add_space(4.0);
                    ui.label("Accuracy:");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Low, "Low");
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Medium, "Medium");
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::High, "High");
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Custom, "Custom");
                    });
                    if self.orbital_generation.preset == OrbitalAccuracyPreset::Custom {
                        ui.horizontal(|ui| {
                            ui.label("Spacing (Bohr):");
                            ui.add(egui::DragValue::new(&mut self.orbital_generation.custom_spacing_bohr).speed(0.01).range(0.02..=2.0));
                        });
                    } else {
                        ui.label(
                            egui::RichText::new(format!("Grid spacing: {:.2} Bohr", self.orbital_generation.resolve_spacing_bohr()))
                                .small()
                                .weak(),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Box padding (Bohr):");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.padding_bohr).speed(0.1).range(1.0..=20.0));
                    });
                    ui.label(
                        egui::RichText::new("If a generated orbital's isosurface looks flat-cut at one side, raise this.")
                            .small()
                            .weak(),
                    );

                    ui.add_space(4.0);
                    let selected_count = self.structures[active].selected_alpha_mos.len() + self.structures[active].selected_beta_mos.len();
                    if ui
                        .add_sized([ui.available_width(), 26.0], egui::Button::new(format!("Generate {selected_count} orbital(s)")))
                        .clicked()
                    {
                        self.generate_selected_orbitals();
                    }
                } else {
                    ui.label(egui::RichText::new("This structure has no orbital data (open an .fchk with orbital output).").small().weak());
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Isosurfaces").strong());

                let has_isosurface = self.structures[active].isosurface.is_some();
                if has_isosurface {
                    if ui.button("Default").clicked() {
                        self.reset_active_isosurface_to_default();
                    }

                    let mut needs_recomposite = false;
                    {
                        let iso = self.structures[active].isosurface.as_mut().unwrap();
                        if ui.checkbox(&mut iso.show, "Show isosurface").changed() {
                            needs_recomposite = true;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Isovalue:");
                            ui.add(egui::DragValue::new(&mut iso.isovalue).speed(0.001).range(0.0..=f32::MAX));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Refinement:");
                            ui.selectable_value(&mut iso.refinement, 1, "1x");
                            ui.selectable_value(&mut iso.refinement, 2, "2x");
                            ui.selectable_value(&mut iso.refinement, 3, "3x");
                        });
                        if ui.checkbox(&mut iso.both_signs, "Both signs (+/-)").changed() {
                            needs_recomposite = true;
                        }
                        if ui.add(Slider::new(&mut iso.opacity, 0.05..=1.0).text("opacity")).changed() {
                            needs_recomposite = true;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Positive:");
                            if ui.color_edit_button_srgba(&mut iso.positive_color).changed() {
                                needs_recomposite = true;
                            }
                            ui.label("Negative:");
                            if ui.color_edit_button_srgba(&mut iso.negative_color).changed() {
                                needs_recomposite = true;
                            }
                        });
                        if ui.button("Invert colors").clicked() {
                            std::mem::swap(&mut iso.positive_color, &mut iso.negative_color);
                            needs_recomposite = true;
                        }
                    }
                    if needs_recomposite {
                        self.rebuild_isosurface();
                    }

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Isovalue/refinement/both-signs changes need Update surface (re-extraction is a real cost).")
                            .small()
                            .weak(),
                    );
                    if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("Update surface")).clicked() {
                        self.extract_active_isosurface();
                    }
                } else {
                    ui.label(egui::RichText::new("This structure has no .cube isosurface.").small().weak());
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Keep surface").clicked() {
                        self.keep_active_isosurface();
                    }
                    if ui.button("Clean").clicked() {
                        self.clean_isosurfaces();
                    }
                });
                if !self.kept_isosurfaces.is_empty() {
                    ui.label(egui::RichText::new(format!("{} isosurface(s) kept", self.kept_isosurfaces.len())).small().weak());
                }

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
