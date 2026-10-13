//! Measure window: distances, angles and dihedrals, drawn as labeled
//! dashed lines in the view.

use apost3dview_core::{element_data, measure, LengthUnit, MeasurementKind};
use egui::{Color32, RichText, Slider};

use crate::app::App;
use crate::state::Measurement;
use crate::util::{format_measurement, format_measurement_atoms};

impl App {
    /// Adds the measurement the current selection defines: 2 atoms a
    /// distance, 3 an angle, 4 a dihedral, in click order. The selection
    /// is kept, as after any other action on it.
    pub(crate) fn add_measurement_from_selection(&mut self) {
        let Some(active) = self.active_structure else { return };
        let picks = self.structures[active].selected_atoms.clone();
        if let Some(kind) = MeasurementKind::from_picks(&picks) {
            self.structures[active].measurements.push(Measurement { kind, label_offset: egui::Vec2::ZERO });
            self.rebuild_measurements();
        }
    }

    pub(crate) fn show_measure_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_measure;
        egui::Window::new("Measure")
            .open(&mut open)
            .default_pos([350.0, 50.0])
            .default_width(280.0)
            .show(ctx, |ui| {
                let Some(active) = self.active_structure else {
                    ui.label(RichText::new("Open a structure first.").weak());
                    return;
                };

                let picks = self.structures[active].selected_atoms.clone();
                let picks_summary = picks
                    .iter()
                    .map(|&i| format!("{}{}", element_data(self.structures[active].molecule.atomic_numbers[i]).symbol, i + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                let what = match picks.len() {
                    2 => "Add distance",
                    3 => "Add angle",
                    4 => "Add dihedral",
                    _ => "Add",
                };
                ui.horizontal(|ui| {
                    if ui.add_enabled((2..=4).contains(&picks.len()), egui::Button::new(what)).clicked() {
                        self.add_measurement_from_selection();
                    }
                    if picks.is_empty() {
                        ui.label(RichText::new("Click 2 atoms for a distance, 3 for an angle, 4 for a dihedral.").small().weak());
                    } else {
                        ui.label(RichText::new(picks_summary).small());
                    }
                });

                ui.add_space(6.0);
                ui.separator();
                if self.structures[active].measurements.is_empty() {
                    ui.label(RichText::new("No measurements yet.").weak());
                } else {
                    let mut remove_index = None;
                    for (index, measurement) in self.structures[active].measurements.iter().enumerate() {
                        let value = measure(&self.structures[active].molecule, measurement.kind);
                        let atoms = format_measurement_atoms(&self.structures[active].molecule, measurement.kind);
                        let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
                        ui.horizontal(|ui| {
                            ui.label(format!("{atoms}: {text}"));
                            if ui.small_button("\u{d7}").clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove_index {
                        self.structures[active].measurements.remove(index);
                        self.rebuild_measurements();
                    }
                    if ui.button("Remove all").clicked() {
                        self.structures[active].measurements.clear();
                        self.rebuild_measurements();
                    }
                    ui.label(RichText::new("Drag a label in the view to move it.").small().weak());
                }

                ui.add_space(4.0);
                egui::CollapsingHeader::new("Label style").id_salt("measurement_label_style").show(ui, |ui| {
                    ui.add(Slider::new(&mut self.measurement_style.font_size, 8.0..=128.0).text("font size"));
                    ui.checkbox(&mut self.measurement_style.bold, "Bold");
                    ui.horizontal(|ui| {
                        ui.label("Distances in:");
                        ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Angstrom, "\u{c5}ngstrom");
                        ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Bohr, "Bohr");
                    });
                    let mut decimals = self.measurement_style.decimal_places as u32;
                    if ui.add(Slider::new(&mut decimals, 0..=4).text("decimal places")).changed() {
                        self.measurement_style.decimal_places = decimals as usize;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Text color:");
                        ui.color_edit_button_srgba(&mut self.measurement_style.text_color);
                        let mut line_color = Color32::from_rgb(
                            (self.measurement_style.line_color[0] * 255.0) as u8,
                            (self.measurement_style.line_color[1] * 255.0) as u8,
                            (self.measurement_style.line_color[2] * 255.0) as u8,
                        );
                        ui.label("Line color:");
                        if ui.color_edit_button_srgba(&mut line_color).changed() {
                            self.measurement_style.line_color =
                                [line_color.r() as f32 / 255.0, line_color.g() as f32 / 255.0, line_color.b() as f32 / 255.0];
                            self.rebuild_measurements();
                        }
                    });
                });
            });
        self.show_measure = open;
    }
}
