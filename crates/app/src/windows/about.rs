//! About window.

use crate::app::App;

impl App {
    pub(crate) fn show_about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("About APOST3Dview")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let logo_size = self.logo_texture.size_vec2();
                    let display_size = logo_size * (240.0 / logo_size.x);
                    ui.image((self.logo_texture.id(), display_size));

                    ui.add_space(8.0);
                    ui.label(format!("APOST3Dview v{}", env!("CARGO_PKG_VERSION")));
                    ui.label("A molecular visualizer for APOST-3D.");
                    ui.add_space(8.0);
                    ui.label("Martí Gimferrer");
                    ui.hyperlink_to("mgimferrer18@gmail.com", "mailto:mgimferrer18@gmail.com");
                    ui.add_space(8.0);
                    ui.label(
                        "Sister project to APOST-3D, a software to extract state-of-the-art \
                         chemical bonding indicators from wavefunction analysis",
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "APOST3Dview does not yet have its own dedicated publication. \
                             If you use it, please cite the APOST-3D paper:",
                        )
                        .small(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "P. Salvador, E. Ramos-Cordoba, M. Montilla, L. Pujal and \
                             M. Gimferrer, J. Chem. Phys., 2024, 160, 172502",
                        )
                        .small()
                        .italics(),
                    );
                    ui.hyperlink_to("DOI: 10.1063/5.0206187", "https://doi.org/10.1063/5.0206187");
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Figure captions use the Arimo font, by the Arimo Project Authors, under the SIL Open Font License 1.1, whose text comes with the program as Arimo-OFL.txt.")
                            .small()
                            .weak(),
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.hyperlink_to("Report a problem or suggest a feature", "https://github.com/mgimferrer/APOST3Dview/issues/new/choose");
                    let details = self.system_details();
                    ui.label(egui::RichText::new(&details).small().weak());
                    if ui.small_button("Copy these details").on_hover_text("For a bug report: version, system and graphics").clicked() {
                        ui.ctx().copy_text(details);
                    }
                });
            });
        self.show_about = open;
    }

    /// "APOST3Dview 0.9.0 · macos aarch64 · Apple M5 (Metal)": what a bug
    /// report needs to say about where the app runs.
    fn system_details(&self) -> String {
        let info = self.render_state.adapter.get_info();
        format!("APOST3Dview {} · {} {} · {} ({:?})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, info.name, info.backend)
    }
}
