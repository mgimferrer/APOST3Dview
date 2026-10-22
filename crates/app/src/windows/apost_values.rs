//! The APOST-3D window's results sections: what to show on the molecule
//! (M3-M5) and the values themselves, as a table, or for a pair quantity
//! a heat map and the strongest pairs.

use apost3dview_core::{ApostResults, MatrixObject, Quantity};
use apost3dview_render::{format_signed, ColorScale};
use egui::{Color32, RichText, Sense};

use crate::app::App;
use crate::names::{atom_name, fragment_name};
use crate::util::rgb_to_color32;
use crate::results_view::{build_scene, display_unit, enpart_term, enpart_term_name, is_energy, scaled_quantity, EnergyUnit, focus_target, choice_label, grouped_entries, is_secondary, matrix_meanings, range_key, shows_diagonal, shown_entry, ResultsView, Shown, FOCUS_COLOR};

fn shown_label(results: &ApostResults, view: &ResultsView) -> String {
    match &view.shown {
        Shown::Nothing => "Nothing".to_string(),
        Shown::Fragments => "Fragments".to_string(),
        Shown::Quantity(id) => match results.entry(id) {
            Some(entry) => choice_label(results, entry, shows_diagonal(results, view)),
            None => id.clone(),
        },
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
        let list_other = &mut self.list_other_values;
        let structure = &mut self.structures[active];
        let molecule = structure.molecule.clone();
        let Some(attachment) = structure.apost.as_mut() else { return };
        let (Ok(results), Some(view)) = (&attachment.results, attachment.view.as_mut()) else { return };
        if results.molecule.is_none() {
            return;
        }

        ui.horizontal(|ui| {
            ui.label("Show");
            egui::ComboBox::from_id_salt("results_shown").width(250.0).selected_text(shown_label(results, view)).show_ui(ui, |ui| {
                ui.selectable_value(&mut view.shown, Shown::Nothing, "Nothing");
                if !results.fragments().is_empty() {
                    ui.selectable_value(&mut view.shown, Shown::Fragments, "Fragments");
                }
                for (group, choices) in grouped_entries(results, *list_other) {
                    ui.separator();
                    ui.label(RichText::new(group).small().weak());
                    for choice in choices {
                        let value = Shown::Quantity(choice.entry.id.clone());
                        let selected = view.shown == value && shows_diagonal(results, view) == choice.diagonal;
                        if ui.selectable_label(selected, &choice.label).clicked() {
                            view.shown = value;
                            view.diagonal = choice.diagonal;
                            view.focus = 0;
                        }
                    }
                }
                if results.index.iter().any(|e| e.is_comparison() || is_secondary(e)) {
                    ui.separator();
                    ui.checkbox(list_other, "Also list other values").on_hover_text(
                        "Mulliken values printed for comparison, electron populations and counts, valences, overlap populations",
                    );
                }
            });
            ui.checkbox(&mut view.visible, "").on_hover_text("Show or hide on the molecule, keeping the choice");
        });

        // IQA: the term shown, between the same atoms or fragments.
        if let Some((term, on_fragments)) = shown_entry(results, view).and_then(|e| enpart_term(&e.id)).map(|(t, f)| (t.to_string(), f)) {
            let level = if on_fragments { "fragments" } else { "atoms" };
            let terms: Vec<String> = results.enpart().map(|e| e.terms).unwrap_or_default().into_iter().filter(|t| results.entry(&format!("enpart.{t}.{level}")).is_some()).collect();
            ui.horizontal(|ui| {
                ui.label("Term");
                egui::ComboBox::from_id_salt("results_iqa_term").width(250.0).selected_text(enpart_term_name(&term)).show_ui(ui, |ui| {
                    for t in &terms {
                        if ui.selectable_label(*t == term, enpart_term_name(t)).clicked() {
                            view.shown = Shown::Quantity(format!("enpart.{t}.{level}"));
                        }
                    }
                });
            });
        }

        let entry = shown_entry(results, view).cloned();
        if let Some(entry) = &entry {
            if let Some(on_fragments) = focus_target(results, view) {
                let count = if on_fragments { results.fragments().len() } else { molecule.atomic_numbers.len() };
                let name = |i: usize| if on_fragments { fragment_name(&results.fragments()[i], &molecule.atomic_numbers) } else { atom_name(&molecule.atomic_numbers, i) };
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

            if is_energy(entry) {
                energy_unit_selector(ui, &mut view.energy_unit);
            }

            // Color range: automatic, or fixed for this quantity everywhere
            // (kept in the file's unit, shown in the chosen one).
            let (factor, _) = display_unit(entry, view);
            let automatic = build_scene(results, &molecule, &ResultsView { ..view.clone() }, &Default::default()).legend.map(|l| l.scale);
            let key = range_key(entry, shows_diagonal(results, view));
            let mut fixed = self.color_ranges.get(&key).map(|s| s.scaled(factor));
            ui.horizontal(|ui| {
                ui.label("Colors");
                if ui.selectable_label(fixed.is_none(), "Automatic").clicked() {
                    fixed = None;
                }
                if ui.selectable_label(fixed.is_some(), "Fixed").on_hover_text("Keep this range for this quantity in every structure, so figures compare").clicked() && fixed.is_none() {
                    fixed = automatic;
                }
                match &mut fixed {
                    Some(ColorScale::Diverging { max_abs, .. }) => {
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
            if matches!(automatic, Some(ColorScale::Diverging { .. })) {
                ui.checkbox(&mut view.log_scale, "Logarithmic colors")
                    .on_hover_text("Colors follow the order of magnitude, so small values (weak interactions) keep their sign's color next to large ones");
            }
            for note in build_scene(results, &molecule, view, &self.color_ranges).notes {
                ui.label(RichText::new(note).small().weak());
            }
            match fixed {
                Some(scale) => {
                    self.color_ranges.insert(key, scale.scaled(1.0 / factor));
                }
                None => {
                    self.color_ranges.remove(&key);
                }
            }

            ui.horizontal(|ui| {
                ui.checkbox(&mut view.labels, "Values on the molecule");
                if view.labels {
                    ui.checkbox(&mut view.names, "with names")
                        .on_hover_text("Each value's atom or fragment named above it; for atoms, as the Molecule window's Labels setting names them");
                }
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

        if results.entry("bond_orders.atoms.matrix").is_some() {
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
        let (factor, unit) = display_unit(&entry, view);
        let quantity = scaled_quantity(quantity, factor);
        let on_fragments = entry.kind.on_fragments();
        let names: Vec<String> = if on_fragments {
            results.fragments().iter().map(|f| fragment_name(f, &molecule.atomic_numbers)).collect()
        } else {
            (0..molecule.atomic_numbers.len()).map(|a| atom_name(&molecule.atomic_numbers, a)).collect()
        };
        let decimals = if unit.is_some() { view.energy_unit.decimals() } else { entry.decimals.unwrap_or(6).min(6) };
        let scene = build_scene(results, &molecule, view, &self.color_ranges);
        let scale = scene.legend.as_ref().map(|l| l.scale);

        egui::CollapsingHeader::new(RichText::new(&entry.title).strong()).id_salt("results_values").default_open(true).show(ui, |ui| match &quantity {
            Quantity::Scalar(_) | Quantity::Table(_) => {}
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
                                ui.painter().rect_filled(rect, 2.0, rgb_to_color32(scale.color(v)));
                            }
                            ui.add_sized([110.0, row_height], egui::Label::new(&names[i]).truncate());
                            let text = values[i].map(|v| if signed { format_signed(v, decimals) } else { format!("{v:.decimals$}") }).unwrap_or_else(|| "n/a".into());
                            ui.label(RichText::new(text).monospace());
                        });
                    }
                });
            }
            Quantity::Matrix(matrix) => {
                let meanings = matrix_meanings(&entry, matrix);
                if let (Some(d), Some(o)) = &meanings {
                    ui.label(RichText::new(format!("Diagonal: {d}. Off-diagonal: {o}.")).small().weak());
                }
                if let (Some(total), Some(rule)) = (matrix.total, &matrix.total_rule) {
                    let unit = unit.map(|u| format!(" {u}")).unwrap_or_default();
                    ui.label(format!("Total {total:.decimals$}{unit} ({rule})"));
                }
                if ui.button("Copy matrix").on_hover_text("As tab-separated text, for a spreadsheet").clicked() {
                    ui.ctx().copy_text(matrix_text(matrix, &names));
                }
                let signed = matches!(scale, Some(ColorScale::Diverging { .. }));
                if let Some(focus) = heat_map(ui, matrix, &meanings, &names, view.focus, signed, view.log_scale) {
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
        let pair = format!("{} - {}", atom_name(&structure.molecule.atomic_numbers, a), atom_name(&structure.molecule.atomic_numbers, b));
        // The IQA terms of the pair apart, as a decomposition of its
        // interaction energy.
        let (mut iqa, other): (Vec<_>, Vec<_>) = values.into_iter().partition(|(entry, _)| enpart_term(&entry.id).is_some());
        // Total first, then its parts, as the term selector lists them.
        let order = |id: &str| enpart_term(id).and_then(|(t, _)| apost3dview_core::ENPART_TERMS.iter().position(|x| *x == t)).unwrap_or(usize::MAX);
        iqa.sort_by_key(|(entry, _)| order(&entry.id));
        let enpart = structure.apost.as_ref().and_then(|x| x.results.as_ref().ok()).and_then(|r| r.enpart());
        let energy_unit = structure.apost.as_ref().and_then(|x| x.view.as_ref()).map(|v| v.energy_unit).unwrap_or_default();
        let (factor, d) = (energy_unit.factor(), energy_unit.decimals());
        egui::CollapsingHeader::new(RichText::new(format!("Selected pair {pair}")).strong()).id_salt("results_pair").default_open(true).show(ui, |ui| {
            egui::Grid::new("results_pair_grid").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                for (entry, value) in &other {
                    ui.label(&entry.title);
                    let d = entry.decimals.unwrap_or(6).min(6);
                    ui.label(RichText::new(format!("{value:.d$}")).monospace());
                    ui.end_row();
                }
            });
            if !iqa.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Inter-atomic interaction energy (IQA), {}", energy_unit.name())).small().weak());
                egui::Grid::new("results_pair_iqa").num_columns(2).spacing([12.0, 3.0]).striped(true).show(ui, |ui| {
                    for (entry, value) in &iqa {
                        let Some((term, _)) = enpart_term(&entry.id) else { continue };
                        if term == "kinetic" {
                            continue;
                        }
                        ui.label(enpart_term_name(term));
                        ui.label(RichText::new(format_signed(*value * factor, d)).monospace());
                        ui.end_row();
                    }
                });
                if enpart.is_some_and(|e| e.is_multipolar(a, b)) {
                    ui.label(RichText::new("This pair's exchange-correlation comes from the multipolar expansion (approximate).").small().weak());
                }
            }
        });
    }
}

/// A heat map of the matrix (up to 40 x 40): off-diagonal cells shaded by
/// magnitude, the diagonal (a different quantity) in gray, the focused row
/// outlined. Hovering shows the value; clicking a cell focuses on its row.
/// The matrix as colored cells, the focus's row outlined. A signed
/// quantity uses the diverging scale (`log` as on the molecule), others
/// the magnitude.
fn heat_map(
    ui: &mut egui::Ui,
    matrix: &MatrixObject,
    meanings: &(Option<String>, Option<String>),
    names: &[String],
    focus: usize,
    signed: bool,
    log: bool,
) -> Option<usize> {
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
                let v = matrix.get(i, j).unwrap_or(0.0);
                if signed {
                    rgb_to_color32(ColorScale::Diverging { max_abs: if max > 0.0 { max } else { 1.0 }, log }.color(v))
                } else {
                    rgb_to_color32(apost3dview_render::colormap::sequential(if max > 0.0 { (v.abs() / max) as f32 } else { 0.0 }))
                }
            };
            painter.rect_filled(r, 1.0, color);
        }
    }
    let focus_row = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, focus as f32 * cell), egui::vec2(cell * n as f32, cell));
    painter.rect_stroke(focus_row, 1.0, egui::Stroke::new(2.0, rgb_to_color32(FOCUS_COLOR)), egui::StrokeKind::Inside);

    let cell_at = |pos: egui::Pos2| {
        let local = pos - rect.min;
        let (i, j) = ((local.y / cell) as usize, (local.x / cell) as usize);
        (i < n && j < n).then_some((i, j))
    };
    if let Some((i, j)) = response.hover_pos().and_then(cell_at) {
        let value = matrix.get(i, j).map(|v| format!("{v:.4}")).unwrap_or_else(|| "n/a".into());
        let meaning = if i == j { meanings.0.clone() } else { meanings.1.clone() }.unwrap_or_default();
        let pair = if i == j { names[i].clone() } else { format!("{} - {}", names[i], names[j]) };
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
            if ui.selectable_label(false, format!("{} - {}   {v:.decimals$}", names[i], names[j])).clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// au or kcal/mol, for the energies of the energy decomposition.
pub(crate) fn energy_unit_selector(ui: &mut egui::Ui, unit: &mut EnergyUnit) {
    ui.horizontal(|ui| {
        ui.label("Energies in");
        ui.selectable_value(unit, EnergyUnit::KcalPerMol, "kcal/mol");
        ui.selectable_value(unit, EnergyUnit::Hartree, "au");
    });
}
