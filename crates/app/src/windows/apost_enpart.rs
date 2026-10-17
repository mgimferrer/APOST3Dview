//! The APOST-3D window's energy decomposition (ENPART, IQA) section: how
//! the decomposition was done (wavefunction, functional), how well its
//! energy adds up to the calculation's (the integration errors, including
//! the two-electron integration and its zero-error correction), and which
//! atom pairs come from the multipolar expansion; and the atomic and
//! fragment energies, as numbers (they are dominated by the core, so as
//! colors they would only tell the elements apart). The pair terms are
//! shown on the molecule (On the molecule, Energy decomposition).

use apost3dview_core::{ApostResults, Enpart, Quantity};
use apost3dview_render::format_signed;
use egui::{Color32, RichText};

use crate::app::App;
use crate::names::{atom_name, fragment_name};
use crate::results_view::EnergyUnit;
use crate::windows::apost_values::energy_unit_selector;

/// Warnings (APOST-3D's suggestions, brief 09): a final error above
/// chemical accuracy when no zero-error correction was applied (after one
/// it is close to zero by construction), a one-electron error above 0.1
/// kcal/mol, a first-grid two-electron error above a few kcal/mol, and a
/// negative or far-from-one damping (a bad interpolation).
const FINAL_ERROR_KCAL: f64 = 1.0;
const ONE_ELECTRON_ERROR_KCAL: f64 = 0.1;
const TWO_ELECTRON_ERROR_KCAL: f64 = 5.0;
const DAMPING_LIMIT: f64 = 3.0;
const NOTICE: Color32 = Color32::from_rgb(200, 130, 20);

fn kcal(value: f64) -> String {
    format!("{} kcal/mol", format_signed(value, if value.abs() < 0.01 { 4 } else { 2 }))
}

fn energy(value: f64) -> String {
    format!("{value:.6} au")
}

/// "KS-DFT, restricted · B3LYP (20% exact exchange)".
fn method_text(enpart: &Enpart) -> String {
    let mut text = [enpart.wavefunction.clone(), enpart.formalism.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ");
    if let Some(f) = &enpart.functional {
        let name = match f.keyword.as_deref() {
            Some("LIBRARY") | None => f.components.iter().filter_map(|c| c.name.clone()).collect::<Vec<_>>().join(" + "),
            Some(keyword) => keyword.to_string(),
        };
        let exact = f.exact_exchange_fraction.filter(|x| *x > 0.0).map(|x| format!(" ({}% exact exchange)", (x * 100.0).round())).unwrap_or_default();
        if !name.is_empty() {
            text = format!("{text} \u{b7} {name}{exact}");
        }
    }
    text
}

fn pair_name(atomic_numbers: &[u32], [a, b]: [usize; 2]) -> String {
    format!("{}\u{2013}{}", atom_name(atomic_numbers, a - 1), atom_name(atomic_numbers, b - 1))
}

/// What deserves a warning in an ENPART run, each with its explanation.
fn warnings(enpart: &Enpart) -> Vec<&'static str> {
    let mut found = Vec::new();
    let two = enpart.two_electron_integration.as_ref();
    let corrected = two.and_then(|t| t.zero_error_correction.as_deref()) == Some("applied");
    if !corrected && enpart.integration_error_kcal.is_some_and(|e| e.abs() > FINAL_ERROR_KCAL) {
        found.push("The decomposition's energy differs from the calculation's by more than 1 kcal/mol: a finer integration grid may be needed.");
    }
    if [enpart.electron_nuclear_error_kcal, enpart.kinetic_error_kcal].into_iter().flatten().any(|e| e.abs() > ONE_ELECTRON_ERROR_KCAL) {
        found.push("A one-electron integration error is above 0.1 kcal/mol: the atomic integration grid may be too coarse.");
    }
    if two.and_then(|t| t.error_kcal).is_some_and(|e| e.abs() > TWO_ELECTRON_ERROR_KCAL) {
        found.push("The two-electron integration error on the first grid is above 5 kcal/mol.");
    }
    if two.and_then(|t| t.damping).is_some_and(|d| d < 0.0 || d.abs() > DAMPING_LIMIT) {
        found.push("The zero-error correction's damping is negative or far from 1: a bad interpolation (check the angular grid).");
    }
    found
}

