//! The APOST-3D window's generic views of what is not drawn on the
//! molecule: the key numbers of the run (scalars, such as the reliability
//! index R of EOS) and its tables (EFOs, OSLOs, frontier occupations), one
//! at a time, with a Draw button on each orbital row that has its orbital
//! in an orbital file.

use apost3dview_core::{ApostResults, IndexEntry, Molecule, Quantity, QuantityKind, TableCell, TableColumn, TableObject};
use apost3dview_render::format_signed;
use egui::{Color32, RichText};

use crate::app::App;
use crate::orbital_sets::{fragment_name, LoadedOrbitalSet};
use crate::results::fragment_color;

/// The orbital set (by analysis) and channel (beta) whose orbitals a
/// table describes: its `fchk_orbitals` block, in the analysis it sits in.
/// `None` for a table with no orbital file (EFFAO's, frontier tables).
fn table_channel<'a>(entry: &'a IndexEntry, table: &TableObject) -> Option<(&'a str, bool)> {
    let analysis = entry.path.strip_prefix("/analyses/")?.split('/').next()?;
    match table.fchk_orbitals.as_deref()? {
        "alpha" => Some((analysis, false)),
        "beta" => Some((analysis, true)),
        _ => None,
    }
}

fn number_text(value: f64, column: &TableColumn) -> String {
    let decimals = column.decimals.unwrap_or(4).min(6);
    if column.signed { format_signed(value, decimals) } else { format!("{value:.decimals$}") }
}

/// One cell as text: a number, a list (fragments joined by "+", shares by
/// "/"), or nothing.
fn cell_text(table: &TableObject, column: &TableColumn, row: usize, results: &ApostResults, molecule: &Molecule) -> String {
    let fragment_column = column.key == "fragment" || column.key == "fragments";
    match table.cell_value(&column.key, row) {
        TableCell::Number(v) if fragment_column => fragment_name(results, molecule, v as u32),
        TableCell::Number(v) => number_text(v, column),
        TableCell::List(values) => {
            if fragment_column {
                values.iter().map(|&v| fragment_name(results, molecule, v as u32)).collect::<Vec<_>>().join(" + ")
            } else {
                values.iter().map(|&v| number_text(v, column)).collect::<Vec<_>>().join(" / ")
            }
        }
        TableCell::Empty => String::new(),
    }
}

fn header_text(column: &TableColumn) -> String {
    match &column.unit {
        Some(unit) if !unit.is_empty() => format!("{} ({unit})", column.title),
        _ => column.title.clone(),
    }
}

/// A table as tab-separated text, for a spreadsheet.
fn table_text(table: &TableObject, names: &[String], results: &ApostResults, molecule: &Molecule) -> String {
    let mut text = format!("\t{}\n", table.columns.iter().map(header_text).collect::<Vec<_>>().join("\t"));
    for (row, name) in names.iter().enumerate() {
        let cells: Vec<String> = table.columns.iter().map(|c| cell_text(table, c, row, results, molecule)).collect();
        text.push_str(&format!("{name}\t{}\n", cells.join("\t")));
    }
    text
}

fn fragment_swatches(ui: &mut egui::Ui, results: &ApostResults, ids: &[f64]) {
    for &id in ids {
        if let Some(position) = results.fragments().iter().position(|f| f.id == id as u32) {
            let [r, g, b] = fragment_color(position);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
        }
    }
}

