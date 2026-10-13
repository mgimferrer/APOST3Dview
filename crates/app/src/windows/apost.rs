//! APOST-3D window: what was computed and how (run information), then the
//! results themselves, for the active structure.

use apost3dview_core::ApostResults;
use egui::{Color32, RichText};

use crate::app::App;
use crate::results::ApostAttachment;

const ERROR_FILL: Color32 = Color32::from_rgb(196, 60, 40);
const NOTICE_FILL: Color32 = Color32::from_rgb(200, 130, 20);

/// A full-width colored box for the things that must not be missed: a
/// stopped run, a refused results file, a missing `.fchk`.
fn banner(ui: &mut egui::Ui, fill: Color32, title: &str, text: &str) {
    egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).color(Color32::WHITE).strong());
        if !text.is_empty() {
            ui.label(RichText::new(text).color(Color32::WHITE));
        }
    });
}

fn or_dash(value: Option<String>) -> String {
    value.unwrap_or_else(|| "-".to_string())
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn show_run_summary(ui: &mut egui::Ui, attachment: &ApostAttachment, results: &ApostResults) {
    let run = results.run.as_ref();
    let wavefunction = results.wavefunction.as_ref();
    let definition = results.input.as_ref().and_then(|i| i.atomic_definition.as_ref());

    egui::Grid::new("apost_run_summary").num_columns(2).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
        ui.label("Results file");
        ui.label(attachment.file_name()).on_hover_text(attachment.results_path.display().to_string());
        ui.end_row();

        ui.label("Job");
        ui.label(or_dash(run.and_then(|r| r.job_name.clone())));
        ui.end_row();

        ui.label("APOST-3D");
        let version = results.program.as_ref().and_then(|p| p.version.clone()).map(|v| format!("version {v}"));
        let threads = run.and_then(|r| r.threads).map(|t| format!("{t} thread{}", if t == 1 { "" } else { "s" }));
        ui.label(or_dash(Some([version, threads].into_iter().flatten().collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty())));
        ui.end_row();

        ui.label("Wavefunction");
        let method = wavefunction.and_then(|w| w.method.clone());
        let formalism = wavefunction.and_then(|w| w.formalism.as_deref().map(capitalized));
        ui.label(or_dash(Some([method, formalism].into_iter().flatten().collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty())));
        ui.end_row();

        ui.label("Basis functions");
        ui.label(or_dash(wavefunction.and_then(|w| w.n_basis_functions).map(|n| n.to_string())));
        ui.end_row();

        ui.label("Electrons");
        let electrons = wavefunction.and_then(|w| {
            let total = w.n_electrons?;
            Some(match (w.n_alpha, w.n_beta) {
                (Some(a), Some(b)) => format!("{total} ({a} \u{3b1}, {b} \u{3b2})"),
                _ => total.to_string(),
            })
        });
        ui.label(or_dash(electrons));
        ui.end_row();

        ui.label("Energy");
        match wavefunction.and_then(|w| w.energy_au) {
            Some(energy) => ui.label(format!("{energy:.8} Hartree")),
            None => ui.label(RichText::new("not in the .fchk").weak()),
        };
        ui.end_row();

        ui.label("Atomic definition");
        let definition_text = definition.map(|d| {
            let mut text = match (&d.name, &d.keyword) {
                (Some(name), Some(keyword)) => format!("{name} ({keyword})"),
                (Some(name), None) => name.clone(),
                (None, Some(keyword)) => keyword.clone(),
                (None, None) => "-".to_string(),
            };
            match d.space.as_deref() {
                Some("real") => text.push_str(", real space"),
                Some("hilbert") => text.push_str(", Hilbert space"),
                _ => {}
            }
            if d.default == Some(true) {
                text.push_str(" (default)");
            }
            text
        });
        ui.label(or_dash(definition_text));
        ui.end_row();

        ui.label("Analyses");
        let analyses = results.analysis_names();
        if analyses.is_empty() {
            ui.label(RichText::new("none in this file").weak());
        } else {
            ui.label(analyses.join(", "));
        }
        ui.end_row();
    });
}

fn show_input_text(ui: &mut egui::Ui, results: &ApostResults) {
    let Some(text) = results.input.as_ref().and_then(|i| i.text.as_deref()) else { return };
    let input_name = results.run.as_ref().and_then(|r| r.input_file.clone()).unwrap_or_else(|| "Input".to_string());
    egui::CollapsingHeader::new(format!("Input ({input_name})")).id_salt("apost_input").show(ui, |ui| {
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(text.to_string());
        }
        egui::ScrollArea::both().id_salt("apost_input_scroll").max_height(220.0).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).monospace()).selectable(true).wrap_mode(egui::TextWrapMode::Extend));
        });
    });
}

/// The `.apost` text report, read on first expand and drawn one visible row
/// at a time, since a large system's report runs to many thousands of lines.
fn show_report(ui: &mut egui::Ui, attachment: &mut ApostAttachment) {
    let Some(path) = &attachment.report_path else { return };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let response = egui::CollapsingHeader::new(format!("Text report ({name})")).id_salt("apost_report").show(ui, |ui| {
        match &attachment.report_lines {
            Some(Ok(lines)) => {
                if ui.button("Copy all").clicked() {
                    ui.ctx().copy_text(lines.join("\n"));
                }
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                egui::ScrollArea::both().id_salt("apost_report_scroll").max_height(320.0).show_rows(ui, row_height, lines.len(), |ui, rows| {
                    for line in &lines[rows] {
                        ui.add(egui::Label::new(RichText::new(line).monospace()).wrap_mode(egui::TextWrapMode::Extend));
                    }
                });
            }
            Some(Err(err)) => {
                ui.label(RichText::new(format!("Could not read it: {err}")).color(ERROR_FILL));
            }
            None => {
                ui.spinner();
            }
        }
    });
    if response.body_returned.is_some() {
        attachment.load_report();
    }
}

impl App {
    pub(crate) fn show_apost_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_apost;
        egui::Window::new("APOST-3D")
            .open(&mut open)
            .default_pos([ctx.content_rect().right() - 400.0, 60.0])
            .default_width(360.0)
            .show(ctx, |ui| {
                let attachment = self.active_structure.and_then(|i| self.structures.get_mut(i)).and_then(|s| s.apost.as_mut());
                let Some(attachment) = attachment else {
                    ui.label(RichText::new("No APOST-3D results for this structure.").weak());
                    ui.label(
                        RichText::new(
                            "Open an APOST-3D results file (.apost.json), or a .fchk with the <job>.apost.json \
                             APOST-3D wrote next to it.",
                        )
                        .small()
                        .weak(),
                    );
                    return;
                };

                let results = match &attachment.results {
                    Ok(results) => results,
                    Err(reason) => {
                        banner(ui, ERROR_FILL, "Results not loaded", reason);
                        return;
                    }
                };

                if results.stopped() {
                    let message = results.run.as_ref().and_then(|r| r.stop_message.clone()).unwrap_or_default();
                    banner(ui, ERROR_FILL, "APOST-3D stopped before the end of this run", &message);
                    ui.add_space(6.0);
                }
                if !attachment.has_fchk {
                    banner(
                        ui,
                        NOTICE_FILL,
                        "No .fchk found next to the results file",
                        "The geometry shown comes from the results file; orbitals are not available.",
                    );
                    ui.add_space(6.0);
                }

                show_run_summary(ui, attachment, results);
                ui.add_space(8.0);
                show_input_text(ui, results);
                show_report(ui, attachment);
            });
        self.show_apost = open;
    }
}
