//! Analysis orbitals (M6): the EFOs of EOS and GEOS and the OSLOs of OSLO,
//! read from the orbital file APOST-3D writes next to its results, labeled
//! from the tables, in the usual blue/red (by fragment on request); the key numbers and tables of
//! the APOST-3D window; sessions that keep such orbitals.

mod common;

use std::path::PathBuf;

use egui_kittest::kittest::{By, Queryable};

use common::{app_with, example, repo_dir, scratch_dir, screenshot, toasts};

fn orbital_fixtures() -> Option<PathBuf> {
    let dir = repo_dir().join("fixtures/apost3d-orbitals");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("fixtures/apost3d-orbitals/ not found; run scripts/sync-apost3d-fixtures.sh to enable this test");
        None
    }
}

/// Picks `option` in the Orbitals menu of the Surfaces window.
fn choose_orbitals(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, option: &str) {
    let combo = harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).find(|n| n.value().as_deref().is_some_and(|v| v.contains("orbitals") || v.contains("EFOs") || v.contains("OSLOs")));
    combo.expect("Orbitals menu").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label(option).click_accesskit();
    harness.run_steps(2);
}

#[test]
fn oslos_are_generated_with_their_fragments() {
    let dir = example("C2H6-OSLO-2CH3");
    let mut harness = app_with(vec![dir.join("C2H6-OSLO-2CH3.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose_orbitals(&mut harness, "OSLOs");
    // Only the OSLOs, each with its fragment(s) and FOLI.
    harness.get_by_label_contains("OSLO 1  1 CH3");
    harness.get_by_label_contains("OSLO 3  1 CH3");
    harness.get_by_label("Select all").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 9 orbital(s)").click_accesskit();
    harness.run_steps(3);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels.len(), 9, "{labels:?}");
    assert_eq!(labels[0], "OSLO 1 \u{b7} 1 CH3");
    assert_eq!(labels[8], "OSLO 9 \u{b7} 1 CH3 + 2 CH3");
    assert!(toasts(&harness).iter().any(|m| m == "9 OSLOs generated"), "{:?}", toasts(&harness));
    // The usual positive/negative colors; fragment colors on request.
    harness.get_by_label("Color by fragment");
    assert!(!harness.state().surface_colored_by_fragment(0));
    screenshot(&mut harness, "orbital-set-oslos");
}

#[test]
fn oslos_before_orthogonalization_are_a_set_of_their_own() {
    let dir = example("C2H6-OSLO-2CH3");
    let scratch = scratch_dir("orbital-set-preortho");
    let session = scratch.join("preortho.a3dview");
    {
        let mut harness = app_with(vec![dir.join("C2H6-OSLO-2CH3.apost.json")]);
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        // The final C-C OSLO, then the same one before orthogonalization:
        // both stay, told apart by name.
        harness.get_by_label("Tables (1)").click_accesskit();
        harness.run_steps(2);
        harness.get_all_by_label("Draw").nth(8).unwrap().click_accesskit();
        harness.run_steps(3);
        choose_orbitals(&mut harness, "OSLOs (before orthogonalization)");
        harness.get_by_label_contains("OSLOs (before orthogonalization) (9)");
        harness.get_by_label_contains("OSLO 3 before orth.  1 CH3");
        let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec!["OSLO 9 \u{b7} 1 CH3 + 2 CH3"]);
        // Tick OSLO 9 (the list shows the first eight without scrolling).
        harness.state_mut().select_set_orbitals(&[8], &[]);
        harness.run_steps(2);
        harness.get_by_label("Generate 1 orbital(s)").click_accesskit();
        harness.run_steps(3);
        let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec!["OSLO 9 \u{b7} 1 CH3 + 2 CH3", "OSLO 9 before orth. \u{b7} 1 CH3 + 2 CH3"]);
        assert!(toasts(&harness).iter().any(|m| m == "1 OSLO (before orthogonalization) generated"), "{:?}", toasts(&harness));
        screenshot(&mut harness, "orbital-set-preortho");
        harness.state_mut().save_session_to(&session).unwrap();
    }
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("orbital_set = \"oslo-preortho\""), "{text}");
    let harness = app_with(vec![session]);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["OSLO 9 \u{b7} 1 CH3 + 2 CH3", "OSLO 9 before orth. \u{b7} 1 CH3 + 2 CH3"]);
}

