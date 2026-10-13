//! Analysis window (measurements).

use apost3dview_core::{element_data, measure, MeasurementKind};
use egui::{Color32, Slider};

use crate::app::App;
use crate::state::Measurement;
use crate::util::{format_measurement, format_measurement_atoms};

impl App {
    pub(crate) fn show_analysis_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_analysis;
        egui::Window::new("Analysis")
            .open(&mut open)
            .default_pos([320.0, 460.0])
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Select 2 atoms for a distance, 3 for an angle, 4 for a dihedral (Visualization has the same selection), then Add.")
                        .small()
                        .italics(),
                );

                ui.add_space(10.0);
                ui.separator();

                let Some(active) = self.active_structure else {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                };

                // Reuses `selected_atoms`, the same selection Visualization's
                // "Create bond"/"Hide atoms" actions read. No dedicated
                // picking mode is required.
                let picks = self.structures[active].selected_atoms.clone();
                let picks_summary = picks
                    .iter()
                    .map(|&i| format!("{}{}", element_data(self.structures[active].molecule.atomic_numbers[i]).symbol, i + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(format!("Selected: {} atom(s)", picks.len()));
                if !picks_summary.is_empty() {
                    ui.label(egui::RichText::new(picks_summary).small());
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled((2..=4).contains(&picks.len()), egui::Button::new("Add")).clicked() {
                        if let Some(kind) = MeasurementKind::from_picks(&picks) {
                            self.structures[active].measurements.push(Measurement { kind, label_offset: egui::Vec2::ZERO });
                            self.rebuild_measurements();
                        }
                    }
                    // The selection is not cleared automatically, consistent
                    // with "Create bond"; use "Clear selection" explicitly.
                    if ui.button("Clear selection").clicked() {
                        self.clear_selection();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label("Measurements");
                if self.structures[active].measurements.is_empty() {
                    ui.label(egui::RichText::new("None yet.").weak());
                } else {
                    let mut remove_index = None;
                    for (index, measurement) in self.structures[active].measurements.iter().enumerate() {
                        let value = measure(&self.structures[active].molecule, measurement.kind);
                        let atoms = format_measurement_atoms(&self.structures[active].molecule, measurement.kind);
                        let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
                        ui.horizontal(|ui| {
                            ui.label(format!("{atoms}: {text}"));
                            if ui.small_button("×").clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove_index {
                        self.structures[active].measurements.remove(index);
                        self.rebuild_measurements();
                    }
                    if ui.button("Clear all").clicked() {
                        self.structures[active].measurements.clear();
                        self.rebuild_measurements();
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label("Label style");
                ui.add(Slider::new(&mut self.measurement_style.font_size, 8.0..=128.0).text("font size"));
                ui.checkbox(&mut self.measurement_style.bold, "Bold");
                let mut decimals = self.measurement_style.decimal_places as u32;
                if ui.add(Slider::new(&mut decimals, 0..=4).text("decimal places")).changed() {
                    self.measurement_style.decimal_places = decimals as usize;
                }
                ui.horizontal(|ui| {
                    ui.label("Text color:");
                    ui.color_edit_button_srgba(&mut self.measurement_style.text_color);
                });
                let mut line_color = Color32::from_rgb(
                    (self.measurement_style.line_color[0] * 255.0) as u8,
                    (self.measurement_style.line_color[1] * 255.0) as u8,
                    (self.measurement_style.line_color[2] * 255.0) as u8,
                );
                ui.horizontal(|ui| {
                    ui.label("Line color:");
                    if ui.color_edit_button_srgba(&mut line_color).changed() {
                        self.measurement_style.line_color =
                            [line_color.r() as f32 / 255.0, line_color.g() as f32 / 255.0, line_color.b() as f32 / 255.0];
                        self.rebuild_measurements();
                    }
                });
                ui.label(egui::RichText::new("Drag a label in the viewport to reposition it.").small().weak());
            });
        self.show_analysis = open;
    }
}