/// One table row per atom (or fragment): name, self-energy, additive energy.
fn energy_rows(results: &ApostResults, level: &str, names: &[String]) -> Option<Vec<(String, Option<f64>, Option<f64>)>> {
    let vector = |key: &str| match results.index.iter().find(|e| e.id == format!("enpart.atomic_energies.{level}.{key}")).map(|e| results.quantity(e)) {
        Some(Ok(Quantity::Vector(v))) => Some(v),
        _ => None,
    };
    let (own, additive) = (vector("self_energy")?, vector("additive_energy")?);
    Some(names.iter().enumerate().map(|(i, n)| (n.clone(), own.get(i).copied().flatten(), additive.get(i).copied().flatten())).collect())
}

impl App {
    pub(crate) fn show_enpart_quality(&mut self, ui: &mut egui::Ui) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let molecule = structure.molecule.clone();
        let Some(attachment) = structure.apost.as_mut() else { return };
        let (Ok(results), Some(view)) = (&attachment.results, attachment.view.as_mut()) else { return };
        let Some(enpart) = results.enpart() else { return };
        let warnings = warnings(&enpart);
        let mut header = RichText::new(match enpart.integration_error_kcal {
            Some(e) => format!("Energy decomposition (IQA) \u{b7} error {}", kcal(e)),
            None => "Energy decomposition (IQA)".to_string(),
        })
        .strong();
        if !warnings.is_empty() {
            header = header.color(NOTICE);
        }
        egui::CollapsingHeader::new(header).id_salt("apost_enpart").show(ui, |ui| {
            if enpart.total_energy.is_none() {
                ui.label(RichText::new("The run stopped during the energy decomposition: no terms were written.").color(NOTICE));
            }
            let two = enpart.two_electron_integration.as_ref();
            let notice = |text: String, warn: bool| if warn { RichText::new(text).monospace().color(NOTICE) } else { RichText::new(text).monospace() };
            egui::Grid::new("apost_enpart_grid").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, name: &str, value: RichText| {
                    ui.label(name);
                    ui.label(value);
                    ui.end_row();
                };
                row(ui, "Method", RichText::new(method_text(&enpart)));
                if let Some(e) = enpart.total_energy {
                    row(ui, "Total energy (IQA)", RichText::new(energy(e)).monospace());
                }
                if let Some(e) = enpart.field_energy {
                    row(ui, "External field energy", RichText::new(energy(e)).monospace());
                }
                if let Some(e) = enpart.reference_energy {
                    row(ui, "Energy of the .fchk", RichText::new(energy(e)).monospace());
                }
                if let Some(e) = enpart.integration_error_kcal {
                    let corrected = two.and_then(|t| t.zero_error_correction.as_deref()) == Some("applied");
                    row(ui, "Integration error", notice(kcal(e), !corrected && e.abs() > FINAL_ERROR_KCAL));
                }
                if let Some(e) = enpart.electron_nuclear_error_kcal {
                    row(ui, "  electron-nuclear", notice(kcal(e), e.abs() > ONE_ELECTRON_ERROR_KCAL));
                }
                if let Some(e) = enpart.kinetic_error_kcal {
                    row(ui, "  kinetic", notice(kcal(e), e.abs() > ONE_ELECTRON_ERROR_KCAL));
                }
                if let Some(t) = two {
                    if let Some(e) = t.error_kcal {
                        let tolerance = match t.tolerance_kcal {
                            Some(x) if x == 0.0 => " (tolerance 0: always corrected)".to_string(),
                            Some(x) => format!(" (tolerance {x})"),
                            None => String::new(),
                        };
                        row(ui, "  two-electron, first grid", notice(format!("{}{tolerance}", kcal(e)), e.abs() > TWO_ELECTRON_ERROR_KCAL));
                    }
                    let damping_bad = t.damping.is_some_and(|d| d < 0.0 || d.abs() > DAMPING_LIMIT);
                    let correction = match t.zero_error_correction.as_deref() {
                        Some("applied") => {
                            let detail = [
                                t.rotated_grid_error_kcal.map(|e| format!("rotated grid {}", kcal(e))),
                                t.damping.map(|d| format!("damping {d:.3}")),
                            ]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(", ");
                            Some(if detail.is_empty() { "applied".to_string() } else { format!("applied ({detail})") })
                        }
                        Some("not needed") => Some("not needed (within tolerance)".to_string()),
                        Some("no reference energy") => Some("not possible".to_string()),
                        Some(other) => Some(other.to_string()),
                        None => None,
                    };
                    if let Some(c) = correction {
                        row(ui, "  zero-error correction", if damping_bad { RichText::new(c).color(NOTICE) } else { RichText::new(c) });
                    }
                    if let Some(e) = t.corrected_error_kcal {
                        row(ui, "  two-electron, corrected", RichText::new(kcal(e)).monospace());
                    }
                }
                if let Some(threshold) = enpart.pair_threshold {
                    let n = enpart.multipolar_pairs.len();
                    row(ui, "Multipolar pairs", RichText::new(format!("{n} (bond order below {threshold})")));
                }
            });
            if two.and_then(|t| t.zero_error_correction.as_deref()) == Some("no reference energy") {
                ui.label(
                    RichText::new("Not possible: the .fchk has no reference energy components (kinetic, electron-nuclear, electron-electron). The total-energy check is still made.")
                        .small()
                        .weak(),
                );
            }
            if !enpart.multipolar_pairs.is_empty() {
                let names: Vec<String> = enpart.multipolar_pairs.iter().map(|&p| pair_name(&molecule.atomic_numbers, p)).collect();
                ui.label(RichText::new(format!("Exchange-correlation from the multipolar expansion: {}.", names.join(", "))).small().weak());
            }
            for warning in &warnings {
                ui.label(RichText::new(*warning).small().color(NOTICE));
            }

