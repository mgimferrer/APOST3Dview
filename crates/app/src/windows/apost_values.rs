//! The APOST-3D window's results sections: what to show on the molecule
//! (M3-M5) and the values themselves, as a table, or for a pair quantity
//! a heat map and the strongest pairs.

use apost3dview_core::{element_data, hill_formula, ApostResults, MatrixObject, Molecule, Quantity};
use apost3dview_render::{format_signed, ColorScale};
use egui::{Color32, RichText, Sense};

use crate::app::App;
use crate::results_view::{build_scene, focus_target, grouped_entries, shown_entry, ResultsView, Shown, FOCUS_COLOR};

fn color32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

fn fragment_name(results: &ApostResults, molecule: &Molecule, f: usize) -> String {
    let fragment = &results.fragments()[f];
    let formula = hill_formula(fragment.atoms.iter().filter_map(|&a| molecule.atomic_numbers.get(a - 1).copied()));
    format!("{} {formula}", fragment.id)
}

fn atom_name(molecule: &Molecule, a: usize) -> String {
    format!("{}{}", element_data(molecule.atomic_numbers[a]).symbol, a + 1)
}

fn shown_label(results: &ApostResults, shown: &Shown) -> String {
    match shown {
        Shown::Nothing => "Nothing".to_string(),
        Shown::Fragments => "Fragments".to_string(),
        Shown::Quantity(id) => results.index.iter().find(|e| &e.id == id).map(|e| e.title.clone()).unwrap_or_else(|| id.clone()),
    }
}

/// A matrix as tab-separated text, with names, for a spreadsheet.
fn matrix_text(matrix: &MatrixObject, names: &[String]) -> String {
    let mut text = format!("\t{}\n", names.join("\t"));
    for (i, row) in matrix.values.iter().enumerate() {
        let cells: Vec<String> = row.iter().map(|v| v.map(|x| format!("{x}")).unwrap_or_default()).collect();
        text.push_str(&format!("{}\t{}\n", names[i], cells.join("\t")));
    }
    text
}

impl App {
    /// "On the molecule": the one choice of what to draw, and how.
    pub(crate) fn show_results_controls(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active_structure else { return };
        let list_comparison = &mut self.list_comparison_values;
        let structure = &mut self.structures[active];
        let molecule = structure.molecule.clone();
        let Some(attachment) = structure.apost.as_mut() else { return };
        let (Ok(results), Some(view)) = (&attachment.results, attachment.view.as_mut()) else { return };
        if results.molecule.is_none() {
            return;
        }

        ui.horizontal(|ui| {
            ui.label("Show");
            egui::ComboBox::from_id_salt("results_shown").width(250.0).selected_text(shown_label(results, &view.shown)).show_ui(ui, |ui| {
                ui.selectable_value(&mut view.shown, Shown::Nothing, "Nothing");
                if !results.fragments().is_empty() {
                    ui.selectable_value(&mut view.shown, Shown::Fragments, "Fragments");
                }
                for (group, entries) in grouped_entries(results, *list_comparison) {
                    ui.separator();
                    ui.label(RichText::new(group).small().weak());
                    for entry in entries {
                        let value = Shown::Quantity(entry.id.clone());
                        if ui.selectable_label(view.shown == value, &entry.title).clicked() {
                            view.shown = value;
                            view.focus = 0;
                        }
                    }
                }
                if results.index.iter().any(|e| e.is_comparison()) {
                    ui.separator();
                    ui.checkbox(list_comparison, "Also list Mulliken values (for comparison)");
                }
            });
            ui.checkbox(&mut view.visible, "").on_hover_text("Show or hide on the molecule, keeping the choice");
        });

        let entry = shown_entry(results, view).cloned();
        if let Some(entry) = &entry {
            if let Some(on_fragments) = focus_target(results, view) {
                let count = if on_fragments { results.fragments().len() } else { molecule.atomic_numbers.len() };
                let name = |i: usize| if on_fragments { fragment_name(results, &molecule, i) } else { atom_name(&molecule, i) };
                view.focus = view.focus.min(count.saturating_sub(1));
                ui.horizontal(|ui| {
                    ui.label("Seen from");
                    egui::ComboBox::from_id_salt("results_focus").selected_text(name(view.focus)).show_ui(ui, |ui| {
                        for i in 0..count {
                            ui.selectable_value(&mut view.focus, i, name(i));
                        }
                    });
                });
                let what = if on_fragments { "a fragment" } else { "an atom" };
                ui.label(RichText::new(format!("Click {what} in the view to see it from there.")).small().weak());
            }

            // Color range: automatic, or fixed for this quantity everywhere.
            let automatic = build_scene(results, &molecule, &ResultsView { ..view.clone() }, &Default::default()).legend.map(|l| l.scale);
            let mut fixed = self.color_ranges.get(&entry.id).copied();
            ui.horizontal(|ui| {
                ui.label("Colors");
                if ui.selectable_label(fixed.is_none(), "Automatic").clicked() {
                    fixed = None;
                }
                if ui.selectable_label(fixed.is_some(), "Fixed").on_hover_text("Keep this range for this quantity in every structure, so figures compare").clicked() && fixed.is_none() {
                    fixed = automatic;
                }
                match &mut fixed {
                    Some(ColorScale::Diverging { max_abs }) => {
                        ui.label("\u{b1}");
                        ui.add(egui::DragValue::new(max_abs).speed(0.005).range(1e-4..=1e4).max_decimals(4));
                    }
                    Some(ColorScale::Sequential { min, max }) => {
                        ui.add(egui::DragValue::new(min).speed(0.005).max_decimals(4));
                        ui.label("to");
                        ui.add(egui::DragValue::new(max).speed(0.005).max_decimals(4));
                    }
                    None => {}
                }
            });
            match fixed {
                Some(scale) => {
                    self.color_ranges.insert(entry.id.clone(), scale);
                }
                None => {
                    self.color_ranges.remove(&entry.id);
                }
            }

            ui.horizontal(|ui| {
                ui.checkbox(&mut view.labels, "Values on the molecule");
                if view.labels {
                    let mut auto = view.decimals.is_none();
                    ui.label("decimals");
                    if ui.selectable_label(auto, "auto").clicked() {
                        auto = !auto;
                        view.decimals = if auto { None } else { Some(2) };
                    }
                    if let Some(d) = &mut view.decimals {
                        ui.add(egui::DragValue::new(d).range(0..=6));
                    }
                }
            });
            if view.labels {
                ui.add(egui::Slider::new(&mut view.label_size, 12.0..=48.0).text("label size"));
            }
        }

        if results.index.iter().any(|e| e.id == "bond_orders.atoms.matrix") {
            ui.checkbox(&mut view.bond_thickness, "Bond thickness from bond orders")
                .on_hover_text("Bonds drawn thicker for larger bond orders; off: all bonds equal, usually nicer for figures");
        }
    }

