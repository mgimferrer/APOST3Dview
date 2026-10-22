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
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.apost.json")]);
    assert!(harness.state_mut().show_on_molecule("enpart.total.atoms"));
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-atomic interaction energy with O1 (TFVC), kcal/mol"));
    // O1's pair with H5 comes from the multipolar expansion.
    harness.get_by_label_contains("pair from the multipolar expansion (approximate exchange-correlation)");
    harness.get_by_label("Intra-atomic (self) energy of O1: \u{2212}46986.21 kcal/mol");
    // The run-quality section, with the final error in its heading.
    harness.get_by_label_contains("Energy decomposition (IQA) \u{b7} error");
    screenshot(&mut harness, "iqa-atoms-total");

    // Another term, the same atoms.
    combo_with_value(&harness, "Total (IQA)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Classical (electrostatic)").click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-atomic classical (electrostatic) interaction with O1 (TFVC), kcal/mol"));

    // Logarithmic colors.
    harness.get_by_label("Logarithmic colors").click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-atomic classical (electrostatic) interaction with O1 (TFVC), kcal/mol, log scale"));
    screenshot(&mut harness, "iqa-atoms-classical-log");
}

#[test]
fn iqa_between_fragments() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.apost.json")]);
    assert!(harness.state_mut().show_on_molecule("enpart.total.fragments"));
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-fragment interaction energy with 1 H2O (TFVC), kcal/mol"));
    combo_with_value(&harness, "Total (IQA)");
    // The two waters' interaction includes multipolar atom pairs.
    harness.get_by_label_contains("includes atom pairs from the multipolar expansion");
    screenshot(&mut harness, "iqa-fragments");
}

#[test]
fn a_correlated_wavefunction_splits_exchange_and_correlation() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-CISD.apost.json")]);
    assert!(harness.state_mut().show_on_molecule("enpart.xc.atoms"));
    harness.run_steps(3);
    combo_with_value(&harness, "Exchange-correlation").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Exchange (part of exchange-correlation)");
    harness.get_by_label("Correlation (part of exchange-correlation)").click_accesskit();
    harness.run_steps(3);
    assert!(harness.state().legend_title().unwrap().contains("with O1"));
    harness.get_by_label_contains("CASSCF");
}

#[test]
fn atomic_energies_and_the_selected_pair() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.apost.json")]);
    // Atomic energies as numbers, in the IQA section.
    harness.get_by_label_contains("Energy decomposition (IQA)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Atomic energies, kcal/mol");
    harness.get_by_label("Fragment energies, kcal/mol");
    // Selecting the first water's O and H shows their IQA decomposition.
    harness.state_mut().select_atoms(&[0, 4]);
    harness.run_steps(3);
    harness.get_by_label("Inter-atomic interaction energy (IQA), kcal/mol");
    harness.get_by_label("Total (IQA)");
    harness.get_by_label_contains("comes from the multipolar expansion (approximate)");
    screenshot(&mut harness, "iqa-additive-pair");
}

#[test]
fn stopped_and_unreferenced_runs() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-WB97X.apost.json")]);
    harness.get_by_label_contains("Energy decomposition (IQA)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("The run stopped during the energy decomposition: no terms were written.");
    let mut harness = app_with(vec![dir.join("H2O-Dimer-RHF-NOREF.apost.json")]);
    harness.get_by_label_contains("Energy decomposition (IQA)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label_contains("Not possible: the .fchk has no reference energy components");
    // A run in an external field: its energy is listed.
    let mut harness = app_with(vec![dir.join("H2O-RHF-FIELD.apost.json")]);
    harness.get_by_label_contains("Energy decomposition (IQA)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("External field energy");
}

#[test]
fn energies_in_kcal_per_mol_or_au() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.apost.json")]);
    assert!(harness.state_mut().show_on_molecule("enpart.total.fragments"));
    harness.run_steps(3);
    // kcal/mol by default: the waters' interaction, -0.033217 au.
    harness.get_by_label("1 H2O - 2 H2O   -20.84");
    harness.get_all_by_label("au").next().unwrap().click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-fragment interaction energy with 1 H2O (TFVC), au"));
    harness.get_by_label("1 H2O - 2 H2O   -0.033217");
    // Fixed ranges are kept in au whatever the unit shown.
    harness.get_by_label("Fixed").click_accesskit();
    harness.run_steps(2);
    let fixed_au = harness.state().legend_bounds();
    harness.get_all_by_label("kcal/mol").next().unwrap().click_accesskit();
    harness.run_steps(3);
    let (lo, _) = harness.state().legend_bounds().unwrap();
    assert_eq!(fixed_au.unwrap().0, "\u{2212}0.0332");
    assert_eq!(lo, "\u{2212}20.8437");
}

/// Opens the Show menu and picks `label` in it.
fn show(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, label: &str) {
    // (Opened only if it isn't still open from the last pick.)
    if harness.query_all_by_label(label).count() == 0 {
        harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).next().unwrap().click_accesskit();
        harness.run_steps(2);
    }
    harness.get_all_by_label(label).last().unwrap().click_accesskit();
    harness.run_steps(3);
}