            // The atomic (and fragment) energies, as numbers, in the chosen
            // unit (the total energies above stay in au, as in the .fchk).
            ui.add_space(6.0);
            energy_unit_selector(ui, &mut view.energy_unit);
            let (factor, unit, d) = (view.energy_unit.factor(), view.energy_unit.name(), if view.energy_unit == EnergyUnit::KcalPerMol { 2 } else { 6 });
            let molecule = &molecule;
            let atom_names: Vec<String> = (0..molecule.atomic_numbers.len()).map(|a| atom_name(&molecule.atomic_numbers, a)).collect();
            let fragment_names: Vec<String> = results.fragments().iter().map(|f| fragment_name(f, &molecule.atomic_numbers)).collect();
            for (level, title, names) in [("fragments", "Fragment energies", &fragment_names), ("atoms", "Atomic energies", &atom_names)] {
                let Some(rows) = energy_rows(results, level, names) else { continue };
                let rows: Vec<_> = rows.into_iter().map(|(n, s, a)| (n, s.map(|v| v * factor), a.map(|v| v * factor))).collect();
                let title = format!("{title}, {unit}");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).small().weak());
                    if ui.small_button("Copy").on_hover_text("As tab-separated text, for a spreadsheet").clicked() {
                        let text: String = std::iter::once("\tSelf-energy\tAdditive energy\n".to_string())
                            .chain(rows.iter().map(|(n, s, a)| format!("{n}\t{}\t{}\n", s.map(|v| v.to_string()).unwrap_or_default(), a.map(|v| v.to_string()).unwrap_or_default())))
                            .collect();
                        ui.ctx().copy_text(text);
                    }
                });
                egui::ScrollArea::vertical().id_salt(("apost_enpart_energies", level)).max_height(180.0).show(ui, |ui| {
                    egui::Grid::new(("apost_enpart_energies_grid", level)).num_columns(3).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
                        ui.label("");
                        ui.label(RichText::new("Self-energy").strong());
                        ui.label(RichText::new("Additive energy").strong());
                        ui.end_row();
                        for (name, own, additive) in &rows {
                            ui.label(name);
                            for v in [own, additive] {
                                ui.label(RichText::new(v.map(|x| format!("{x:.d$}")).unwrap_or_else(|| "n/a".into())).monospace());
                            }
                            ui.end_row();
                        }
                    });
                });
            }
            ui.label(RichText::new("Additive energies are the self-energy plus half of each interaction; they add up to the total energy.").small().weak());
        });
    }
}
