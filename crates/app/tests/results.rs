//! Showing APOST-3D values on the molecule (M3-M5): the choice of what to
//! show, fragment and atom values, the focus view for pair quantities, the
//! tables, the two-atom pair values, fixed color ranges shared between
//! structures, bond thickness from bond orders, and legends in exports.

mod common;

use egui_kittest::kittest::Queryable;

use common::{app_with, fixtures_dir, screenshot};

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p != q).count()
}

#[test]
fn fragments_are_shown_by_default_and_values_on_request() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    harness.get_by_label("On the molecule");
    assert_eq!(harness.state().legend_title(), None, "fragments in their own colors need no legend");

    assert!(harness.state_mut().show_on_molecule("populations.fragments.spin"));
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Fragment spin populations (Lowdin)"));
    // The table, at the file's precision: the unpaired electron on NO.
    harness.get_by_label("+0.693235");
    harness.get_by_label("Fragment spin populations (Lowdin)");
    assert!(!harness.state_mut().show_on_molecule("no.such.quantity"));
    screenshot(&mut harness, "results-fragment-spin");
}

#[test]
fn a_pair_quantity_is_seen_from_one_fragment() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    assert!(harness.state_mut().show_on_molecule("bond_orders.fragments.matrix"));
    harness.run_steps(3);
    assert!(harness.state().legend_title().unwrap().contains("with 1 Fe"));
    harness.get_by_label("Diagonal: localization index. Off-diagonal: bond order.");
    harness.get_by_label_contains("(diagonal and one triangle)");
    // The strongest pair first.
    harness.get_by_label_contains("1 Fe \u{2013} 7 NO   1.852845");
    // In the focus view, a fragment in the list is a place to look from.
    harness.get_by_label("7  NO   N12, O13").click_accesskit();
    harness.run_steps(3);
    assert!(harness.state().legend_title().unwrap().contains("with 7 NO"), "{:?} selected {:?}", harness.state().legend_title(), harness.state().selected_atoms());
    assert!(harness.state().selected_atoms().is_empty(), "the click changed the focus, not the selection");
    screenshot(&mut harness, "results-fragment-focus");
}

#[test]
fn atom_values_and_the_selected_pair() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    assert!(harness.state_mut().show_on_molecule("populations.atoms.charges"));
    harness.run_steps(3);
    assert_eq!(harness.state().legend_title().as_deref(), Some("Atomic charges (Lowdin)"));
    // Selecting the NO fragment selects two atoms: their pair values appear.
    harness.get_by_label("7  NO   N12, O13").click_accesskit();
    harness.run_steps(3);
    harness.get_by_label_contains("Selected pair N12 \u{2013} O13");
    harness.get_by_label("Bond orders (Lowdin)");
    screenshot(&mut harness, "results-atom-charges");
}

#[test]
fn a_fixed_range_is_shared_by_every_structure() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("NaBH3--UHF.fchk"), dir.join("NaBH3--UHF-DAVIDSON.fchk")]);
    // The second structure is active: show charges and fix the range.
    assert!(harness.state_mut().show_on_molecule("populations.fragments.charges"));
    harness.run_steps(2);
    let automatic_second = harness.state().legend_bounds();
    harness.get_by_label("Fixed").click();
    harness.run_steps(2);
    assert_eq!(harness.state().legend_bounds(), automatic_second, "fixing keeps the range it had");
    // The first structure, a different run: same quantity id, same range.
    harness.get_all_by_label("NaBH3--UHF.fchk").next().unwrap().click();
    harness.run_steps(2);
    assert!(harness.state_mut().show_on_molecule("populations.fragments.charges"));
    harness.run_steps(2);
    assert_eq!(harness.state().legend_bounds(), automatic_second);
}

#[test]
fn bond_thickness_and_legends_reach_the_exported_image() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    assert!(harness.state_mut().show_on_molecule("nothing"));
    harness.run_steps(3);
    let plain = harness.state_mut().render_image(600, 450).unwrap();

    harness.get_by_label("Bond thickness from bond orders").click();
    harness.run_steps(3);
    let thick = harness.state_mut().render_image(600, 450).unwrap();
    assert!(differing(&plain, &thick) > 500, "bonds change thickness");
    harness.get_by_label("Bond thickness from bond orders").click();
    harness.run_steps(3);
    assert_eq!(differing(&plain, &harness.state_mut().render_image(600, 450).unwrap()), 0, "and come back equal");

    // A legend (bottom left) is drawn into exports with a quantity shown.
    assert!(harness.state_mut().show_on_molecule("populations.atoms.charges"));
    harness.run_steps(3);
    let with_values = harness.state_mut().render_image(600, 450).unwrap();
    let corner = |pixels: &[u8]| -> Vec<u8> {
        (360..450).flat_map(|y| (0..250).flat_map(move |x| { let i = (y * 600 + x) * 4; [i, i + 1, i + 2] })).map(|i| pixels[i]).collect()
    };
    assert!(differing(&corner(&plain), &corner(&with_values)) > 300, "legend in the corner");
}

/// The Show menu lists the run's own atomic definition and the quantities
/// worth drawing; the Mulliken values APOST-3D prints for comparison, and
/// the secondary ones (electron counts, valences, ...), only on request.
#[test]
fn other_values_are_listed_only_on_request() {
    use egui_kittest::kittest::By;
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).next().unwrap().click();
    harness.run_steps(2);
    for listed in ["Atomic charges (Lowdin)", "Spin populations (Lowdin)", "Fragment charges (Lowdin)", "Bond orders (Lowdin)", "EOS oxidation states (Lowdin)"] {
        assert!(harness.query_by_label(listed).is_some(), "{listed} listed");
    }
    let other = ["Atomic charges (Mulliken)", "Electron populations (Lowdin)", "Total valences (Lowdin)", "Fragment free valences (Lowdin)", "EOS electrons, alpha (Lowdin)"];
    for hidden in other {
        assert!(harness.query_by_label(hidden).is_none(), "{hidden} not listed");
    }
    harness.get_by_label("Also list other values").click_accesskit();
    harness.run_steps(2);
    for shown in other {
        assert!(harness.query_by_label(shown).is_some(), "{shown} listed on request");
    }
}
