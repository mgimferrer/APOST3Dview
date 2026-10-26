//! The APOST-3D window's "New input": an input for a `.fchk` from a few
//! choices, fragments by hand, around a metal or per molecule, saved as
//! `<job>.inp` (or under another job name, with the `.fchk` linked), and
//! an existing input loaded back into the form.

mod common;

use std::path::{Path, PathBuf};

use common::{app_with, fixtures_dir, repo_dir, scratch_dir, screenshot, toasts};
use egui_kittest::kittest::{By, NodeT, Queryable};
use egui::accesskit::Role;
use egui_kittest::Harness;

use apost3dview::App;

/// A copy of fixture `job`.fchk in a fresh folder, as `name`.fchk.
fn copy_fchk(dir: &Path, job: &str, test: &str, name: &str) -> PathBuf {
    let scratch = scratch_dir(test);
    let path = scratch.join(format!("{name}.fchk"));
    std::fs::copy(dir.join(format!("{job}.fchk")), &path).unwrap();
    path
}

/// The app on `fchk`, with the APOST-3D window open (on "New input": the
/// `.fchk` has no results).
fn generator(fchk: &Path) -> Harness<'static, App> {
    let mut harness = app_with(vec![fchk.to_path_buf()]);
    harness.get_by_label("APOST-3D input...").click_accesskit();
    harness.run_steps(3);
    harness
}

fn click(harness: &mut Harness<'static, App>, label: &str) {
    harness.get_by_label(label).click_accesskit();
    harness.run_steps(3);
}

#[test]
fn eos_around_the_metal_saved_next_to_the_fchk() {
    let Some(dir) = fixtures_dir() else { return };
    let fchk = copy_fchk(&dir, "FeCN5NO3--UBLYP", "input-eos", "fe");
    let mut harness = generator(&fchk);
    harness.get_by_label("Input for fe.fchk");
    harness.get_by_label("Gaussian \u{b7} UBLYP \u{b7} unrestricted");

    click(&mut harness, "Effective oxidation states");
    harness.get_by_label("EOS, GEOS and OSLO need fragments.");
    harness.get_by_label("None yet: EOS, GEOS and OSLO need them.");
    assert!(harness.state().input_text().unwrap().contains("EOS"));

    // Click the iron, split around it: Fe, five CN, NO (the reference's
    // hand-made fragments).
    harness.state_mut().select_atoms(&[0]);
    harness.run_steps(2);
    click(&mut harness, "Around the selected atom(s)");
    assert_eq!(harness.state().input_fragments(), vec![vec![0], vec![1, 2], vec![3, 4], vec![5, 6], vec![7, 8], vec![9, 10], vec![11, 12]]);
    harness.get_by_label("complete");
    harness.remove_cursor();
    screenshot(&mut harness, "input-eos-around-fe");

    click(&mut harness, "Save fe.inp");
    let written = std::fs::read_to_string(fchk.with_extension("inp")).unwrap();
    assert_eq!(written, "# METHOD\nTFVC\nEOS\nDOFRAGS\n#\n# FRAGMENTS\n7\n1\n1\n2\n2 3\n2\n4 5\n2\n6 7\n2\n8 9\n2\n10 11\n-1\n#\n");
    assert!(toasts(&harness).contains(&"fe.inp saved".to_string()));

    // Opened again, the form starts from fe.inp; another analysis goes to
    // another job, its .fchk a link to fe.fchk.
    let mut harness = generator(&fchk);
    harness.get_by_label("Filled in from the existing fe.inp.");
    assert_eq!(harness.state().input_fragments().len(), 7);
    harness.get_by_label("Replace fe.inp");
    click(&mut harness, "Local spin (SPIN)");
    click(&mut harness, "Save as another job...");
    click(&mut harness, "Save fe-EOS.inp");
    let other = std::fs::read_to_string(fchk.with_file_name("fe-EOS.inp")).unwrap();
    assert!(other.starts_with("# METHOD\nTFVC\nEOS\nSPIN\nDOFRAGS\n#\n"), "{other}");
    assert_eq!(std::fs::read_to_string(fchk.with_extension("inp")).unwrap(), written, "fe.inp is kept");
    let linked = fchk.with_file_name("fe-EOS.fchk");
    assert_eq!(std::fs::metadata(&linked).unwrap().len(), std::fs::metadata(&fchk).unwrap().len());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&linked).unwrap().ino(), std::fs::metadata(&fchk).unwrap().ino(), "a hard link, not a copy");
    }
    assert!(toasts(&harness).iter().any(|t| t.contains("fe-EOS.inp saved, with fe-EOS.fchk (a link to fe.fchk")), "{:?}", toasts(&harness));

    // The next suggestion doesn't take a name in use.
    click(&mut harness, "Save as another job...");
    click(&mut harness, "Save fe-EOS-2.inp");
    assert!(fchk.with_file_name("fe-EOS-2.fchk").is_file());
}

