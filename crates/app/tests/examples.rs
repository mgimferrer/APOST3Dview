//! The committed examples (`examples/`): each opens from its results file
//! with nothing to warn about, and shows what the user guide's five-minute
//! tour (`docs/src/tour.md`, followed step by step by `tests/guide.rs`)
//! tells a new user to look at.

mod common;

use egui_kittest::kittest::{By, Queryable};

use common::{app_with, example, toasts};

const JOBS: [&str; 5] = ["FeCN5NO3--UBLYP", "H2O-Dimer-B3LYP", "C2H6-OSLO-2CH3", "CH3F-GEOS", "H2O-CAS44"];

fn open(job: &str) -> egui_kittest::Harness<'static, apost3dview::App> {
    let mut harness = app_with(vec![example(job).join(format!("{job}.apost.json"))]);
    harness.run_steps(2);
    assert_eq!(toasts(&harness), vec![format!("APOST-3D results loaded from {job}.apost.json")], "{job}");
    harness
}

/// Opens the menu currently showing `value`, checks it offers every one of
/// `options` (the names the guide uses), and closes it again.
fn menu_offers(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, value: &str, options: &[&str]) {
    harness
        .get_all(By::new().role(egui::accesskit::Role::ComboBox))
        .find(|n| n.value().as_deref() == Some(value))
        .unwrap_or_else(|| panic!("no menu showing {value}"))
        .click_accesskit();
    harness.run_steps(2);
    for option in options {
        assert!(harness.query_all_by_label(option).next().is_some(), "no {option} in the menu showing {value}");
    }
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
}

/// Picks `option` in the menu currently showing `value`.
fn choose(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, value: &str, option: &str) {
    harness
        .get_all(By::new().role(egui::accesskit::Role::ComboBox))
        .find(|n| n.value().as_deref() == Some(value))
        .unwrap_or_else(|| panic!("no menu showing {value}"))
        .click_accesskit();
    harness.run_steps(2);
    harness.query_all_by_label(option).last().unwrap_or_else(|| panic!("no {option} in the menu showing {value}")).click_accesskit();
    harness.run_steps(3);
}

#[test]
fn every_example_is_a_complete_run_folder() {
    for job in JOBS {
        let dir = example(job);
        for extension in ["inp", "fchk", "apost", "apost.json"] {
            assert!(dir.join(format!("{job}.{extension}")).is_file(), "{job}.{extension}");
        }
    }
}

#[test]
fn fragments_charges_and_bond_orders() {
    let mut harness = open("FeCN5NO3--UBLYP");
    menu_offers(&mut harness, "Nothing", &["Fragments", "Fragment charges (Lowdin)", "EOS oxidation states (Lowdin)", "Inter-fragment bond order (Lowdin)", "Atomic charges (Lowdin)"]);
    choose(&mut harness, "Nothing", "Inter-fragment bond order (Lowdin)");
    assert!(harness.state().legend_title().unwrap().contains("with 1 Fe"));
    harness.get_by_label("Bond thickness from bond orders");
    // The EFO file isn't in the folder: the tables say so.
    harness.get_by_label("Key numbers");
    harness.get_by_label_contains("Tables (").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Their orbital file, FeCN5NO3--UBLYP-EOS-EFOs.fchk, isn't next to the results: they can't be drawn.");
    assert_eq!(harness.query_all_by_label("Draw").count(), 0);
}

#[test]
fn energy_decomposition() {
    let mut harness = open("H2O-Dimer-B3LYP");
    menu_offers(&mut harness, "Nothing", &["Inter-atomic interaction energy (TFVC)", "Inter-fragment interaction energy (TFVC)"]);
    choose(&mut harness, "Nothing", "Inter-atomic interaction energy (TFVC)");
    assert_eq!(harness.state().legend_title().as_deref(), Some("Inter-atomic interaction energy with O1 (TFVC), kcal/mol"));
    harness.get_by_label_contains("pair from the multipolar expansion");
    harness.get_by_label("Logarithmic colors");
    menu_offers(&mut harness, "Total (IQA)", &["Classical (electrostatic)"]);
    harness.get_by_label_contains("Energy decomposition (IQA) \u{b7} error");
}

#[test]
fn the_shared_oslo_in_two_colors() {
    let mut harness = open("C2H6-OSLO-2CH3");
    harness.get_by_label("Tables (1)").click_accesskit();
    harness.run_steps(2);
    harness.get_all_by_label("Draw").nth(8).expect("nine OSLOs").click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.state().surface_labels(), vec![("OSLO 9 \u{b7} 1 CH3 + 2 CH3".to_string(), true)]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Color by fragment");
    menu_offers(&mut harness, "OSLOs", &["OSLOs (before orthogonalization)"]);
}

#[test]
fn geos_efos() {
    let mut harness = open("CH3F-GEOS");
    menu_offers(&mut harness, "Nothing", &["GEOS oxidation states (TFVC)"]);
    harness.get_by_label("Key numbers");
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose(&mut harness, "SCF orbitals", "GEOS EFOs");
    harness.get_by_label("Select all").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label_contains("orbital(s)").click_accesskit();
    harness.run_steps(4);
    let surfaces = harness.state().surface_labels();
    assert!(!surfaces.is_empty() && surfaces.iter().all(|(label, _)| label.starts_with("EFO ")), "{surfaces:?}");
    harness.get_by_label("Color by fragment");
}

#[test]
fn correlated_density_and_natural_orbitals() {
    let mut harness = open("H2O-CAS44");
    menu_offers(&mut harness, "Nothing", &["Intra-atomic S\u{b2} (local spin) (TFVC)", "Inter-atomic bond order (exchange-only bond orders, TFVC)", "Inter-atomic bond order (improved bond orders, TFVC)"]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Clear").click_accesskit();
    harness.get_by_label("Total density").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 1 surface(s)").click_accesskit();
    harness.run_steps(4);
    choose(&mut harness, "Orbitals of the file", "Natural orbitals (CASSCF)");
    harness.get_by_label("Frontier");
    harness.get_by_label("Fractional");
    harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
    harness.run_steps(4);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["Total density (CASSCF)", "NO 5 (CASSCF)", "NO 6 (CASSCF)"]);
}