#[test]
fn a_table_row_draws_its_orbital() {
    let dir = example("C2H6-OSLO-2CH3");
    let mut harness = app_with(vec![dir.join("C2H6-OSLO-2CH3.apost.json")]);
    harness.get_by_label("Key numbers");
    harness.get_by_label("Sum of the FOLI values, total");
    harness.get_by_label("Tables (1)").click_accesskit();
    harness.run_steps(2);
    assert!(harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).any(|n| n.value().as_deref() == Some("OSLOs, total")));
    harness.get_by_label("Draw shows the orbital as a surface.");
    // The last row: the C-C bond, shared by the two methyls.
    harness.get_all_by_label("Draw").nth(8).expect("nine rows").click_accesskit();
    harness.run_steps(3);
    let labels = harness.state().surface_labels();
    assert_eq!(labels, vec![("OSLO 9 \u{b7} 1 CH3 + 2 CH3".to_string(), true)]);
    screenshot(&mut harness, "orbital-set-table-draw");
}

#[test]
fn geos_sets_name_paired_unpaired_and_negative_efos() {
    let Some(dir) = orbital_fixtures() else { return };
    let mut harness = app_with(vec![dir.join("NaBH3--B3LYP-GEOS.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose_orbitals(&mut harness, "GEOS EFOs");
    harness.get_by_label_contains("GEOS EFOs paired");
    harness.get_by_label_contains("GEOS EFOs unpaired");
    // The negative paired EFOs, in their own table, named after their
    // place in the orbital file.
    harness.get_by_label("Tables (5)").click_accesskit();
    harness.run_steps(2);
    harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).find(|n| n.value().as_deref().is_some_and(|v| v.contains("EFOs, paired"))).expect("Tables menu").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("GEOS negative paired EFOs (TFVC)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("EFO 164 paired");
    harness.get_by_label("EFO 165 paired");
    assert_eq!(harness.get_all_by_label("Draw").count(), 2);
}

/// Schema 0.15: Frontier ticks the frontier EFOs, the last occupied (LO)
/// and first unoccupied (FU) ones that set R (also marked LO and FU in the
/// list; those rows are scrolled out of sight here). CH3F
/// GEOS: one LO (F, paired) and one FU (CH3), no unpaired electrons.
#[test]
fn frontier_ticks_the_lo_and_fu_efos() {
    let dir = example("CH3F-GEOS");
    let mut harness = app_with(vec![dir.join("CH3F-GEOS.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose_orbitals(&mut harness, "GEOS EFOs");
    harness.get_by_label("Frontier").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
    harness.run_steps(4);
    let labels: Vec<String> = harness.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["EFO 9 paired \u{b7} 1 F", "EFO 10 paired \u{b7} 2 CH3"]);
}

/// Degenerate frontier EFOs all count: FeCO2's two CO fragments share the
/// LO occupation (rows 25 and 26), Fe has the FU (row 27).
#[test]
fn degenerate_frontier_efos_are_all_ticked() {
    let Some(dir) = orbital_fixtures() else { return };
    let mut harness = app_with(vec![dir.join("FeCO2-PBEPBE.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose_orbitals(&mut harness, "EOS EFOs");
    if harness.query_by_label("Frontier").is_none() {
        eprintln!("fixtures/apost3d-orbitals/ predate APOST-3D 0.15: no frontier EFOs");
        return;
    }
    harness.get_by_label("Frontier").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 3 orbital(s)");
}

#[test]
fn a_closed_shell_eos_set_says_beta_equals_alpha() {
    let Some(dir) = orbital_fixtures() else { return };
    let mut harness = app_with(vec![dir.join("FeCO2-PBEPBE.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose_orbitals(&mut harness, "EOS EFOs");
    harness.get_by_label_contains("same as the \u{3b1} ones");
    // An iron d EFO from its table row.
    harness.get_by_label_contains("Tables (").click_accesskit();
    harness.run_steps(4);
    let draws = harness.get_all_by_label("Draw").count();
    assert!(draws > 0);
    harness.get_all_by_label("Draw").nth(draws - 1).unwrap().click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.state().surface_labels().len(), 1);
    screenshot(&mut harness, "orbital-set-eos");
}

#[test]
fn an_orbital_file_of_another_molecule_is_refused() {
    let dir = example("C2H6-OSLO-2CH3");
    let scratch = scratch_dir("orbital-set-mismatch");
    for name in ["C2H6-OSLO-2CH3.fchk", "C2H6-OSLO-2CH3.apost.json"] {
        std::fs::copy(dir.join(name), scratch.join(name)).unwrap();
    }
    // The orbital file of another system, under the expected name.
    std::fs::copy(example("CH3F-GEOS").join("CH3F-GEOS-GEOS-EFOs.fchk"), scratch.join("C2H6-OSLO-2CH3-OSLOs.fchk")).unwrap();
    let mut harness = app_with(vec![scratch.join("C2H6-OSLO-2CH3.apost.json")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    choose_orbitals(&mut harness, "OSLOs");
    harness.get_by_label("Select all").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 9 orbital(s)").click_accesskit();
    harness.run_steps(3);
    assert!(harness.state().surface_labels().is_empty());
    assert!(toasts(&harness).iter().any(|m| m.contains("does not belong to the open .fchk")), "{:?}", toasts(&harness));
    assert!(harness.get_all_by_label_contains("does not belong to the open .fchk").next().is_some(), "the reason stays in the window");
}

#[test]
fn an_orbital_file_of_another_run_is_refused() {
    let dir = example("C2H6-OSLO-2CH3");
    let scratch = scratch_dir("orbital-set-stale");
    for name in ["C2H6-OSLO-2CH3.fchk", "C2H6-OSLO-2CH3.apost.json"] {
        std::fs::copy(dir.join(name), scratch.join(name)).unwrap();
    }
    // The same orbitals, as if left by an earlier run: another run id.
    let text = std::fs::read_to_string(dir.join("C2H6-OSLO-2CH3-OSLOs.fchk")).unwrap();
    let (first, rest) = text.split_once('\n').unwrap();
    let stale = format!("{}, run 2026-01-01T00:00:00+01:00/abcd\n{rest}", first.rsplit_once(", run ").unwrap().0);
    std::fs::write(scratch.join("C2H6-OSLO-2CH3-OSLOs.fchk"), stale).unwrap();
    let mut harness = app_with(vec![scratch.join("C2H6-OSLO-2CH3.apost.json")]);
    harness.get_by_label("Tables (1)").click_accesskit();
    harness.run_steps(2);
    harness.get_all_by_label("Draw").next().unwrap().click_accesskit();
    harness.run_steps(3);
    assert!(harness.state().surface_labels().is_empty());
    assert!(toasts(&harness).iter().any(|m| m.contains("another APOST-3D run")), "{:?}", toasts(&harness));
}

#[test]
fn effao_tables_are_listed_but_not_drawable() {
    let Some(dir) = common::fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("CH3F-EFFAO-U.apost.json")]);
    harness.get_by_label("Tables (2)").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("No orbital file for these orbitals.");
    assert_eq!(harness.query_all_by_label("Draw").count(), 0);
}

#[test]
fn sessions_keep_analysis_orbitals() {
    let dir = example("C2H6-OSLO-2CH3");
    let scratch = scratch_dir("orbital-set-session");
    let session = scratch.join("oslo.a3dview");
    {
        let mut harness = app_with(vec![dir.join("C2H6-OSLO-2CH3.apost.json")]);
        harness.get_by_label("Tables (1)").click_accesskit();
        harness.run_steps(2);
        harness.get_all_by_label("Draw").nth(8).unwrap().click_accesskit();
        harness.run_steps(3);
        harness.state_mut().save_session_to(&session).unwrap();
    }
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("orbital_set = \"oslo\""), "{text}");
    let harness = app_with(vec![session]);
    assert_eq!(harness.state().surface_labels(), vec![("OSLO 9 \u{b7} 1 CH3 + 2 CH3".to_string(), true)]);
}
