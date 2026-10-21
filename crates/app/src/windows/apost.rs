//! APOST-3D window: what was computed and how (run information), then the
//! results themselves, for the active structure.

use apost3dview_core::{fragment_coverage, hill_formula, ApostResults};
use egui::{Color32, RichText};

use crate::app::App;
use crate::windows::ToolWindow;
use crate::names::atom_name;
use crate::util::{capitalize, swatch};
use crate::results::{fragment_color, ApostAttachment, FRAGMENT_PALETTE, UNASSIGNED_COLOR};
use crate::results_view::focus_target;

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
        if let Some(id) = run.and_then(|r| r.id.as_deref()) {
            ui.label("Run id");
            ui.label(id).on_hover_text("Also on the first line of the orbital files this run wrote, to match them with these results");
            ui.end_row();
        }

        ui.label("Wavefunction");
        let method = wavefunction.and_then(|w| w.method.clone());
        let formalism = wavefunction.and_then(|w| w.formalism.as_deref().map(capitalize));
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
        egui::ScrollArea::both().id_salt("apost_input_scroll").auto_shrink([false, true]).max_height(220.0).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).monospace()).selectable(true).wrap_mode(egui::TextWrapMode::Extend));
        });
    });
}

/// Atom names ("Fe1, C2, N3"), from 0-based indices.
fn atom_names(atomic_numbers: &[u32], atoms: impl IntoIterator<Item = usize>) -> Vec<String> {
    atoms.into_iter().map(|i| atom_name(atomic_numbers, i)).collect()
}

/// At most `limit` names, then a count of the rest.
fn abbreviated(names: &[String], limit: usize) -> String {
    if names.len() <= limit {
        names.join(", ")
    } else {
        format!("{}, ... (+{} more)", names[..limit].join(", "), names.len() - limit)
    }
}

/// What the fragment list asks the window to do after drawing.
enum FragmentAction {
    /// Select the atoms of the fragment at this position in the list;
    /// `add` keeps the current selection (Cmd/Ctrl-click).
    Select { position: usize, add: bool },
}