#[test]
fn enpart_takes_the_functional_from_the_fchk() {
    let Some(dir) = fixtures_dir() else { return };
    let fchk = copy_fchk(&dir, "H2O-Dimer-B3LYP", "input-enpart", "dimer");
    let mut harness = generator(&fchk);
    click(&mut harness, "Energy decomposition, IQA (ENPART)");
    harness.get_by_label("From the .fchk (RB3LYP). It must be the functional of the calculation.");
    click(&mut harness, "Separate molecules");
    click(&mut harness, "Faster (40 \u{d7} 146)");
    harness.get_by_label("The faster two-electron grid is for tests and large systems: check the integration error in the output.");
    assert_eq!(
        harness.state().input_text().unwrap(),
        "# METHOD\nTFVC\nENPART\nDOFRAGS\n#\n# FRAGMENTS\n2\n3\n1 2 3\n-1\n#\n# ENPART\nB3LYP\n#\n# GRID\nRADIAL_2E 40\nANGULAR_2E 146\n#\n"
    );
    harness.remove_cursor();
    screenshot(&mut harness, "input-enpart-dimer");

    // A Hilbert-space scheme can't do ENPART: the problem is named and
    // Save waits.
    harness.get_all(By::new().role(Role::ComboBox)).next().unwrap().click_accesskit();
    harness.run_steps(2);
    click(&mut harness, "MULLIKEN  Mulliken");
    harness.get_by_label("ENPART needs a real-space atomic definition (TFVC), not MULLIKEN.");
    assert!(harness.query_by_label("complete").is_none());
}

#[test]
fn a_correlated_wavefunction_gets_its_rdm_files() {
    let Some(dir) = fixtures_dir() else { return };
    let fchk = copy_fchk(&dir, "H2O-CAS44", "input-cas", "cas");
    let mut harness = generator(&fchk);
    harness.get_by_label("pySCF \u{b7} CASSCF \u{b7} correlated");
    // DM 2 with the RDM files from the start, whatever the analyses
    // (APOST-3D's rule); it does the local spin analysis too, so SPIN is
    // shown ticked and can't be unticked.
    assert_eq!(harness.state().input_text().unwrap(), "# METHOD\nTFVC\nDM 2\n#\n# DM\ncas.dm1\ncas.dm2\npySCF\n#\n");
    assert!(harness.get_by_label("Local spin (SPIN)").accesskit_node().is_disabled());
    harness.get_by_label("cas.dm1 (written by APOST-3D's pySCF script with the .fchk) is not next to the .fchk.");
    click(&mut harness, "Energy decomposition, IQA (ENPART)");
    harness.get_by_label("Energy of the CASSCF wavefunction (also FCI and CISD from pySCF), from its RDMs.");
    assert!(harness.state().input_text().unwrap().contains("# ENPART\nCASSCF\n#\n"));
}

