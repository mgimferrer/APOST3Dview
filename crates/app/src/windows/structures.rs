//! Structures window.

use crate::app::App;

impl App {
    pub(crate) fn show_structures_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_structures;
        egui::Window::new("Structures")
            .open(&mut open)
            .default_pos([40.0, 60.0])
            .default_width(260.0)
            .show(ctx, |ui| {
                if self.structures.is_empty() {
                    ui.label(egui::RichText::new("No structures loaded yet. Use Open in the toolbar, or drag files onto the window.").weak());
                } else {
                    ui.separator();
                    let mut switch_to = None;
                    for (index, structure) in self.structures.iter().enumerate() {
                        let selected = self.active_structure == Some(index);
                        if ui.selectable_label(selected, &structure.label).clicked() {
                            switch_to = Some(index);
                        }
                    }
                    if let Some(index) = switch_to {
                        self.set_active(index);
                    }
                }
            });
        self.show_structures = open;
    }
}
