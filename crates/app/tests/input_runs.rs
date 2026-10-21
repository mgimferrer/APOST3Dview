//! Inputs made with the generator, run through APOST-3D itself, and their
//! results opened in the viewer. Needs an APOST-3D checkout with the
//! program built: `APOST3D_DIR=path/to/APOST3D` (its `apost3d` binary and
//! `tests/inputs/`); without it the test prints a notice and passes.
//! Takes a minute or two (two ENPART runs on the faster grid).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use apost3dview_core::{check_against_fchk, load_apost_results, parse_fchk_summary};
use common::{app_with, scratch_dir};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use apost3dview::App;

fn apost3d_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APOST3D_DIR")?);
    if dir.join("apost3d").is_file() && dir.join("tests/inputs").is_dir() {
        Some(dir)
    } else {
        eprintln!("APOST3D_DIR has no built apost3d and tests/inputs/");
        None
    }
}

fn click(harness: &mut Harness<'static, App>, label: &str) {
    harness.get_by_label(label).click_accesskit();
    harness.run_steps(3);
}

/// One run: `job` copied from APOST-3D's test inputs (with `extra`
/// files renamed to the new job), the generator set up by `choose`, the
/// input saved, APOST-3D run, its results checked and opened.
fn run(apost3d: &Path, job: &str, extra: &[&str], choose: impl FnOnce(&mut Harness<'static, App>)) {
    let scratch = scratch_dir(&format!("run-{job}"));
    let name = "gen";
    let fchk = scratch.join(format!("{name}.fchk"));
    std::fs::copy(apost3d.join(format!("tests/inputs/{job}.fchk")), &fchk).unwrap();
    for ext in extra {
        std::fs::copy(apost3d.join(format!("tests/inputs/{job}.{ext}")), scratch.join(format!("{name}.{ext}"))).unwrap();
    }
    let mut harness = app_with(vec![fchk.clone()]);
    click(&mut harness, "APOST-3D input...");
    choose(&mut harness);
    harness.get_by_label("complete");
    click(&mut harness, &format!("Save {name}.inp"));
    let fragments = harness.state().input_fragments();
    drop(harness);

    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("ulimit -s unlimited 2>/dev/null; OMP_NUM_THREADS=4 '{}' {name} > {name}.apost 2>&1", apost3d.join("apost3d").display()))
        .current_dir(&scratch)
        .status()
        .unwrap();
    let report = std::fs::read_to_string(scratch.join(format!("{name}.apost"))).unwrap_or_default();
    assert!(status.success() && report.contains("Normal Termination"), "{job}: APOST-3D did not end normally:\n{}", report.lines().rev().take(15).collect::<Vec<_>>().join("\n"));

    let json = scratch.join(format!("{name}.apost.json"));
    let results = load_apost_results(&json).unwrap();
    check_against_fchk(&results, &parse_fchk_summary(&fchk).unwrap()).unwrap();
    let written: Vec<Vec<usize>> = results.fragments().iter().map(|f| f.atoms.iter().map(|a| a - 1).collect()).collect();
    assert_eq!(written, fragments, "{job}: APOST-3D's fragments");

    let mut harness = app_with(vec![json]);
    harness.run_steps(3);
    assert!(harness.query_by_label("Results not loaded").is_none(), "{job}: results refused");
    println!("{job}: ran, analyses {:?}, {} fragments", results.analysis_names(), written.len());
}

#[test]
fn generated_inputs_run_in_apost3d() {
    let Some(apost3d) = apost3d_dir() else {
        eprintln!("APOST3D_DIR not set: generated inputs not run through APOST-3D");
        return;
    };
    run(&apost3d, "FeCN5NO3--UBLYP", &[], |h| {
        click(h, "Effective oxidation states");
        h.state_mut().select_atoms(&[0]);
        h.run_steps(2);
        click(h, "Around the selected atom(s)");
    });
    run(&apost3d, "CH3F", &[], |h| {
        click(h, "Oxidation states from localized orbitals (OSLO)");
        h.state_mut().select_atoms(&[1]);
        h.run_steps(2);
        click(h, "Around the selected atom(s)");
    });
    run(&apost3d, "NaBH3--UHF", &[], |h| {
        click(h, "Effective oxidation states");
        click(h, "GEOS");
        click(h, "Local spin (SPIN)");
        h.state_mut().select_atoms(&[0]);
        h.run_steps(2);
        click(h, "Around the selected atom(s)");
    });
    run(&apost3d, "H2O-Dimer-B3LYP", &[], |h| {
        click(h, "Energy decomposition, IQA (ENPART)");
        click(h, "Separate molecules");
        click(h, "Faster (40 \u{d7} 146)");
    });
    run(&apost3d, "H2O-CAS44", &["dm1", "dm2"], |h| {
        click(h, "Local spin (SPIN)");
        click(h, "Energy decomposition, IQA (ENPART)");
        click(h, "Faster (40 \u{d7} 146)");
    });
}
