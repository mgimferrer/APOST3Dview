//! Style window.

use apost3dview_render::{AoSettings, DofSettings, IsosurfaceMaterial, Material};
use egui::{Color32, Slider};

use crate::app::App;
use crate::state::AtomLabelMode;

impl App {
    pub(crate) fn show_style_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_style;
        egui::Window::new("Style")
            .open(&mut open)
            .default_pos([ctx.content_rect().right() - 280.0, 60.0])
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Default").clicked() {
                        self.material = Material::default();
                        self.ao_settings = AoSettings::default();
                        self.dof_settings = DofSettings::default();
                        self.isosurface_material = IsosurfaceMaterial::default();
                        self.rebuild_isosurface();
                    }
                    if ui.button("Publication").clicked() {
                        self.material = Material::publication();
                        self.ao_settings = AoSettings::default();
                        self.dof_settings = DofSettings::default();
                        self.isosurface_material = IsosurfaceMaterial::default();
                        // Same reasoning as `Material::publication`'s own
                        // reflectance cut — the isosurface sits under the
                        // same dead-on light and would otherwise be the
                        // one part of the scene still showing the harsher
                        // "flash" hotspot.
                        self.isosurface_material.material[2] *= 0.6;
                        self.rebuild_isosurface();
                    }
                });

                ui.add_space(8.0);
                ui.label("Custom presets");
                if self.style_presets.is_empty() {
                    ui.label(egui::RichText::new("No custom presets saved yet — tune the settings below, then save one.").small().weak());
                } else {
                    let mut apply_name = None;
                    let mut delete_name = None;
                    ui.horizontal_wrapped(|ui| {
                        for name in &self.style_presets {
                            if ui.button(name).clicked() {
                                apply_name = Some(name.clone());
                            }
                            if ui.small_button("×").clicked() {
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
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_preset_name).hint_text("preset name").desired_width(120.0));
                    let can_save = !self.new_preset_name.trim().is_empty();
                    if ui.add_enabled(can_save, egui::Button::new("Save current as preset")).clicked() {
                        let name = self.new_preset_name.trim().to_string();
                        self.save_style_preset(&name);
                        self.new_preset_name.clear();
                    }
                });

                ui.add_space(8.0);
                ui.label("Geometry");
                ui.add(Slider::new(&mut self.material.atom_scale, 0.1..=1.5).text("atom scale"));
                ui.add(Slider::new(&mut self.material.bond_radius, 0.02..=0.5).text("bond radius"));

                ui.add_space(12.0);
                ui.label("Atom labels");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::None, "None");
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::Number, "Number");
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::Type, "Type");
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::NumberType, "Number+Type");
                });
                if self.atom_label_mode != AtomLabelMode::None {
                    ui.add(Slider::new(&mut self.atom_label_style.relative_size, 0.2..=6.0).text("label size (× atom radius)"));
                    ui.horizontal(|ui| {
                        ui.label("Label color:");
                        ui.color_edit_button_srgba(&mut self.atom_label_style.text_color);
                    });
                }

                ui.add_space(12.0);
                ui.label("Material");
                ui.add(Slider::new(&mut self.material.ambient, 0.0..=1.0).text("ambient"));
                ui.add(Slider::new(&mut self.material.roughness, 0.05..=1.0).text("roughness"));
                ui.add(Slider::new(&mut self.material.reflectance, 0.0..=0.3).text("reflectance"));
                ui.add(Slider::new(&mut self.material.light_intensity, 0.5..=8.0).text("light intensity"));
                ui.add(Slider::new(&mut self.material.exposure, 0.3..=2.5).text("exposure"));

                ui.add_space(12.0);
                ui.label("Isosurface material");
                ui.label(egui::RichText::new("Independent from the atom/bond material above.").small().weak());
                let mut isosurface_material_changed = false;
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[0], 0.0..=1.0).text("ambient")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[1], 0.05..=1.0).text("roughness")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[2], 0.0..=0.3).text("reflectance")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[3], 0.5..=8.0).text("light intensity")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.fresnel[0], 0.5..=8.0).text("rim power")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.fresnel[1], 0.0..=2.0).text("rim glow")).changed();
                if isosurface_material_changed {
                    self.rebuild_isosurface();
                }

                ui.add_space(12.0);
                if ui.checkbox(&mut self.ao_enabled, "Ambient occlusion").changed() {
                    // The isosurface material buffer isn't re-uploaded every
                    // frame the way atom/bond material is (see
                    // `ao_render_isosurface_material`'s doc) — without this,
                    // toggling AO alone (no other isosurface setting touched)
                    // would leave it stale until something else happened to
                    // trigger a rebuild.
                    self.rebuild_isosurface();
                }
                ui.label(
                    egui::RichText::new("Real per-pixel contact shading on atoms and bonds — live preview uses far fewer samples than export.")
                        .small()
                        .weak(),
                );
                if self.ao_enabled {
                    ui.add(Slider::new(&mut self.ao_settings.radius, 0.1..=3.0).text("radius (\u{c5}ngstrom)"));
                    ui.add(Slider::new(&mut self.ao_settings.strength, 0.0..=1.0).text("strength"));
                    ui.add(Slider::new(&mut self.ao_settings.contrast_power, 0.5..=6.0).text("contrast"));
                    ui.add(Slider::new(&mut self.ao_settings.outline_strength, 0.0..=6.0).text("outline"));
                    if ui.button("Default").clicked() {
                        self.ao_settings = AoSettings::default();
                    }
                }

                ui.add_space(12.0);
                ui.checkbox(&mut self.dof_enabled, "Depth of field");
                ui.label(
                    egui::RichText::new("Blurs whatever's far from the focal plane (always the current orbit target) — the finishing touch on a render, without changing any colors.")
                        .small()
                        .weak(),
                );
                if self.dof_enabled {
                    ui.add(Slider::new(&mut self.dof_settings.strength, 0.0..=1.0).text("strength"));
                    ui.add(Slider::new(&mut self.dof_settings.focus_range, 0.05..=2.0).text("focus range"));
                    if ui.button("Default").clicked() {
                        self.dof_settings = DofSettings::default();
                    }
                }

                ui.add_space(12.0);
                ui.label("Lighting");
                ui.add(
                    Slider::new(&mut self.material.light_yaw, -std::f32::consts::PI..=std::f32::consts::PI)
                        .text("light yaw"),
                );
                ui.add(Slider::new(&mut self.material.light_pitch, -1.5..=1.5).text("light pitch"));

                ui.add_space(12.0);
                ui.label("Background");
                let mut background = Color32::from_rgb(
                    (self.material.background[0] * 255.0) as u8,
                    (self.material.background[1] * 255.0) as u8,
                    (self.material.background[2] * 255.0) as u8,
                );
                if ui.color_edit_button_srgba(&mut background).changed() {
                    self.material.background = [
                        background.r() as f32 / 255.0,
                        background.g() as f32 / 255.0,
                        background.b() as f32 / 255.0,
                    ];
                }

                ui.add_space(16.0);
                ui.separator();
                ui.label("Right-drag to orbit, shift+right-drag to pan, scroll/arrows to zoom/rotate. Left-click selects.");
                ui.label(egui::RichText::new("Shared across every loaded structure.").small().weak());
            });
        self.show_style = open;
    }
}