    /// The values of the shown quantity: a table, or for a pair quantity
    /// a heat map (small systems) and the strongest pairs.
    pub(crate) fn show_results_values(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active_structure else { return };
        let structure = &mut self.structures[active];
        let molecule = structure.molecule.clone();
        let Some(attachment) = structure.apost.as_mut() else { return };
        let (Ok(results), Some(view)) = (&attachment.results, attachment.view.as_mut()) else { return };
        let Some(entry) = shown_entry(results, view).cloned() else { return };
        let Ok(quantity) = results.quantity(&entry) else { return };
        let on_fragments = entry.kind.on_fragments();
        let names: Vec<String> = if on_fragments {
            (0..results.fragments().len()).map(|f| fragment_name(results, &molecule, f)).collect()
        } else {
            (0..molecule.atomic_numbers.len()).map(|a| atom_name(&molecule, a)).collect()
        };
        let decimals = entry.decimals.unwrap_or(6).min(6);
        let scene = build_scene(results, &molecule, view, &self.color_ranges);
        let scale = scene.legend.as_ref().map(|l| l.scale);

        egui::CollapsingHeader::new(RichText::new(&entry.title).strong()).id_salt("results_values").default_open(true).show(ui, |ui| match &quantity {
            Quantity::Vector(values) => {
                if ui.button("Copy").on_hover_text("As tab-separated text, for a spreadsheet").clicked() {
                    let text: String = names.iter().zip(values).map(|(n, v)| format!("{n}\t{}\n", v.map(|x| x.to_string()).unwrap_or_default())).collect();
                    ui.ctx().copy_text(text);
                }
                let signed = matches!(scale, Some(ColorScale::Diverging { .. }));
                let row_height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
                egui::ScrollArea::vertical().id_salt("results_vector").max_height(220.0).show_rows(ui, row_height, values.len(), |ui, rows| {
                    for i in rows {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                            if let (Some(scale), Some(v)) = (scale, values[i]) {
                                ui.painter().rect_filled(rect, 2.0, color32(scale.color(v)));
                            }
                            ui.add_sized([110.0, row_height], egui::Label::new(&names[i]).truncate());
                            let text = values[i].map(|v| if signed { format_signed(v, decimals) } else { format!("{v:.decimals$}") }).unwrap_or_else(|| "n/a".into());
                            ui.label(RichText::new(text).monospace());
                        });
                    }
                });
            }
            Quantity::Matrix(matrix) => {
                if let (Some(d), Some(o)) = (&matrix.diagonal, &matrix.off_diagonal) {
                    ui.label(RichText::new(format!("Diagonal: {d}. Off-diagonal: {o}.")).small().weak());
                }
                if let (Some(total), Some(rule)) = (matrix.total, &matrix.total_rule) {
                    ui.label(format!("Total {total:.decimals$} ({rule})"));
                }
                if ui.button("Copy matrix").on_hover_text("As tab-separated text, for a spreadsheet").clicked() {
                    ui.ctx().copy_text(matrix_text(matrix, &names));
                }
                if let Some(focus) = heat_map(ui, matrix, &names, view.focus) {
                    view.focus = focus;
                }
                if let Some(focus) = strongest_pairs(ui, matrix, &names, decimals) {
                    view.focus = focus;
                }
            }
        });
    }

    /// The values of every atom-pair quantity for the two selected atoms.
    pub(crate) fn show_selected_pair(&self, ui: &mut egui::Ui) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return };
        let [a, b] = structure.selected_atoms[..] else { return };
        let values = self.pair_values(a, b);
        if values.is_empty() {
            return;
        }
        let pair = format!("{} \u{2013} {}", atom_name(&structure.molecule, a), atom_name(&structure.molecule, b));
        egui::CollapsingHeader::new(RichText::new(format!("Selected pair {pair}")).strong()).id_salt("results_pair").default_open(true).show(ui, |ui| {
            egui::Grid::new("results_pair_grid").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                for (title, value, decimals) in values {
                    ui.label(title);
                    let d = decimals.unwrap_or(6).min(6);
                    ui.label(RichText::new(format!("{value:.d$}")).monospace());
                    ui.end_row();
                }
            });
        });
    }
}