/// The fragments as APOST-3D defined them, which doubles as the user's check
/// that they were defined as intended: atoms in no fragment, or in two, are
/// called out.
fn show_fragments(ui: &mut egui::Ui, results: &ApostResults, atomic_numbers: &[u32], selected: &[usize], focus_mode: bool) -> Option<FragmentAction> {
    let fragments = results.fragments();
    let mut action = None;
    egui::CollapsingHeader::new(format!("Fragments ({})", fragments.len())).id_salt("apost_fragments").default_open(true).show(ui, |ui| {
        if fragments.is_empty() {
            ui.label(RichText::new("This run defines no fragments.").weak());
            return;
        }
        if fragments.len() > FRAGMENT_PALETTE.len() {
            ui.label(RichText::new(format!("More than {} fragments: colors repeat.", FRAGMENT_PALETTE.len())).small().weak());
        }

        let coverage = fragment_coverage(atomic_numbers.len(), fragments);
        if !coverage.unassigned.is_empty() {
            let names = atom_names(atomic_numbers, coverage.unassigned.iter().copied());
            ui.horizontal_wrapped(|ui| {
                swatch(ui, Color32::from_rgb(UNASSIGNED_COLOR[0], UNASSIGNED_COLOR[1], UNASSIGNED_COLOR[2]), 12.0);
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
                    let [r, g, b] = fragment_color(position);
                    swatch(ui, Color32::from_rgb(r, g, b), 12.0);
                    let text = format!("{}  {formula}   {}", fragment.id, abbreviated(&names, 8));
                    let hint = if focus_mode { "Click to see the values from this fragment." } else { "Click to select its atoms, Cmd/Ctrl-click to add them to the selection." };
                    let response = ui.selectable_label(is_selected, text).on_hover_text(format!("Fragment {}: {}\n{hint}", fragment.id, names.join(", ")));
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
    let name = crate::util::file_display_name(path);
    let response = egui::CollapsingHeader::new(format!("Text report ({name})")).id_salt("apost_report").show(ui, |ui| {
        match &attachment.report_lines {
            Some(Ok(lines)) => {
                if ui.button("Copy all").clicked() {
                    ui.ctx().copy_text(lines.join("\n"));
                }
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                egui::ScrollArea::both().id_salt("apost_report_scroll").auto_shrink([false, true]).max_height(320.0).show_rows(ui, row_height, lines.len(), |ui, rows| {
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
        // Its size stays as the user leaves it, whatever is shown (the
        // content scrolls inside): see `column.rs`.
        let response = self
            .column_window(ctx, ToolWindow::Apost, &mut open)
            .show(ctx, |ui| {
                if self.show_apost_tabs(ui) == crate::input_generator::ApostTab::Input {
                    self.show_input_generator(ui);
                    return;
                }
                if !self.show_apost_header(ui) {
                    return;
                }
                ui.add_space(6.0);
                egui::CollapsingHeader::new(RichText::new("On the molecule").strong()).id_salt("apost_on_molecule").default_open(true).show(ui, |ui| {
                    self.show_results_controls(ui);
                });
                self.show_results_values(ui);
                self.show_selected_pair(ui);
                self.show_enpart_quality(ui);
                self.show_key_numbers(ui);
                self.show_tables(ui);

                let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
                let atomic_numbers = structure.molecule.atomic_numbers.clone();
                let selected_atoms = structure.selected_atoms.clone();
                let Some(attachment) = structure.apost.as_mut() else { return };
                let Ok(results) = &attachment.results else { return };
                let focus_mode = attachment.view.as_ref().and_then(|v| focus_target(results, v)) == Some(true);
                if results.molecule.is_some() {
                    fragment_action = show_fragments(ui, results, &atomic_numbers, &selected_atoms, focus_mode).map(|a| (a, focus_mode));
                }
                show_input_text(ui, results);
                show_report(ui, attachment);
            });
        self.record_column_window(ctx, ToolWindow::Apost, response.map(|r| r.response));
        self.show_apost = open;
        self.apply_fragment_action(fragment_action);
    }

    /// The part of the window shown now (`apost_tab`).
    pub(crate) fn apost_tab_shown(&self) -> crate::input_generator::ApostTab {
        use crate::input_generator::ApostTab;
        self.apost_tab.unwrap_or_else(|| match self.active_structure.and_then(|i| self.structures.get(i)) {
            Some(s) if s.apost.is_none() && s.results_nearby.is_none() => ApostTab::Input,
            _ => ApostTab::Results,
        })
    }

    /// Results | New input, at the top of the window.
    fn show_apost_tabs(&mut self, ui: &mut egui::Ui) -> crate::input_generator::ApostTab {
        use crate::input_generator::ApostTab;
        let mut tab = self.apost_tab_shown();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, ApostTab::Results, RichText::new("Results").strong());
            ui.selectable_value(&mut tab, ApostTab::Input, RichText::new("New input").strong());
        });
        ui.separator();
        if tab != self.apost_tab_shown() {
            self.apost_tab = Some(tab);
        }
        tab
    }

    /// Banners, then the run in one line, with the details folded away.
    /// False when there is nothing more to show (no results, or refused).
    fn show_apost_header(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else {
            ui.label(RichText::new("Open a structure first.").weak());
            return false;
        };
        let Some(attachment) = structure.apost.as_mut() else {
            // A .fchk is the calculation alone; its results are opened on
            // request.
            let nearby = structure.results_nearby.clone();
            match nearby {
                Some(path) => {
                    ui.label("APOST-3D results of this calculation were found next to it:");
                    ui.label(RichText::new(crate::util::file_display_name(&path)).strong());
                    if ui.button("Open these results").clicked()
                        && let Some(active) = self.active_structure
                    {
                        self.attach_results(active, &path);
                    }
                }
                None => {
                    ui.label(RichText::new("No APOST-3D results loaded.").weak());
                    ui.label(RichText::new("To see APOST-3D results, open the results file (.apost.json) APOST-3D wrote for this calculation.").small().weak());
                }
            }
            return false;
        };
        let results = match &attachment.results {
            Ok(results) => results,
            Err(reason) => {
                banner(ui, ERROR_FILL, "Results not loaded", reason);
                return false;
            }
        };
        if results.stopped() {
            let message = results.run.as_ref().and_then(|r| r.stop_message.clone()).unwrap_or_default();
            banner(ui, ERROR_FILL, "APOST-3D stopped before the end of this run", &message);
            ui.add_space(6.0);
        }
        if !attachment.has_fchk {
            banner(ui, NOTICE_FILL, "No .fchk found next to the results file", "The geometry shown comes from the results file; orbitals are not available.");
            ui.add_space(6.0);
        }
        let mut summary = Vec::new();
        if let Some(d) = results.input.as_ref().and_then(|i| i.atomic_definition.as_ref()).and_then(|d| d.keyword.clone()) {
            summary.push(d);
        }
        if let Some(w) = &results.wavefunction {
            summary.push([w.method.clone(), w.formalism.as_deref().map(capitalize)].into_iter().flatten().collect::<Vec<_>>().join(", "));
        }
        let n = results.fragments().len();
        if n > 0 {
            summary.push(format!("{n} fragment{}", if n == 1 { "" } else { "s" }));
        }
        ui.label(RichText::new(summary.join("  \u{b7}  ")).strong());
        egui::CollapsingHeader::new("Run").id_salt("apost_run").default_open(results.index.is_empty()).show(ui, |ui| show_run_summary(ui, attachment, results));
        if !results.index_problems.is_empty() {
            ui.label(RichText::new(format!("{} quantities in the file could not be read: {}", results.index_problems.len(), results.index_problems.join("; "))).small().color(NOTICE_FILL));
        }
        true
    }

    fn apply_fragment_action(&mut self, action: Option<(FragmentAction, bool)>) {
        let Some((FragmentAction::Select { position, add }, focus_mode)) = action else { return };
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let Some(attachment) = structure.apost.as_mut() else { return };
        if focus_mode {
            if let Some(view) = attachment.view.as_mut() {
                view.focus = position;
            }
            return;
        }
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
