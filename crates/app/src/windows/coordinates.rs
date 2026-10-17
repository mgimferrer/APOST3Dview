//! Coordinates window: the active structure's Cartesian coordinates, to
//! copy or save as .xyz.

use apost3dview_core::{format_coordinates, CoordinateFormat, LengthUnit};

use crate::app::App;

impl App {
    /// Writes the active structure's coordinates to a standard `.xyz` file
    /// next to its source `.fchk`, same base name, `.xyz` extension —
    /// always in Angstrom regardless of the viewer's current unit toggle,
    /// since that's what makes it a valid, portable `.xyz` file.
    pub(crate) fn export_active_xyz(&mut self, target: std::path::PathBuf) {
        let Some(active) = self.active_structure else { return };
        let structure = &self.structures[active];
        let text = format_coordinates(&structure.molecule, LengthUnit::Angstrom, CoordinateFormat::XyzFile, &structure.label);
        match std::fs::write(&target, text) {
            Ok(()) => self.show_status(format!("Saved {}", target.display())),
            Err(err) => self.show_warning(format!("Could not save {}: {err}", target.display())),
        }
    }

    pub(crate) fn show_coordinates_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_coordinates;
        egui::Window::new("Coordinates")
            .open(&mut open)
            .default_pos([650.0, 300.0])
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Unit:");
                    ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Angstrom, "Ang (Å)");
                    ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Bohr, "Bohr (a.u.)");
                });
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::AtomicNumberTable, "Standard");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::XyzFile, "Symbol XYZ");
                });
                let export_target = self
                    .active_structure
                    .and_then(|i| self.structures.get(i))
                    .and_then(|s| s.source_path.as_ref())
                    .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("fchk")))
                    .map(|p| p.with_extension("xyz"));

                if let Some(target) = &export_target {
                    ui.add_space(4.0);
                    let file_name = crate::util::file_display_name(&target);
                    if ui
                        .add_sized([ui.available_width(), 28.0], egui::Button::new(format!("Save coordinates as {file_name}")))
                        .on_hover_text(target.display().to_string())
                        .clicked()
                    {
                        self.export_active_xyz(target.clone());
                    }
                }

                ui.separator();

                match self.active_structure.and_then(|i| self.structures.get(i)) {
                    Some(structure) => {
                        let text = format_coordinates(&structure.molecule, self.coordinate_unit, self.coordinate_format, &structure.label);
                        if ui.button("Copy").clicked() {
                            ui.ctx().copy_text(text.clone());
                        }
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(text).monospace())
                                    .selectable(true)
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                    }
                    None => {
                        ui.label("No structure loaded.");
                    }
                }
            });
        self.show_coordinates = open;
    }
}