/// A heat map of the matrix (up to 40 x 40): off-diagonal cells shaded by
/// magnitude, the diagonal (a different quantity) in gray, the focused row
/// outlined. Hovering shows the value; clicking a cell focuses on its row.
fn heat_map(ui: &mut egui::Ui, matrix: &MatrixObject, names: &[String], focus: usize) -> Option<usize> {
    let n = matrix.values.len();
    if n == 0 || n > 40 {
        return None;
    }
    let max = (0..n).flat_map(|i| (0..n).filter(move |j| *j != i).map(move |j| (i, j))).filter_map(|(i, j)| matrix.get(i, j)).fold(0.0f64, |m, v| m.max(v.abs()));
    let cell = (300.0 / n as f32).clamp(6.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(cell * n as f32, cell * n as f32), Sense::click());
    let painter = ui.painter_at(rect);
    for i in 0..n {
        for j in 0..n {
            let r = egui::Rect::from_min_size(rect.min + egui::vec2(j as f32 * cell, i as f32 * cell), egui::vec2(cell, cell)).shrink(0.5);
            let color = if i == j {
                Color32::from_gray(150)
            } else {
                let v = matrix.get(i, j).unwrap_or(0.0).abs();
                color32(apost3dview_render::colormap::sequential(if max > 0.0 { (v / max) as f32 } else { 0.0 }))
            };
            painter.rect_filled(r, 1.0, color);
        }
    }
    let focus_row = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, focus as f32 * cell), egui::vec2(cell * n as f32, cell));
    painter.rect_stroke(focus_row, 1.0, egui::Stroke::new(2.0, color32(FOCUS_COLOR)), egui::StrokeKind::Inside);

    let cell_at = |pos: egui::Pos2| {
        let local = pos - rect.min;
        let (i, j) = ((local.y / cell) as usize, (local.x / cell) as usize);
        (i < n && j < n).then_some((i, j))
    };
    if let Some((i, j)) = response.hover_pos().and_then(cell_at) {
        let value = matrix.get(i, j).map(|v| format!("{v:.4}")).unwrap_or_else(|| "n/a".into());
        let meaning = if i == j { matrix.diagonal.clone() } else { matrix.off_diagonal.clone() }.unwrap_or_default();
        let pair = if i == j { names[i].clone() } else { format!("{} \u{2013} {}", names[i], names[j]) };
        response.clone().on_hover_text(format!("{pair}: {value}\n{meaning}"));
    }
    if response.clicked() {
        return response.interact_pointer_pos().and_then(cell_at).map(|(i, _)| i);
    }
    None
}

/// The off-diagonal pairs, strongest first; clicking one focuses on it.
fn strongest_pairs(ui: &mut egui::Ui, matrix: &MatrixObject, names: &[String], decimals: usize) -> Option<usize> {
    let n = matrix.values.len();
    let mut pairs: Vec<(usize, usize, f64)> = (0..n).flat_map(|i| ((i + 1)..n).map(move |j| (i, j))).filter_map(|(i, j)| matrix.get(i, j).map(|v| (i, j, v))).collect();
    pairs.sort_by(|a, b| b.2.abs().total_cmp(&a.2.abs()));
    ui.label(RichText::new("Strongest pairs").small().weak());
    let mut clicked = None;
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    egui::ScrollArea::vertical().id_salt("results_pairs").max_height(160.0).show_rows(ui, row_height, pairs.len(), |ui, rows| {
        for k in rows {
            let (i, j, v) = pairs[k];
            if ui.selectable_label(false, format!("{} \u{2013} {}   {v:.decimals$}", names[i], names[j])).clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}
