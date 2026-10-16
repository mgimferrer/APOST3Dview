//! The energy decomposition (ENPART, IQA): the IQA view with its term
//! selector, between atoms and between fragments, the pairs from the
//! multipolar expansion marked, the atomic energies as numbers, the
//! selected pair's decomposition, and the run-quality section.

mod common;

use egui_kittest::kittest::{By, Queryable};

use common::{app_with, fixtures_dir, screenshot};

fn combo_with_value<'a>(harness: &'a egui_kittest::Harness<'_, apost3dview::App>, value: &str) -> egui_kittest::Node<'a> {
    harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).find(|n| n.value().as_deref() == Some(value)).unwrap_or_else(|| panic!("no menu showing {value}"))
}

#[test]
fn iqa_between_atoms_with_its_terms() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.fchk")]);
    assert!(harness.state_mut().show_on_molecule("enpart.total.atoms"));
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Interaction energy with O1 (TFVC), kcal/mol"));
    // O1's pair with H5 comes from the multipolar expansion.
    harness.get_by_label_contains("pair from the multipolar expansion (approximate exchange-correlation)");
    harness.get_by_label("Self-energy of O1: \u{2212}46986.21 kcal/mol");
    // The run-quality section, with the final error in its heading.
    harness.get_by_label_contains("Energy decomposition (IQA) \u{b7} error");
    screenshot(&mut harness, "iqa-atoms-total");

    // Another term, the same atoms.
    combo_with_value(&harness, "Total (IQA)").click();
    harness.run_steps(2);
    harness.get_by_label("Classical (electrostatic)").click();
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Classical interaction energy with O1 (TFVC), kcal/mol"));

    // Logarithmic colors.
    harness.get_by_label("Logarithmic colors").click();
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Classical interaction energy with O1 (TFVC), kcal/mol, log scale"));
    screenshot(&mut harness, "iqa-atoms-classical-log");
}

#[test]
fn iqa_between_fragments() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.fchk")]);
    assert!(harness.state_mut().show_on_molecule("enpart.total.fragments"));
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Interaction energy with 1 H2O (TFVC), kcal/mol"));
    combo_with_value(&harness, "Total (IQA)");
    // The two waters' interaction includes multipolar atom pairs.
    harness.get_by_label_contains("includes atom pairs from the multipolar expansion");
    screenshot(&mut harness, "iqa-fragments");
}

#[test]
fn a_correlated_wavefunction_splits_exchange_and_correlation() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-CISD.fchk")]);
    assert!(harness.state_mut().show_on_molecule("enpart.xc.atoms"));
    harness.run_steps(3);
    combo_with_value(&harness, "Exchange-correlation").click();
    harness.run_steps(2);
    harness.get_by_label("Exchange (part of exchange-correlation)");
    harness.get_by_label("Correlation (part of exchange-correlation)").click();
    harness.run_steps(3);
    assert!(harness.state().legend_title().unwrap().contains("with O1"));
    harness.get_by_label_contains("CASSCF");
}

#[test]
fn atomic_energies_and_the_selected_pair() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.fchk")]);
    // Atomic energies as numbers, in the IQA section.
    harness.get_by_label_contains("Energy decomposition (IQA)").click();
    harness.run_steps(2);
    harness.get_by_label("Atomic energies, kcal/mol");
    harness.get_by_label("Fragment energies, kcal/mol");
    // Selecting the first water's O and H shows their IQA decomposition.
    harness.state_mut().select_atoms(&[0, 4]);
    harness.run_steps(3);
    harness.get_by_label("Interaction energy (IQA), kcal/mol");
    harness.get_by_label("Total (IQA)");
    harness.get_by_label_contains("comes from the multipolar expansion (approximate)");
    screenshot(&mut harness, "iqa-additive-pair");
}

#[test]
fn stopped_and_unreferenced_runs() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-TPSS.fchk")]);
    harness.get_by_label_contains("Energy decomposition (IQA)").click();
    harness.run_steps(2);
    harness.get_by_label("The run stopped during the energy decomposition: no terms were written.");
    let mut harness = app_with(vec![dir.join("H2O-Dimer-RHF-NOREF.fchk")]);
    harness.get_by_label_contains("Energy decomposition (IQA)").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Not possible: the .fchk has no reference energy components");
    // A run in an external field: its energy is listed.
    let mut harness = app_with(vec![dir.join("H2O-RHF-FIELD.fchk")]);
    harness.get_by_label_contains("Energy decomposition (IQA)").click();
    harness.run_steps(2);
    harness.get_by_label("External field energy");
}

#[test]
fn energies_in_kcal_per_mol_or_au() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.fchk")]);
    assert!(harness.state_mut().show_on_molecule("enpart.total.fragments"));
    harness.run_steps(3);
    // kcal/mol by default: the waters' interaction, -0.033217 au.
    harness.get_by_label("1 H2O \u{2013} 2 H2O   -20.84");
    harness.get_all_by_label("au").next().unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Interaction energy with 1 H2O (TFVC), au"));
    harness.get_by_label("1 H2O \u{2013} 2 H2O   -0.033217");
    // Fixed ranges are kept in au whatever the unit shown.
    harness.get_by_label("Fixed").click();
    harness.run_steps(2);
    let fixed_au = harness.state().legend_bounds();
    harness.get_all_by_label("kcal/mol").next().unwrap().click();
    harness.run_steps(3);
    let (lo, _) = harness.state().legend_bounds().unwrap();
    assert_eq!(fixed_au.unwrap().0, "\u{2212}0.0332");
    assert_eq!(lo, "\u{2212}20.8437");
}
