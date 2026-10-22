//! Appearance window: the look of the whole scene, shared by every open
//! structure. The choices most people make (a preset, the background,
//! ambient occlusion, depth of field) come first; every fine-tuning slider
//! is under "Advanced".

use apost3dview_render::{AoSettings, DofSettings, IsosurfaceMaterial, Material};
use egui::{Color32, RichText, Slider};

use crate::app::App;
use crate::windows::ToolWindow;

impl App {
    pub(crate) fn show_appearance_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_appearance;
        let response = self
            .column_window(ctx, ToolWindow::Appearance, &mut open)
            .show(ctx, |ui| {
                self.show_looks(ui);

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Background:");
                    let [r, g, b] = self.material.background.map(|c| (c * 255.0).round() as u8);
                    let mut background = Color32::from_rgb(r, g, b);
                    if ui.color_edit_button_srgba(&mut background).changed() {
                        self.material.background = [background.r(), background.g(), background.b()].map(|c| c as f32 / 255.0);
                    }
                });
                // The surface material buffer is not re-uploaded every frame
                // the way the atom/bond material is, and its AO dampening
                // depends on this toggle (`ao_render_isosurface_material`).
                if ui
                    .checkbox(&mut self.ao_enabled, "Ambient occlusion")
                    .on_hover_text("Contact shadows where atoms, bonds and surfaces meet. Exports use many more samples than the live view.")
                    .changed()
                {
                    self.rebuild_isosurface();
                }
                ui.checkbox(&mut self.dof_enabled, "Depth of field")
                    .on_hover_text("Blurs what is far from the center of rotation, like a camera lens.");
                ui.horizontal(|ui| {
                    ui.label("Projection:");
                    ui.selectable_value(&mut self.camera.orthographic, true, "Orthographic")
                        .on_hover_text("No perspective, as in most figures of papers: equal bonds look equal anywhere in the picture.");
                    ui.selectable_value(&mut self.camera.orthographic, false, "Perspective").on_hover_text("Nearer parts look larger, as through a camera.");
                });

                ui.add_space(6.0);
                egui::CollapsingHeader::new("Advanced").id_salt("appearance_advanced").show(ui, |ui| self.show_advanced_appearance(ui));
            });
        self.record_column_window(ctx, ToolWindow::Appearance, response.map(|r| r.response));
        self.show_appearance = open;
    }

    /// Default, Publication, and the user's own saved looks.
    fn show_looks(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Look").strong());
        let mut apply_name = None;
        let mut delete_name = None;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Default").clicked() {
                self.material = Material::default();
                self.ao_settings = AoSettings::default();
                self.dof_settings = DofSettings::default();
                self.isosurface_material = IsosurfaceMaterial::default();
                self.rebuild_isosurface();
            }
            if ui.button("Publication").on_hover_text("Light from the viewer's direction, no shiny hotspots").clicked() {
                self.material = Material::publication();
                self.ao_settings = AoSettings::default();
                self.dof_settings = DofSettings::default();
                self.isosurface_material = IsosurfaceMaterial::default();
                // Same reasoning as `Material::publication`'s own
                // reflectance cut — the isosurface sits under the same
                // dead-on light and would otherwise be the one part of the
                // scene still showing the harsher "flash" hotspot.
                self.isosurface_material.material[2] *= 0.6;
                self.rebuild_isosurface();
            }
            for name in &self.style_presets {
                if ui.button(name).clicked() {
                    apply_name = Some(name.clone());
                }
                if ui.small_button("\u{d7}").on_hover_text(format!("Delete \"{name}\"")).clicked() {
                    delete_name = Some(name.clone());
                }
            }
        });
        if let Some(name) = apply_name {
            self.apply_style_preset(&name);
        }
        if let Some(name) = delete_name {
            self.delete_style_preset(&name);
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_preset_name).hint_text("name").desired_width(110.0));
            let can_save = !self.new_preset_name.trim().is_empty();
            if ui.add_enabled(can_save, egui::Button::new("Save this look")).on_hover_text("Keep the current settings as a look of your own").clicked() {
                let name = self.new_preset_name.trim().to_string();
                self.save_style_preset(&name);
                self.new_preset_name.clear();
            }
            if ui.button("Open presets folder").on_hover_text("Where your looks are saved, one file each: copy them to another computer or share them").clicked() {
                self.open_presets_folder();
            }
        });
    }

    fn show_advanced_appearance(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Light").strong());
        ui.add(Slider::new(&mut self.material.light_yaw, -std::f32::consts::PI..=std::f32::consts::PI).text("direction (left/right)"));
        ui.add(Slider::new(&mut self.material.light_pitch, -1.5..=1.5).text("direction (up/down)"));
        ui.add(Slider::new(&mut self.material.exposure, 0.3..=2.5).text("exposure"));

        ui.add_space(8.0);
        ui.label(RichText::new("Atoms and bonds").strong());
        ui.add(Slider::new(&mut self.material.ambient, 0.0..=1.0).text("ambient"));
        ui.add(Slider::new(&mut self.material.roughness, 0.05..=1.0).text("roughness"));
        ui.add(Slider::new(&mut self.material.reflectance, 0.0..=0.3).text("reflectance"));
        ui.add(Slider::new(&mut self.material.light_intensity, 0.5..=8.0).text("light intensity"));

        ui.add_space(8.0);
        ui.label(RichText::new("Surfaces").strong());
        let m = &mut self.isosurface_material;
        let mut changed = false;
        changed |= ui.add(Slider::new(&mut m.material[0], 0.0..=1.0).text("ambient")).changed();
        changed |= ui.add(Slider::new(&mut m.material[1], 0.05..=1.0).text("roughness")).changed();
        changed |= ui.add(Slider::new(&mut m.material[2], 0.0..=0.3).text("reflectance")).changed();
        changed |= ui.add(Slider::new(&mut m.material[3], 0.5..=8.0).text("light intensity")).changed();
        changed |= ui.add(Slider::new(&mut m.fresnel[0], 0.5..=8.0).text("rim power")).changed();
        changed |= ui.add(Slider::new(&mut m.fresnel[1], 0.0..=2.0).text("rim glow")).changed();
        if changed {
            self.rebuild_isosurface();
        }

        if self.ao_enabled {
            ui.add_space(8.0);
            ui.label(RichText::new("Ambient occlusion").strong());
            ui.add(Slider::new(&mut self.ao_settings.radius, 0.1..=3.0).text("radius (\u{c5})"));
            ui.add(Slider::new(&mut self.ao_settings.strength, 0.0..=1.0).text("strength"));
            ui.add(Slider::new(&mut self.ao_settings.contrast_power, 0.5..=6.0).text("contrast"));
            ui.add(Slider::new(&mut self.ao_settings.outline_strength, 0.0..=6.0).text("outline"));
            if ui.small_button("Default").clicked() {
                self.ao_settings = AoSettings::default();
            }
        }
        if self.dof_enabled {
            ui.add_space(8.0);
            ui.label(RichText::new("Depth of field").strong());
            ui.add(Slider::new(&mut self.dof_settings.strength, 0.0..=1.0).text("strength"));
            ui.add(Slider::new(&mut self.dof_settings.focus_range, 0.05..=2.0).text("focus range"));
            if ui.small_button("Default").clicked() {
                self.dof_settings = DofSettings::default();
            }
        }
    }
}
