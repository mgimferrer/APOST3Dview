//! Export window: the current view as a PNG image (alone, or a figure of
//! panels with captions) or a turning GIF.

use egui::RichText;

use crate::app::App;
use crate::export::{RenderPreset, FIGURE_WIDTH_DOUBLE_COLUMN_IN, FIGURE_WIDTH_SINGLE_COLUMN_IN};
use crate::panels::{grid_for, CaptionStyle, FigureBackground, FigureSizing, ImageMode, Panel, PanelShape, FULL_PAGE_IN, HALF_PAGE_IN};
use crate::windows::ToolWindow;

/// What the panel list asks for, applied after it is drawn.
enum PanelAction {
    Up(usize),
    Down(usize),
    Remove(usize),
    Add(Panel),
    AddAll,
}

impl App {
    pub(crate) fn show_export_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_export;
        let response = self
            .column_window(ctx, ToolWindow::Export, &mut open)
            .show(ctx, |ui| {
                if self.active_structure.is_none() {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                }

                ui.checkbox(&mut self.render_export.save_session_with_image, "Also save the session")
                    .on_hover_text("Writes name.a3dview next to the image: open it later to get exactly this picture again");
                ui.add_space(4.0);
                let mode_before = self.render_export.figure.settings.mode;
                ui.horizontal(|ui| {
                    ui.label("Image:");
                    let mode = &mut self.render_export.figure.settings.mode;
                    ui.selectable_value(mode, ImageMode::Single, "Single image");
                    ui.selectable_value(mode, ImageMode::Panels, "Panels")
                        .on_hover_text("Several surfaces side by side in one picture, same view, each with a caption");
                });
                if mode_before != ImageMode::Panels && self.render_export.figure.settings.mode == ImageMode::Panels {
                    self.fill_panels_if_empty();
                }
                ui.add_space(4.0);

                match self.render_export.figure.settings.mode {
                    ImageMode::Single => self.show_single_image_export(ui),
                    ImageMode::Panels => self.show_panels_export(ui),
                }

                ui.add_space(12.0);
                ui.separator();
                self.show_gif_export(ui);
            });
        self.record_column_window(ctx, ToolWindow::Export, response.map(|r| r.response));
        self.show_export = open;
    }

    fn show_single_image_export(&mut self, ui: &mut egui::Ui) {
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
                    egui::RichText::new("300 DPI is the common journal minimum: raise it for larger print sizes. Embeds the physical size in the PNG.")
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
                egui::RichText::new("Ambient occlusion is on (Appearance window): this export uses far more samples than the live view.")
                    .small()
                    .weak(),
            );
        }

        ui.add_space(6.0);
        let default_caption = self.default_single_caption();
        let settings = &mut self.render_export.figure.settings;
        ui.checkbox(&mut settings.single_caption, "Caption").on_hover_text("A caption below the image");
        if settings.single_caption {
            ui.add(egui::TextEdit::singleline(&mut settings.single_caption_text).hint_text(default_caption).desired_width(f32::INFINITY))
                .on_hover_text("Leave empty for the automatic caption");
            ui.horizontal(|ui| {
                ui.label("Text size:");
                ui.add(egui::DragValue::new(&mut settings.font_size_pt).range(4.0..=36.0).speed(0.1).suffix(" pt"));
            });
        }

        ui.add_space(8.0);
        ui.separator();
        if self.render_export.figure.settings.single_caption {
            self.show_figure_preview(ui);
            if let Some((w, h)) = self.figure_output_size() {
                ui.label(RichText::new(format!("Output: {w} x {h} px")).small().weak());
            }
            ui.add_space(6.0);
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Render and save PNG...")).clicked() {
                self.export_figure_png();
            }
        } else {
            let settings = self.resolve_export_settings();
            ui.label(egui::RichText::new(format!("Output: {} x {} px", settings.width, settings.height)).small().weak());
            ui.add_space(6.0);
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Render and save PNG...")).clicked() {
                self.export_render_png(settings);
            }
        }
    }

    fn show_panels_export(&mut self, ui: &mut egui::Ui) {
        // The figure first, so it stays in sight while it is set up below.
        self.show_figure_preview(ui);
        if let Some((w, h)) = self.figure_output_size() {
            let settings = &self.render_export.figure.settings;
            let dpi = if settings.sizing == FigureSizing::Dpi { format!(", {} DPI", settings.dpi) } else { String::new() };
            ui.label(RichText::new(format!("Output: {w} x {h} px{dpi}")).small().weak());
        }
        ui.add_space(4.0);
        let ready = !self.render_export.figure.panels.is_empty();
        if ui.add_enabled(ready, egui::Button::new("Render and save panels...").min_size(egui::vec2(ui.available_width(), 30.0))).clicked() {
            self.export_figure_png();
        }
        ui.add_space(6.0);
        ui.separator();
        self.show_panel_list(ui);

        ui.add_space(6.0);
        let n = self.render_export.figure.panels.len();
        let several_structures = self.render_export.figure.panels.iter().any(|p| Some(p.structure) != self.active_structure);
        let settings = &mut self.render_export.figure.settings;
        ui.horizontal(|ui| {
            ui.checkbox(&mut settings.captions, "Captions");
            ui.add_enabled_ui(settings.captions, |ui| {
                ui.selectable_value(&mut settings.caption_style, CaptionStyle::Full, "Full")
                    .on_hover_text("The orbital's name, fragments and number: OSLO 9 \u{b7} 1 CH3 + 2 CH3 \u{b7} FOLI 2.001");
                ui.selectable_value(&mut settings.caption_style, CaptionStyle::ValueAndFragment, "Value and fragment")
                    .on_hover_text("The number and the fragments: 2.001 (CH3 + CH3)");
            });
        });
        ui.horizontal(|ui| {
            ui.label("Text size:");
            ui.add(egui::DragValue::new(&mut settings.font_size_pt).range(4.0..=36.0).speed(0.1).suffix(" pt"));
            ui.checkbox(&mut settings.letters, "Letters (a), (b), ...");
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Layout:");
            ui.selectable_value(&mut settings.fixed_layout, false, "Automatic");
            ui.selectable_value(&mut settings.fixed_layout, true, "Rows and columns");
        });
        if settings.fixed_layout {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut settings.rows).range(1..=12));
                ui.label("rows x");
                ui.add(egui::DragValue::new(&mut settings.columns).range(1..=12));
                ui.label("columns");
                if ui.button("Swap").on_hover_text("Rows become columns: a figure that runs across instead of down, or the reverse").clicked() {
                    std::mem::swap(&mut settings.rows, &mut settings.columns);
                }
            });
        } else {
            let (rows, columns) = grid_for(settings, n);
            ui.label(RichText::new(format!("{rows} x {columns}")).small().weak());
        }
        ui.horizontal(|ui| {
            ui.label("Panel shape:");
            ui.selectable_value(&mut settings.panel_shape, PanelShape::Square, "Square");
            ui.selectable_value(&mut settings.panel_shape, PanelShape::View, "As the view");
            ui.label("Gap:");
            ui.add(egui::DragValue::new(&mut settings.gap_percent).range(0.0..=20.0).speed(0.1).suffix(" %"));
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Size:");
            ui.selectable_value(&mut settings.sizing, FigureSizing::Dpi, "DPI");
            ui.selectable_value(&mut settings.sizing, FigureSizing::Pixels, "Pixels");
        });
        match settings.sizing {
            FigureSizing::Dpi => {
                ui.horizontal(|ui| {
                    ui.label("DPI:");
                    ui.add(egui::DragValue::new(&mut settings.dpi).range(72..=1200));
                    ui.label("Figure width:");
                    ui.add(egui::DragValue::new(&mut settings.figure_width_in).range(1.0..=20.0).speed(0.05).suffix(" in"));
                });
                ui.horizontal(|ui| {
                    if ui.button("Half page (3.25 in)").on_hover_text("One column of a two-column journal page").clicked() {
                        settings.figure_width_in = HALF_PAGE_IN;
                    }
                    if ui.button("Full page (6.75 in)").on_hover_text("The whole width of a two-column journal page").clicked() {
                        settings.figure_width_in = FULL_PAGE_IN;
                    }
                });
            }
            FigureSizing::Pixels => {
                ui.horizontal(|ui| {
                    ui.label("Panel width:");
                    ui.add(egui::DragValue::new(&mut settings.panel_width_px).range(64..=4096).suffix(" px"));
                });
            }
        }
        ui.horizontal(|ui| {
            ui.label("Background:");
            ui.selectable_value(&mut settings.background, FigureBackground::White, "White");
            ui.selectable_value(&mut settings.background, FigureBackground::View, "As the view");
            ui.selectable_value(&mut settings.background, FigureBackground::Transparent, "Transparent");
        });
        if several_structures {
            let text = if self.keep_view {
                "Every structure is seen with the same orientation and zoom, centered on it."
            } else {
                "Each structure is seen with its own view: rotate it while it is the active one. Keep the view when switching structures (Molecule window) shows them all the same way."
            };
            ui.label(RichText::new(text).small().weak());
        }
    }

    /// The panels in order, each with its caption field, and the menu to
    /// add more.
    fn show_panel_list(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active_structure else { return };
        let panels = self.render_export.figure.panels.clone();
        let defaults: Vec<String> = panels.iter().map(|p| self.default_caption(p.structure, p.surface.as_deref())).collect();
        let several = self.structures.len() > 1;
        let name = |app: &App, panel: &Panel| {
            let structure = app.structures.get(panel.structure).map(|s| s.label.as_str()).unwrap_or("?");
            match (&panel.surface, several) {
                (Some(surface), false) => surface.clone(),
                (Some(surface), true) => format!("{structure}: {surface}"),
                (None, _) => format!("{structure} (as shown)"),
            }
        };
        let names: Vec<String> = panels.iter().map(|p| name(self, p)).collect();

        ui.label(RichText::new(format!("Panels ({})", panels.len())).strong());
        let mut action = None;
        let count = panels.len();
        egui::ScrollArea::vertical().id_salt("panel_list").max_height(260.0).show(ui, |ui| {
            let figure = &mut self.render_export.figure;
            for (k, panel) in figure.panels.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.add_enabled(k > 0, egui::Button::new("\u{2b06}").small()).on_hover_text("Move up").clicked() {
                        action = Some(PanelAction::Up(k));
                    }
                    if ui.add_enabled(k + 1 < count, egui::Button::new("\u{2b07}").small()).on_hover_text("Move down").clicked() {
                        action = Some(PanelAction::Down(k));
                    }
                    if ui.small_button("\u{d7}").on_hover_text("Remove this panel").clicked() {
                        action = Some(PanelAction::Remove(k));
                    }
                    ui.label(&names[k]);
                });
                ui.add(egui::TextEdit::singleline(&mut panel.caption).hint_text(defaults[k].as_str()).desired_width(f32::INFINITY))
                    .on_hover_text("Caption: leave empty for the automatic one");
            }
        });

        // What can still be added: surfaces not yet in a panel, and each
        // structure as it is shown; the active structure first.
        let mut order: Vec<usize> = vec![active];
        order.extend((0..self.structures.len()).filter(|&s| s != active));
        let mut candidates: Vec<Panel> = Vec::new();
        for s in order {
            let has = |surface: &Option<String>| panels.iter().any(|p| p.structure == s && &p.surface == surface);
            for label in self.structures[s].surfaces.iter().map(|x| Some(x.label.clone())).chain(std::iter::once(None)) {
                if !has(&label) {
                    candidates.push(Panel { structure: s, surface: label, caption: String::new() });
                }
            }
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("add_panel").selected_text("Add a panel").width(200.0).show_ui(ui, |ui| {
                for candidate in &candidates {
                    if ui.selectable_label(false, name(self, candidate)).clicked() {
                        action = Some(PanelAction::Add(candidate.clone()));
                    }
                }
            });
            let missing = self.structures[active].surfaces.iter().any(|x| !panels.iter().any(|p| p.structure == active && p.surface.as_deref() == Some(x.label.as_str())));
            if ui.add_enabled(missing, egui::Button::new("All surfaces")).on_hover_text("Adds every surface of the active structure not in a panel yet").clicked() {
                action = Some(PanelAction::AddAll);
            }
        });

        let figure = &mut self.render_export.figure;
        match action {
            Some(PanelAction::Up(k)) => figure.panels.swap(k, k - 1),
            Some(PanelAction::Down(k)) => figure.panels.swap(k, k + 1),
            Some(PanelAction::Remove(k)) => {
                figure.panels.remove(k);
            }
            Some(PanelAction::Add(panel)) => figure.panels.push(panel),
            Some(PanelAction::AddAll) => {
                for label in self.structures[active].surfaces.iter().map(|x| x.label.clone()) {
                    if !figure.panels.iter().any(|p| p.structure == active && p.surface.as_deref() == Some(label.as_str())) {
                        figure.panels.push(Panel { structure: active, surface: Some(label), caption: String::new() });
                    }
                }
            }
            None => {}
        }
    }

    /// The preview of the figure, as wide as the window; a click shows it
    /// larger.
    fn show_figure_preview(&mut self, ui: &mut egui::Ui) {
        let preview = &mut self.render_export.figure.preview;
        if let Some(problem) = &preview.problem {
            ui.label(RichText::new(problem).small().color(ui.visuals().warn_fg_color));
            return;
        }
        let Some(texture) = &preview.texture else {
            ui.label(RichText::new("Preparing the preview...").small().weak());
            return;
        };
        // At most this tall, so the settings below stay in sight; a click
        // shows it larger.
        const MAX_HEIGHT: f32 = 240.0;
        let size = texture.size_vec2();
        let scale = (ui.available_width() / size.x).min(MAX_HEIGHT / size.y).min(1.0);
        let image = egui::Image::new((texture.id(), size * scale)).sense(egui::Sense::click());
        if ui.vertical_centered(|ui| ui.add(image)).inner.on_hover_text("Preview of the figure. Click to see it larger.").clicked() {
            preview.large = true;
        }
    }

    /// The preview, larger, in a window of its own.
    pub(crate) fn show_figure_preview_window(&mut self, ctx: &egui::Context) {
        let preview = &mut self.render_export.figure.preview;
        if !preview.large || !self.show_export {
            return;
        }
        let Some(texture) = preview.texture.clone() else { return };
        let mut open = true;
        egui::Window::new("Figure preview").open(&mut open).default_size([820.0, 640.0]).resizable(true).show(ctx, |ui| {
            let size = texture.size_vec2();
            let available = ui.available_size();
            let scale = (available.x / size.x).min(available.y / size.y).min(1.0);
            ui.add(egui::Image::new((texture.id(), size * scale)));
        });
        self.render_export.figure.preview.large = open;
    }

    fn show_gif_export(&mut self, ui: &mut egui::Ui) {
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
                "One full turn from the current view, {} frames, {} x {} px, looping seamlessly. Turns in the auto-spin direction (the last arrow key(s) pressed), around the center of the view, so leave some margin around the molecule. Uses the Appearance background color.",
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
    }
}
