//! APOST-3D window: what was computed and how (run information), then the
//! results themselves, for the active structure.

use apost3dview_core::{element_data, fragment_coverage, hill_formula, ApostResults};
use egui::{Color32, RichText};

use crate::app::App;
use crate::results::{fragment_color, ApostAttachment, FRAGMENT_PALETTE, UNASSIGNED_COLOR};

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

/// "2026-10-13T14:05:31+02:00" -> "2026-10-13 14:05 (UTC+02:00)"; any
/// other form is shown as written.
fn readable_date(iso: &str) -> String {
    let Some((date, rest)) = iso.split_once('T') else { return iso.to_string() };
    if rest.len() < 5 {
        return iso.to_string();
    }
    let time = &rest[..5];
    let zone = rest.find(['+', '-', 'Z']).map(|i| &rest[i..]).unwrap_or("");
    match zone {
        "" => format!("{date} {time}"),
        "Z" => format!("{date} {time} (UTC)"),
        offset => format!("{date} {time} (UTC{offset})"),
    }
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
        let program = results.program.as_ref();
        let version = program.and_then(|p| p.version.clone()).map(|v| match program.and_then(|p| p.version_date.as_deref()) {
            Some(date) => format!("version {v} ({date})"),
            None => format!("version {v}"),
        });
        let threads = run.and_then(|r| r.threads).map(|t| format!("{t} thread{}", if t == 1 { "" } else { "s" }));
        ui.label(or_dash(Some([version, threads].into_iter().flatten().collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty())));
        ui.end_row();

        if let Some(date) = run.and_then(|r| r.date.as_deref()) {
            ui.label("Run started");
            ui.label(readable_date(date));
            ui.end_row();
        }

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

/// Atom numbers as the user sees them elsewhere ("Fe1, C2, N3"), from
/// 0-based indices.
fn atom_names(atomic_numbers: &[u32], atoms: impl IntoIterator<Item = usize>) -> Vec<String> {
    atoms.into_iter().map(|i| format!("{}{}", element_data(atomic_numbers[i]).symbol, i + 1)).collect()
}

/// At most `limit` names, then a count of the rest.
fn abbreviated(names: &[String], limit: usize) -> String {
    if names.len() <= limit {
        names.join(", ")
    } else {
        format!("{}, ... (+{} more)", names[..limit].join(", "), names.len() - limit)
    }
}

fn swatch(ui: &mut egui::Ui, color: [u8; 3]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(color[0], color[1], color[2]));
}

/// What the fragment list asks the window to do after drawing.
enum FragmentAction {
    /// Select the atoms of the fragment at this position in the list;
    /// `add` keeps the current selection (Cmd/Ctrl-click).
    Select { position: usize, add: bool },
    ToggleColors,
}