#[test]
fn an_existing_input_names_what_the_form_leaves_out() {
    let Some(dir) = fixtures_dir() else { return };
    let fchk = copy_fchk(&dir, "CH3F", "input-existing", "ch3f");
    std::fs::write(fchk.with_extension("inp"), "# METHOD\nTFVC\nOSLO\nDOFRAGS\nVERBOSE 1\n#\n# OSLO\nPRINT_NONORTHO\n#\n# FRAGMENTS\n2\n1\n2\n-1\n#\n").unwrap();
    let harness = generator(&fchk);
    harness.get_by_label("ch3f.inp also sets what this form doesn't handle; saving from here leaves it out: VERBOSE 1; # OSLO: PRINT_NONORTHO.");
    assert_eq!(harness.state().input_fragments(), vec![vec![1], vec![0, 2, 3, 4]]);
    assert_eq!(harness.state().input_text().unwrap(), "# METHOD\nTFVC\nOSLO\nDOFRAGS\n#\n# FRAGMENTS\n2\n1\n2\n-1\n#\n# OSLO\n#\n");
}

#[test]
fn alt_drag_selects_the_atoms_in_a_box() {
    let Some(dir) = fixtures_dir() else { return };
    let mut harness = app_with(vec![dir.join("FeCN5NO3--UBLYP.fchk")]);
    let drag = |harness: &mut Harness<'static, App>, from: egui::Pos2, to: egui::Pos2| {
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::ALT));
        harness.event(egui::Event::PointerMoved(from));
        harness.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::ALT });
        harness.run_steps(1);
        for step in 1..=8 {
            harness.event(egui::Event::PointerMoved(from + (to - from) * step as f32 / 8.0));
            harness.run_steps(1);
        }
        harness.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::ALT });
        harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        harness.run_steps(2);
    };
    // The left part of the view: some atoms, not all.
    drag(&mut harness, egui::pos2(8.0, 40.0), egui::pos2(500.0, 860.0));
    let some = harness.state().selected_atoms();
    assert!(!some.is_empty() && some.len() < 13, "{some:?}");
    // Then the whole view: every atom joins the selection.
    drag(&mut harness, egui::pos2(4.0, 30.0), egui::pos2(1396.0, 870.0));
    let mut all = harness.state().selected_atoms();
    assert!(all.starts_with(&some), "a box adds to the selection");
    all.sort_unstable();
    assert_eq!(all, (0..13).collect::<Vec<_>>());
}

/// An ORCA calculation converted by orca2fchk.py: recognized, HF taken from
/// the file, and the functional menu lists ORCA's functionals (by their
/// libxc ids), not Gaussian's keywords of the same names.
#[test]
fn an_orca_file_gets_orca_functionals() {
    let dir = scratch_dir("input-orca");
    let fchk = dir.join("water.fchk");
    std::fs::copy(repo_dir().join("validation/orca/orca2fchk/H2O-ORCA-ccpVTZ.fchk"), &fchk).unwrap();
    let mut harness = generator(&fchk);
    harness.get_by_label_contains("ORCA (orca2fchk.py)");
    click(&mut harness, "Energy decomposition, IQA (ENPART)");
    assert!(harness.state().input_text().unwrap().contains("# ENPART\nHF\n#"), "{}", harness.state().input_text().unwrap());
    harness.get_by_label_contains("ENPART uses ORCA's functional by its libxc ids");
    harness.get_all(By::new().role(Role::ComboBox)).find(|n| n.value().as_deref() == Some("HF")).expect("functional menu").click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_label("SVWN5").is_none(), "Gaussian's keywords are not offered");
    harness.get_by_label("B3LYP (ORCA)").click_accesskit();
    harness.run_steps(2);
    assert!(harness.state().input_text().unwrap().contains("# ENPART\nLIBRARY\nEXC_FUNCTIONAL 475\n#"), "{}", harness.state().input_text().unwrap());
}
