//! Loads every APOST-3D reference results file in `fixtures/apost3d/`
//! (copied there by `scripts/sync-apost3d-fixtures.sh`) and checks it
//! against the `.fchk` it was computed from. A change of the results-file
//! schema on the APOST-3D side that breaks the viewer fails here.
//!
//! The fixtures are not tracked by git. Without them these tests print a
//! notice and pass, so a fresh checkout still builds and tests cleanly.

use std::path::{Path, PathBuf};

use apost3dview_core::{
    check_against_fchk, find_results_for, fragment_coverage, load_apost_results, parse_fchk_summary, parse_fchk_wavefunction, report_path_for,
    wavefunction_path_for, ApostResults,
};

fn fixtures_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/apost3d");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("fixtures/apost3d/ not found; run scripts/sync-apost3d-fixtures.sh to enable these tests");
        None
    }
}

fn load(dir: &Path, job: &str) -> ApostResults {
    load_apost_results(&dir.join(format!("{job}.apost.json"))).unwrap_or_else(|err| panic!("{job}: {err}"))
}

#[test]
fn every_fixture_loads_and_matches_its_fchk() {
    let Some(dir) = fixtures_dir() else { return };
    let mut jobs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".apost.json"))
        .collect();
    jobs.sort();
    assert!(!jobs.is_empty(), "no .apost.json files in {}", dir.display());

    for json in &jobs {
        let name = json.file_name().unwrap().to_string_lossy().into_owned();
        let results = load_apost_results(json).unwrap_or_else(|err| panic!("{name}: {err}"));

        let fchk = wavefunction_path_for(json, &results).unwrap_or_else(|| panic!("{name}: its .fchk was not found"));
        assert_eq!(find_results_for(&fchk).as_deref(), Some(json.as_path()), "{name}: discovery from the .fchk");
        let summary = parse_fchk_summary(&fchk).unwrap_or_else(|err| panic!("{name}: {err}"));
        check_against_fchk(&results, &summary).unwrap_or_else(|err| panic!("{name}: {err}"));

        assert!(report_path_for(json, &results).is_some(), "{name}: no .apost report found");
        if let Some(molecule) = results.molecule() {
            assert_eq!(molecule.atomic_numbers.len(), summary.atomic_numbers.len());
            let coverage = fragment_coverage(molecule.atomic_numbers.len(), results.fragments());
            if !results.fragments().is_empty() {
                assert!(coverage.shared.is_empty(), "{name}: atoms in two fragments: {:?}", coverage.shared);
            }
        }
    }
    eprintln!("checked {} APOST-3D results files", jobs.len());
}

#[test]
fn particular_cases() {
    let Some(dir) = fixtures_dir() else { return };

    // Schema 0.2: run date and version release date.
    let dimer = load(&dir, "H2O-Dimer-RHF");
    assert!(dimer.run.as_ref().unwrap().date.as_deref().is_some_and(|d| d.len() >= 19 && &d[10..11] == "T"));
    assert!(dimer.program.as_ref().unwrap().version_date.is_some());

    let stopped = load(&dir, "H2O-TPSS");
    assert!(stopped.stopped());
    assert_eq!(stopped.run.as_ref().unwrap().stop_message.as_deref(), Some("META-GGA FUNCTIONALS NOT YET SUPPORTED"));

    // Q-Chem .fchk: no energy on either side.
    assert_eq!(load(&dir, "FeO4-2").wavefunction.unwrap().energy_au, None);
    assert_eq!(parse_fchk_summary(&dir.join("FeO4-2.fchk")).unwrap().scf_energy, None);
    // Q-Chem writes the orbital energies after the MO coefficients
    // (Gaussian: before); the wavefunction must still be read.
    let qchem = parse_fchk_wavefunction(&dir.join("FeO4-2.fchk")).unwrap();
    assert_eq!(qchem.alpha.num_basis_functions, 169);
    assert_eq!(qchem.alpha.orbital_energies.len(), qchem.alpha.num_orbitals());
    assert_eq!(qchem.alpha.homo_index(), 30);

    // ECP: nuclear charge 25 for iodine (Z = 53).
    let ecp = load(&dir, "HI-ECP-pySCF").molecule.unwrap();
    assert_eq!(ecp.atomic_numbers, vec![1, 53]);
    assert_eq!(ecp.nuclear_charges[1], Some(25.0));

    let iron = load(&dir, "FeCN5NO3--UBLYP");
    assert_eq!(iron.fragments().len(), 7);
    assert_eq!(iron.wavefunction.as_ref().unwrap().formalism.as_deref(), Some("unrestricted"));
    assert_eq!(fragment_coverage(13, iron.fragments()), Default::default());

    assert_eq!(load(&dir, "C2H6-B3LYP").fragments().len(), 0);
}

