//! Render window (PNG/GIF export).

use crate::app::App;
use crate::export::{FIGURE_WIDTH_DOUBLE_COLUMN_IN, FIGURE_WIDTH_SINGLE_COLUMN_IN, RenderPreset};

impl App {
    pub(crate) fn show_render_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_render;
        egui::Window::new("Render")
            .open(&mut open)
            .default_pos([320.0, 460.0])
            .default_width(280.0)
            .show(ctx, |ui| {
                if self.active_structure.is_none() {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                }

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.render_export.preset, RenderPreset::Dpi, "DPI");
                    ui.selectable_value(&mut self.render_export.preset, RenderPreset::Custom, "Custom (pixels)");
                });
                ui.add_space(4.0);

                match self.render_export.preset {
                    RenderPreset::Dpi => {
                        ui.horizontal(|ui| {
                            ui.label("DPI:");
                            ui.add(egui::DragValue::new(&mut self.render_export.dpi).range(72..=1200));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Figure width:");
                            ui.add(egui::DragValue::new(&mut self.render_export.figure_width_in).range(1.0..=20.0).speed(0.05).suffix(" in"));
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Single column (3.25 in)").clicked() {
                                self.render_export.figure_width_in = FIGURE_WIDTH_SINGLE_COLUMN_IN;
                            }
                            if ui.button("Double column (6.75 in)").clicked() {
                                self.render_export.figure_width_in = FIGURE_WIDTH_DOUBLE_COLUMN_IN;
                            }
                        });
                        ui.label(
                            egui::RichText::new("300 DPI is the common journal minimum — raise it for larger print sizes. Embeds the physical size in the PNG.")
                                .small()
                                .weak(),
                        );
                    }
                    RenderPreset::Custom => {
                        ui.horizontal(|ui| {
                            ui.label("Width:");
                            ui.add(egui::DragValue::new(&mut self.render_export.custom_width).range(64..=16384));
                            ui.label("Height:");
                            ui.add(egui::DragValue::new(&mut self.render_export.custom_height).range(64..=16384));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Supersample:");
                            ui.selectable_value(&mut self.render_export.custom_supersample, 1, "1x");
                            ui.selectable_value(&mut self.render_export.custom_supersample, 2, "2x");
                            ui.selectable_value(&mut self.render_export.custom_supersample, 4, "4x");
                        });
                    }
                }

                ui.add_space(6.0);
                ui.checkbox(&mut self.render_export.transparent_background, "Transparent background");
                if self.ao_enabled {
                    ui.label(
                        egui::RichText::new("Ambient occlusion is on (Style window) — this export uses far more samples than the live preview.")
                            .small()
                            .weak(),
                    );
                }

                ui.add_space(8.0);
                ui.separator();
                let settings = self.resolve_export_settings();
                ui.label(egui::RichText::new(format!("Output: {} x {} px", settings.width, settings.height)).small().weak());
                ui.add_space(6.0);

                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Render and save PNG...")).clicked() {
                    self.export_render_png(settings);
                }

                ui.add_space(12.0);
                ui.separator();
                ui.label(egui::RichText::new("Animation (GIF)").strong());
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(egui::DragValue::new(&mut self.render_export.gif_width).range(160..=1920).suffix(" px"));
                    ui.label("Duration:");
                    ui.add(egui::DragValue::new(&mut self.render_export.gif_seconds).range(1.0..=30.0).speed(0.1).suffix(" s"));
                });
                ui.horizontal(|ui| {
                    ui.label("Frame rate:");
                    for fps in [15, 20, 25] {
                        ui.selectable_value(&mut self.render_export.gif_fps, fps, format!("{fps} fps"));
                    }
                });
                let gif_settings = self.gif_export_settings();
                ui.label(
                    egui::RichText::new(format!(
                        "One full turn from the current view, {} frames, {} x {} px, looping seamlessly. Turns in the auto-spin direction (the last arrow key(s) pressed), around the center of the view, so leave some margin around the molecule. Uses the Style background color.",
                        self.gif_frame_count(),
                        gif_settings.width,
                        gif_settings.height
                    ))
                    .small()
                    .weak(),
                );
                ui.add_space(6.0);
                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Render and save GIF...")).clicked() {
                    self.export_render_gif();
                }
            });
        self.show_render = open;
    }
}