/// The fragments as APOST-3D defined them, which doubles as the user's check
/// that they were defined as intended: atoms in no fragment, or in two, are
/// called out.
fn show_fragments(ui: &mut egui::Ui, attachment: &ApostAttachment, results: &ApostResults, atomic_numbers: &[u32], selected: &[usize]) -> Option<FragmentAction> {
    let fragments = results.fragments();
    let mut action = None;
    egui::CollapsingHeader::new(format!("Fragments ({})", fragments.len())).id_salt("apost_fragments").default_open(true).show(ui, |ui| {
        if fragments.is_empty() {
            ui.label(RichText::new("This run defines no fragments.").weak());
            return;
        }
        let mut colors_on = attachment.fragment_colors;
        if ui
            .toggle_value(&mut colors_on, "Color atoms by fragment")
            .on_hover_text("Switch off for a figure in the usual element colors; switch on again at any time.")
            .changed()
        {
            action = Some(FragmentAction::ToggleColors);
        }
        if fragments.len() > FRAGMENT_PALETTE.len() {
            ui.label(RichText::new(format!("More than {} fragments: colors repeat. Click a fragment to see its atoms.", FRAGMENT_PALETTE.len())).small().weak());
        }

        let coverage = fragment_coverage(atomic_numbers.len(), fragments);
        if !coverage.unassigned.is_empty() {
            let names = atom_names(atomic_numbers, coverage.unassigned.iter().copied());
            ui.horizontal_wrapped(|ui| {
                swatch(ui, UNASSIGNED_COLOR);
                ui.label(RichText::new(format!("In no fragment: {}", abbreviated(&names, 12))).color(Color32::from_rgb(200, 130, 20)));
            });
        }
        if !coverage.shared.is_empty() {
            let names = atom_names(atomic_numbers, coverage.shared.iter().copied());
            ui.label(RichText::new(format!("In more than one fragment: {}", abbreviated(&names, 12))).color(Color32::from_rgb(196, 60, 40)));
        }

        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("apost_fragment_list").max_height(260.0).show(ui, |ui| {
            for (position, fragment) in fragments.iter().enumerate() {
                let atoms: Vec<usize> = fragment.atoms.iter().map(|&a| a - 1).collect();
                let names = atom_names(atomic_numbers, atoms.iter().copied());
                let formula = hill_formula(atoms.iter().map(|&i| atomic_numbers[i]));
                let is_selected = !selected.is_empty() && selected.len() == atoms.len() && atoms.iter().all(|a| selected.contains(a));
                ui.horizontal(|ui| {
                    swatch(ui, fragment_color(position));
                    let text = format!("{}  {formula}   {}", fragment.id, abbreviated(&names, 8));
                    let response = ui.selectable_label(is_selected, text).on_hover_text(format!(
                        "Fragment {}: {}\nClick to select its atoms, Cmd/Ctrl-click to add them to the selection.",
                        fragment.id,
                        names.join(", ")
                    ));
                    if response.clicked() {
                        let add = ui.input(|i| i.modifiers.command);
                        action = Some(FragmentAction::Select { position, add });
                    }
                });
            }
        });
    });
    action
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
        let mut fragment_action = None;
        egui::Window::new("APOST-3D")
            .open(&mut open)
            .default_pos([ctx.content_rect().right() - 410.0, 50.0])
            .default_width(360.0)
            .show(ctx, |ui| {
                let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else {
                    ui.label(RichText::new("Open a structure first.").weak());
                    return;
                };
                let atomic_numbers = structure.molecule.atomic_numbers.clone();
                let selected_atoms = structure.selected_atoms.clone();
                let Some(attachment) = structure.apost.as_mut() else {
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
                if results.molecule.is_some() {
                    fragment_action = show_fragments(ui, attachment, results, &atomic_numbers, &selected_atoms);
                }
                show_input_text(ui, results);
                show_report(ui, attachment);
            });
        self.show_apost = open;
        self.apply_fragment_action(fragment_action);
    }

    fn apply_fragment_action(&mut self, action: Option<FragmentAction>) {
        let Some(action) = action else { return };
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let Some(attachment) = structure.apost.as_mut() else { return };
        match action {
            FragmentAction::ToggleColors => {
                attachment.fragment_colors = !attachment.fragment_colors;
                self.rebuild_geometry();
            }
            FragmentAction::Select { position, add } => {
                let Ok(results) = &attachment.results else { return };
                let Some(fragment) = results.fragments().get(position) else { return };
                if !add {
                    structure.selected_atoms.clear();
                    structure.selected_bonds.clear();
                }
                for atom in fragment.atoms.iter().map(|&a| a - 1) {
                    if !structure.selected_atoms.contains(&atom) {
                        structure.selected_atoms.push(atom);
                    }
                }
                self.rebuild_highlights();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::readable_date;

    #[test]
    fn dates() {
        assert_eq!(readable_date("2026-10-13T14:05:31+02:00"), "2026-10-13 14:05 (UTC+02:00)");
        assert_eq!(readable_date("2026-10-13T14:05:31-05:00"), "2026-10-13 14:05 (UTC-05:00)");
        assert_eq!(readable_date("2026-10-13T14:05:31Z"), "2026-10-13 14:05 (UTC)");
        assert_eq!(readable_date("2026-10-13T14:05:31"), "2026-10-13 14:05");
        assert_eq!(readable_date("yesterday"), "yesterday");
    }
}