/// The Show menu offers a matrix's diagonal as a value on each atom or
/// fragment (intra: self-energies, local spin) and its pairs seen from one
/// of them (inter).
#[test]
fn a_matrix_diagonal_on_every_atom_or_fragment() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-Dimer-B3LYP.apost.json")]);
    show(&mut harness, "Intra-fragment (self) energy (TFVC)");
    assert_eq!(harness.state().legend_title().as_deref(), Some("Intra-fragment (self) energy (TFVC), kcal/mol"));
    assert!(harness.query_by_label("Seen from").is_none(), "no focus for the diagonal");
    show(&mut harness, "Inter-fragment interaction energy (TFVC)");
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-fragment interaction energy with 1 H2O (TFVC), kcal/mol"));
    harness.get_by_label("Seen from");

    // Atomic and fragment S^2 (the diagonal, local spin), CISD run.
    let mut harness = app_with(vec![dir.join("H2O-CISD.apost.json")]);
    show(&mut harness, "Intra-atomic S\u{b2} (local spin) (TFVC)");
    assert_eq!(harness.state().legend_title().as_deref(), Some("Intra-atomic S\u{b2} (local spin) (TFVC)"));
    screenshot(&mut harness, "local-spin");
    show(&mut harness, "Intra-fragment S\u{b2} (local spin) (TFVC)");
    assert_eq!(harness.state().legend_title().as_deref(), Some("Intra-fragment S\u{b2} (local spin) (TFVC)"));
    show(&mut harness, "Inter-fragment S\u{b2} (spin coupling) (TFVC)");
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-fragment S\u{b2} (spin coupling) with 1 O (TFVC)"));
}

/// Every choice in the Show menu says intra or inter and is unique: where
/// two matrices give a part the same name, the source matrix is named.
#[test]
fn show_menu_names_every_part_once() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("H2O-CISD.apost.json")]);
    harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).next().unwrap().click_accesskit();
    harness.run_steps(3);
    for label in [
        "Intra-fragment localization index (exchange-only bond orders, TFVC)",
        "Intra-fragment localization index (improved bond orders, TFVC)",
        "Intra-fragment localization index (exchange-correlation bond orders, DI, TFVC)",
        "Intra-atomic localization index (exchange-only bond orders, TFVC)",
        "Intra-atomic localization index (improved bond orders, TFVC)",
        "Intra-atomic localization index (exchange-correlation bond orders, DI, TFVC)",
        "Inter-fragment bond order (improved bond orders, TFVC)",
        "Inter-atomic bond order (exchange-only bond orders, TFVC)",
        "Inter-atomic bond order (improved bond orders, TFVC)",
        "Inter-atomic delocalization index (TFVC)",
        "Intra-atomic S\u{b2} (local spin) (TFVC)",
        "Inter-atomic S\u{b2} (spin coupling) (TFVC)",
        "Intra-atomic (self) energy (TFVC)",
        "Inter-atomic interaction energy (TFVC)",
    ] {
        assert_eq!(harness.query_all_by_label(label).count(), 1, "{label}");
    }
    assert_eq!(harness.query_all_by_label("Intra-fragment localization index (TFVC)").count(), 0, "ambiguous name");
}