impl App {
    /// The run's single numbers, in one short list.
    pub(crate) fn show_key_numbers(&mut self, ui: &mut egui::Ui) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return };
        let Some(Ok(results)) = structure.apost.as_ref().map(|a| &a.results) else { return };
        let entries: Vec<&IndexEntry> = results.entries(QuantityKind::Scalar).filter(|e| self.list_comparison_values || !e.is_comparison()).collect();
        if entries.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(RichText::new("Key numbers").strong()).id_salt("apost_key_numbers").default_open(true).show(ui, |ui| {
            egui::Grid::new("apost_key_numbers_grid").num_columns(2).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                for entry in entries {
                    let Ok(Quantity::Scalar(value)) = results.quantity(entry) else { continue };
                    let decimals = entry.decimals.unwrap_or(4).min(6);
                    let text = match value {
                        Some(v) if entry.signed == Some(true) => format_signed(v, decimals),
                        Some(v) => format!("{v:.decimals$}"),
                        None => "n/a".to_string(),
                    };
                    let unit = entry.unit.as_deref().filter(|u| !u.is_empty()).map(|u| if u == "%" { u.to_string() } else { format!(" {u}") }).unwrap_or_default();
                    ui.label(&entry.title);
                    ui.label(RichText::new(format!("{text}{unit}")).monospace());
                    ui.end_row();
                }
            });
        });
    }

    /// One table at a time, chosen from a menu: orbital rows named after
    /// their orbital (with a Draw button when its orbital file is here),
    /// fragment rows after their fragment.
    pub(crate) fn show_tables(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active_structure else { return };
        let structure = &self.structures[active];
        let Some(Ok(results)) = structure.apost.as_ref().map(|a| &a.results) else { return };
        let entries: Vec<&IndexEntry> = results.entries(QuantityKind::Table).filter(|e| self.list_comparison_values || !e.is_comparison()).collect();
        if entries.is_empty() {
            return;
        }
        // By default the first table of orbitals (the EFOs or OSLOs), the
        // one with something to draw.
        let drawable = |e: &IndexEntry| matches!(results.quantity(e), Ok(Quantity::Table(t)) if table_channel(e, &t).is_some());
        let default = entries.iter().find(|e| drawable(e)).copied().unwrap_or(entries[0]);
        let chosen = self.apost_table.as_ref().and_then(|id| entries.iter().find(|e| &e.id == id)).copied().unwrap_or(default);
        let mut choice = chosen.id.clone();
        let mut draw = None;
        egui::CollapsingHeader::new(RichText::new(format!("Tables ({})", entries.len()))).id_salt("apost_tables").show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("apost_table").width(280.0).selected_text(&chosen.title).show_ui(ui, |ui| {
                    for entry in &entries {
                        ui.selectable_value(&mut choice, entry.id.clone(), &entry.title);
                    }
                });
            });
            let Ok(Quantity::Table(table)) = results.quantity(chosen) else { return };
            let channel = table_channel(chosen, &table);
            let set: Option<&LoadedOrbitalSet> = channel.and_then(|(analysis, _)| structure.orbital_sets.iter().find(|s| s.set.analysis == analysis && !s.set.preortho));
            let by_orbital = table.rows == "orbital";
            let names: Vec<String> = (0..table.n_rows)
                .map(|row| {
                    if by_orbital {
                        match (set, channel, table.number("fchk_column", row)) {
                            (Some(set), Some((_, beta)), Some(column)) if column >= 1.0 => set.orbital_name(beta, column as usize - 1),
                            _ => format!("{}", row + 1),
                        }
                    } else {
                        results.fragments().get(row).map(|f| fragment_name(results, &structure.molecule, f.id)).unwrap_or_else(|| format!("{}", row + 1))
                    }
                })
                .collect();
            // The .fchk column is what the Draw button uses; it is not
            // shown as a number of its own.
            let columns: Vec<&TableColumn> = table.columns.iter().filter(|c| c.key != "fchk_column").collect();
            let can_draw = by_orbital && set.is_some() && table.has_column("fchk_column");
            ui.horizontal(|ui| {
                if ui.button("Copy").on_hover_text("As tab-separated text, for a spreadsheet").clicked() {
                    ui.ctx().copy_text(table_text(&table, &names, results, &structure.molecule));
                }
                if can_draw {
                    ui.label(RichText::new("Draw shows the orbital as a surface.").small().weak());
                } else if by_orbital && channel.is_none() {
                    ui.label(RichText::new("No orbital file for these orbitals.").small().weak());
                }
            });
            egui::ScrollArea::both().id_salt(("apost_table_rows", &chosen.id)).max_height(260.0).show(ui, |ui| {
                egui::Grid::new(("apost_table_grid", &chosen.id)).num_columns(columns.len() + 2).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                    ui.label(RichText::new(if by_orbital { "Orbital" } else { "Fragment" }).strong());
                    for column in &columns {
                        ui.label(RichText::new(header_text(column)).strong());
                    }
                    if can_draw {
                        ui.label("");
                    }
                    ui.end_row();
                    for row in 0..table.n_rows {
                        ui.horizontal(|ui| {
                            if !by_orbital {
                                if let Some(f) = results.fragments().get(row) {
                                    fragment_swatches(ui, results, &[f.id as f64]);
                                }
                            }
                            ui.label(&names[row]);
                        });
                        for column in &columns {
                            let text = cell_text(&table, column, row, results, &structure.molecule);
                            if column.key == "fragment" || column.key == "fragments" {
                                ui.horizontal(|ui| {
                                    fragment_swatches(ui, results, &table.numbers(&column.key, row).unwrap_or_default());
                                    ui.label(text);
                                });
                            } else {
                                ui.label(RichText::new(text).monospace());
                            }
                        }
                        if can_draw {
                            match (channel, table.number("fchk_column", row)) {
                                (Some((analysis, beta)), Some(column)) if column >= 1.0 => {
                                    if ui.small_button("Draw").on_hover_text(format!("Generate {} as a surface", names[row])).clicked() {
                                        draw = Some((analysis.to_string(), beta, column as usize - 1));
                                    }
                                }
                                _ => {
                                    ui.label("");
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        });
        self.apost_table = Some(choice);
        if let Some((analysis, beta, index)) = draw {
            self.draw_set_orbital(&analysis, beta, index);
        }
    }
}
