//! ORCA `.molden` files in the app (2026-10-21): opened, orbitals and the
//! density generated; CASSCF natural orbitals (fractional occupations)
//! offer no spin density. The parser itself is checked against ORCA's own
//! cubes in `crates/core/tests/reference_cubes.rs`.

mod common;

use egui_kittest::kittest::Queryable;

use common::{app_with, repo_dir, toasts};

fn orca(name: &str) -> Option<std::path::PathBuf> {
    let path = repo_dir().join("validation/orca").join(name);
    path.is_file().then_some(path).or_else(|| {
        eprintln!("{name} not found; test skipped");
        None
    })
}

#[test]
fn orca_casscf_molden_gives_orbitals_and_density_but_no_spin_density() {
    let Some(path) = orca("H2O-ORCA-CAS44-def2SVP.molden") else { return };
    let mut harness = app_with(vec![path]);
    assert!(toasts(&harness).iter().all(|t| !t.contains("unavailable") && !t.contains("Could not")), "{:?}", toasts(&harness));
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    // Natural orbitals: occupations are spin-summed, no spin density.
    assert_eq!(harness.query_all_by_label("Spin density").count(), 0);
    harness.get_by_label("HOMO/LUMO").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Total density").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 3 surface(s)").click_accesskit();
    harness.run_steps(4);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["HOMO", "LUMO", "Total density"]);
    assert!(toasts(&harness).iter().any(|t| t == "2 orbitals and the total density generated"), "{:?}", toasts(&harness));
}

#[test]
fn orca_open_shell_and_ecp_moldens_open() {
    for (name, spin) in [("O2-ORCA-T-ccpVTZ.molden", true), ("BiCl3-ORCA-def2TZVPP.molden", false)] {
        let Some(path) = orca(name) else { return };
        let mut harness = app_with(vec![path]);
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        assert_eq!(harness.query_all_by_label("Spin density").count(), usize::from(spin), "{name}");
        harness.get_by_label("HOMO/LUMO").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label_contains("orbital(s)").click_accesskit();
        harness.run_steps(4);
        assert!(!harness.state().surface_labels().is_empty(), "{name}: {:?}", toasts(&harness));
        assert!(toasts(&harness).iter().all(|t| !t.contains("could not")), "{name}: {:?}", toasts(&harness));
    }
}