/// Results must not attach to a different calculation on the same
/// geometry: the three water dimers share geometry and basis set, and
/// differ only in the method (so in the energy).
#[test]
fn results_do_not_attach_to_another_calculation() {
    let Some(dir) = fixtures_dir() else { return };
    let rhf = load(&dir, "H2O-Dimer-RHF");
    let b3lyp = parse_fchk_summary(&dir.join("H2O-Dimer-B3LYP.fchk")).unwrap();
    assert!(check_against_fchk(&rhf, &b3lyp).unwrap_err().contains("energy"));
    let methyl_fluoride = parse_fchk_summary(&dir.join("CH3F.fchk")).unwrap();
    assert!(check_against_fchk(&rhf, &methyl_fluoride).unwrap_err().contains("atoms"));
}

/// Schema 0.3: every index entry of every fixture resolves to values of
/// the right shape; symmetric matrices are symmetric; totals follow their
/// stated rule; ids are the paths below `analyses` joined with dots.
#[test]
fn every_index_entry_resolves() {
    use apost3dview_core::{Quantity, QuantityKind};
    let Some(dir) = fixtures_dir() else { return };
    let mut entries = 0;
    for path in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().ends_with(".apost.json")) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let results = load_apost_results(&path).unwrap();
        assert!(results.index_problems.is_empty(), "{name}: {:?}", results.index_problems);
        for entry in &results.index {
            entries += 1;
            assert_ne!(entry.kind, QuantityKind::Other, "{name}: {}", entry.id);
            assert_eq!(entry.path, format!("/analyses/{}", entry.id.replace('.', "/")), "{name}: id and path disagree");
            match results.quantity(entry).unwrap() {
                Quantity::Vector(values) => assert!(values.iter().all(Option::is_some), "{name}: {} has gaps", entry.id),
                Quantity::Matrix(m) => {
                    if m.symmetric {
                        for i in 0..m.values.len() {
                            for j in 0..i {
                                let (a, b) = (m.get(i, j).unwrap(), m.get(j, i).unwrap());
                                assert!((a - b).abs() <= 1e-9 * (1.0 + a.abs()), "{name}: {} not symmetric at {i},{j}", entry.id);
                            }
                        }
                    }
                    if let (Some(total), Some(computed)) = (m.total, m.total_by_rule()) {
                        assert!((total - computed).abs() <= 1e-6 * (1.0 + total.abs()), "{name}: {} total {total} vs {computed}", entry.id);
                    }
                    if entry.kind == QuantityKind::FragmentMatrix {
                        assert!(m.total.is_some() && m.total_rule.is_some(), "{name}: fragment matrices carry their total");
                    }
                }
            }
        }
    }
    assert!(entries > 400, "only {entries} entries");
    eprintln!("{entries} index entries resolved");
}

#[test]
fn schema_0_3_particular_cases() {
    use apost3dview_core::Quantity;
    let Some(dir) = fixtures_dir() else { return };

    // Stopped while reading the input: an empty index, no molecule.
    let stopped = load(&dir, "H2-TWO-AIM");
    assert!(stopped.stopped() && stopped.index.is_empty() && stopped.molecule.is_none());

    // Fragment values are read from the file (Fe-NO bond order 1.853).
    let iron = load(&dir, "FeCN5NO3--UBLYP");
    let entry = iron.index.iter().find(|e| e.id == "bond_orders.fragments.matrix").unwrap();
    let Quantity::Matrix(m) = iron.quantity(entry).unwrap() else { panic!() };
    assert_eq!(m.off_diagonal.as_deref(), Some("bond order between the two fragments"));
    assert!((m.get(0, 6).unwrap() - 1.853).abs() < 1e-3);
    let charges = iron.index.iter().find(|e| e.id == "populations.fragments.charges").unwrap();
    let Quantity::Vector(q) = iron.quantity(charges).unwrap() else { panic!() };
    assert_eq!(q.len(), 7);

    // A correlated local spin analysis brings its own pair matrices.
    let lih = load(&dir, "LiH-35-CAS22");
    assert!(lih.index.iter().any(|e| e.id == "local_spin.atoms.li_di"));
    // Overlap populations total all elements, unlike bond orders.
    let feco2 = load(&dir, "FeCO2-PBEPBE");
    let entry = feco2.index.iter().find(|e| e.id == "bond_orders.overlap_populations.fragments.matrix").unwrap();
    let Quantity::Matrix(m) = feco2.quantity(entry).unwrap() else { panic!() };
    assert_eq!(m.total_rule.as_deref(), Some("all elements"));
}
