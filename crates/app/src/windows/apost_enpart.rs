//! The APOST-3D window's energy decomposition (ENPART, IQA) section: how
//! the decomposition was done (wavefunction, functional), how well its
//! energy adds up to the calculation's (the integration errors, including
//! the two-electron integration and its zero-error correction), and which
//! atom pairs come from the multipolar expansion. The terms themselves are
//! shown on the molecule (On the molecule, Energy decomposition).

use apost3dview_core::{element_data, Enpart, Molecule};
use apost3dview_render::format_signed;
use egui::{Color32, RichText};

use crate::app::App;

/// A final integration error above this (kcal/mol, chemical accuracy) is
/// pointed out.
const NOTABLE_ERROR_KCAL: f64 = 1.0;
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

fn pair_name(molecule: &Molecule, [a, b]: [usize; 2]) -> String {
    let name = |i: usize| molecule.atomic_numbers.get(i - 1).map(|&z| format!("{}{i}", element_data(z).symbol)).unwrap_or_else(|| i.to_string());
    format!("{}\u{2013}{}", name(a), name(b))
}

impl App {
    pub(crate) fn show_enpart_quality(&self, ui: &mut egui::Ui) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return };
        let Some(Ok(results)) = structure.apost.as_ref().map(|a| &a.results) else { return };
        let Some(enpart) = results.enpart() else { return };
        let error = enpart.integration_error_kcal;
        let notable = error.is_some_and(|e| e.abs() > NOTABLE_ERROR_KCAL);
        let mut header = RichText::new(match error {
            Some(e) => format!("Energy decomposition (IQA) \u{b7} error {}", kcal(e)),
            None => "Energy decomposition (IQA)".to_string(),
        })
        .strong();
        if notable {
            header = header.color(NOTICE);
        }
        egui::CollapsingHeader::new(header).id_salt("apost_enpart").show(ui, |ui| {
            if enpart.total_energy.is_none() {
                ui.label(RichText::new("The run stopped during the energy decomposition: no terms were written.").color(NOTICE));
            }
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
                if let Some(e) = enpart.reference_energy {
                    row(ui, "Energy of the .fchk", RichText::new(energy(e)).monospace());
                }
                if let Some(e) = error {
                    let mut text = RichText::new(kcal(e)).monospace();
                    if notable {
                        text = text.color(NOTICE);
                    }
                    row(ui, "Integration error", text);
                }
                if let Some(e) = enpart.electron_nuclear_error_kcal {
                    row(ui, "  electron-nuclear", RichText::new(kcal(e)).monospace());
                }
                if let Some(e) = enpart.kinetic_error_kcal {
                    row(ui, "  kinetic", RichText::new(kcal(e)).monospace());
                }
                if let Some(t) = &enpart.two_electron_integration {
                    if let Some(e) = t.error_kcal {
                        let tolerance = t.tolerance_kcal.map(|x| format!(" (tolerance {x})")).unwrap_or_default();
                        row(ui, "  two-electron, first grid", RichText::new(format!("{}{tolerance}", kcal(e))).monospace());
                    }
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
                        Some("no reference energy") => Some("not possible (the .fchk has no reference energies)".to_string()),
                        Some(other) => Some(other.to_string()),
                        None => None,
                    };
                    if let Some(c) = correction {
                        row(ui, "  zero-error correction", RichText::new(c));
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
            if !enpart.multipolar_pairs.is_empty() {
                let names: Vec<String> = enpart.multipolar_pairs.iter().map(|&p| pair_name(&structure.molecule, p)).collect();
                ui.label(RichText::new(format!("Exchange-correlation from the multipolar expansion: {}.", names.join(", "))).small().weak());
            }
            let in_field = results.input.as_ref().and_then(|i| i.text.as_deref()).is_some_and(|t| t.lines().any(|l| l.trim() == "POLAR"));
            if in_field {
                ui.label(RichText::new("Run in an external field (POLAR): the field terms are not written, so the total energy above leaves them out.").small().weak());
            }
            if notable {
                ui.label(RichText::new("The decomposition's energy differs from the calculation's by more than 1 kcal/mol: a finer integration grid may be needed.").small().color(NOTICE));
            }
        });
    }
}
