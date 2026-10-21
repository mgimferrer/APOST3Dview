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
            assert!(entry.signed.is_some(), "{name}: {} has no signed flag (0.4)", entry.id);
            assert_eq!(entry.path, format!("/analyses/{}", entry.id.replace('.', "/")), "{name}: id and path disagree");
            match results.quantity(entry).unwrap() {
                Quantity::Scalar(_) => {}
                Quantity::Table(table) => {
                    assert!(table.rows == "fragment" || table.rows == "orbital", "{name}: {} rows {}", entry.id, table.rows);
                    assert!(entry.decimals.is_none(), "{name}: a table's decimals are per column");
                }
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
    assert_eq!(m.off_diagonal.as_deref(), Some("bond order"), "a short noun since 0.4");
    assert!((m.get(0, 6).unwrap() - 1.853).abs() < 1e-3);
    let charges = iron.index.iter().find(|e| e.id == "populations.fragments.charges").unwrap();
    let Quantity::Vector(q) = iron.quantity(charges).unwrap() else { panic!() };
    assert_eq!(q.len(), 7);

    // A correlated local spin analysis brings its own pair matrices (the
    // DI, `li_di` before 0.14), and the 1-RDM Mayer's improved bond orders.
    let lih = load(&dir, "LiH-35-CAS22");
    assert!(lih.index.iter().any(|e| e.id == "local_spin.atoms.xc_bond_orders"));
    assert!(lih.index.iter().any(|e| e.id == "bond_orders.atoms.improved_bond_orders"));
    // Overlap populations total all elements, unlike bond orders.
    let feco2 = load(&dir, "FeCO2-PBEPBE");
    let entry = feco2.index.iter().find(|e| e.id == "bond_orders.overlap_populations.fragments.matrix").unwrap();
    let Quantity::Matrix(m) = feco2.quantity(entry).unwrap() else { panic!() };
    assert_eq!(m.total_rule.as_deref(), Some("all elements"));
}

fn orbital_fixtures_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/apost3d-orbitals");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("fixtures/apost3d-orbitals/ not found; run scripts/sync-apost3d-fixtures.sh to enable this test");
        None
    }
}

/// Schema 0.5-0.7 orbital files: each labeled orbital exists in the file,
/// the file belongs to the main .fchk, and brief 04's particular cases.
#[test]
fn orbital_sets_map_onto_their_files() {
    use apost3dview_core::{check_orbital_file, check_orbital_file_id, orbital_file_run_id, parse_fchk_wavefunction, OrbitalDetail};
    let Some(dir) = orbital_fixtures_dir() else { return };
    let (mut cases, mut preortho_cases) = (0, 0);
    for json in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().ends_with(".apost.json")) {
        let name = json.file_name().unwrap().to_string_lossy().into_owned();
        let results = load_apost_results(&json).unwrap();
        assert!(results.index_problems.is_empty(), "{name}: {:?}", results.index_problems);
        let main = parse_fchk_summary(&wavefunction_path_for(&json, &results).unwrap()).unwrap();
        let sets = results.orbital_sets();
        // One set per analysis, plus the OSLOs before orthogonalization
        // when the run wrote them (0.9), labeled like the final ones.
        let preortho = sets.iter().filter(|s| s.preortho).count();
        assert_eq!(sets.len(), 1 + preortho, "{name}");
        if let [last, before] = &sets[..] {
            assert!(before.preortho && before.analysis == "oslo" && before.file.ends_with("-OSLOs-preortho.fchk"), "{name}: {}", before.file);
            assert_eq!((&last.alpha, &last.beta, &last.file_id), (&before.alpha, &before.beta, &before.file_id), "{name}");
            preortho_cases += 1;
        }
        for set in &sets {
            let file = dir.join(&set.file);
            check_orbital_file(&main, &parse_fchk_summary(&file).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
            // The file carries the run id the results expect (0.8).
            let run_id = results.run.as_ref().and_then(|r| r.id.clone());
            assert!(run_id.is_some(), "{name}: no run.id");
            assert_eq!(set.file_id, run_id, "{name}: orbital_file_id is not run.id");
            let found = orbital_file_run_id(&file).unwrap();
            check_orbital_file_id(set.file_id.as_deref(), found.as_deref()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(check_orbital_file_id(set.file_id.as_deref(), Some("2026-01-01T00:00:00+00:00/0000")).is_err());
            let wfn = parse_fchk_wavefunction(&file).unwrap();
            assert!(set.alpha.len() <= wfn.alpha.num_orbitals(), "{name}: alpha labels past the file");
            assert!(set.alpha.iter().flatten().count() > 0, "{name}: no alpha labels");
            match &wfn.beta {
                Some(beta) => assert!(set.beta.len() <= beta.num_orbitals(), "{name}: beta labels past the file"),
                None => assert!(set.beta.iter().flatten().count() == 0, "{name}: beta labels but no beta orbitals"),
            }
            // A labeled EFO's stored "energy" is its gross occupation; an
            // OSLO's, its FOLI value.
            for (k, label) in set.alpha.iter().enumerate() {
                let Some(label) = label else { continue };
                let stored = wfn.alpha.orbital_energies[k];
                let expected = match &label.detail {
                    OrbitalDetail::Efo { gross, net, .. } => gross.or(*net),
                    OrbitalDetail::Oslo { foli, .. } => *foli,
                };
                if let (Some(expected), false) = (expected, matches!(label.detail, OrbitalDetail::Efo { negative: true, .. })) {
                    assert!((stored - expected).abs() < 1e-6, "{name}: alpha {} stores {stored}, table {expected}", k + 1);
                }
            }
        }
        cases += 1;
    }
    assert_eq!((cases, preortho_cases), (9, 2));

    let set = |job: &str| load(&dir, job).orbital_sets().remove(0);
    // OSLO, restricted, the C-C bond shared by both CH3 fragments.
    let c2h6 = set("C2H6-OSLO-2CH3");
    let shared = c2h6.alpha[8].as_ref().unwrap();
    assert_eq!(shared.fragments, vec![1, 2]);
    assert!((shared.shares[0] - 0.5).abs() < 1e-9);
    assert!(c2h6.beta.is_empty());
    // GEOS: two negative EFOs, alpha orbitals 164 and 165.
    let geos = set("NaBH3--B3LYP-GEOS");
    assert!(matches!(geos.alpha[163].as_ref().unwrap().detail, OrbitalDetail::Efo { negative: true, .. }));
    assert!(matches!(geos.alpha[164].as_ref().unwrap().detail, OrbitalDetail::Efo { negative: true, .. }));
    // Their fit: `fit_percent` since schema 0.13 (`recovered_percent` before).
    for (index, percent) in [(163, 74.0), (164, 81.0)] {
        let OrbitalDetail::Efo { fit_percent: Some(fit), .. } = geos.alpha[index].as_ref().unwrap().detail else { panic!("no fit for EFO {}", index + 1) };
        assert!((fit - percent).abs() < 0.5, "EFO {}: fit {fit}", index + 1);
    }
    // A file before 0.13 named the negative EFOs' column `recovered_percent`.
    let text = std::fs::read_to_string(dir.join("NaBH3--B3LYP-GEOS.apost.json")).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["schema_version"] = "0.12".into();
    let negative = &mut value["analyses"]["geos"]["negative_efos"];
    for column in negative["columns"].as_array_mut().unwrap() {
        if column["key"] == "fit_percent" {
            column["key"] = "recovered_percent".into();
        }
    }
    let data = negative["data"].as_object_mut().unwrap();
    let fits = data.remove("fit_percent").unwrap();
    data.insert("recovered_percent".to_string(), fits);
    let old_file = std::env::temp_dir().join(format!("apost3dview-geos-0.12-{}.apost.json", std::process::id()));
    std::fs::write(&old_file, serde_json::to_string(&value).unwrap()).unwrap();
    let old = load_apost_results(&old_file).unwrap();
    std::fs::remove_file(&old_file).ok();
    let old_geos = old.orbital_sets().remove(0);
    let OrbitalDetail::Efo { fit_percent: Some(fit), .. } = old_geos.alpha[163].as_ref().unwrap().detail else { panic!("no fit from recovered_percent") };
    assert!((fit - 74.0).abs() < 0.5, "{fit}");
    // EOS on a closed shell: alpha only.
    assert!(set("FeCO2-PBEPBE").beta.is_empty());
    // EOS, fractional assignment.
    assert!(matches!(set("H2-S-EOS").alpha[0].as_ref().unwrap().detail, OrbitalDetail::Efo { electrons: Some(e), .. } if (e - 0.5).abs() < 1e-9));
    // GEOS on a restricted FCI file: a beta set for the unpaired EFOs.
    assert!(set("LiH-32-FCI").beta.iter().flatten().count() > 0);
    // OSLO, unrestricted, 7 fragments: 55 + 54 orbitals.
    let iron = set("FeCN5NO3--UBLYP-t2");
    assert_eq!((iron.alpha.iter().flatten().count(), iron.beta.iter().flatten().count()), (55, 54));
}

/// Schema 0.8: every orbital table gives `fragments` and `shares` lists
/// (no single `fragment` column); `fchk_orbitals` exactly where there is a
/// `fchk_column`; EFFAO tables have neither.
#[test]
fn orbital_tables_share_one_form() {
    use apost3dview_core::{Quantity, QuantityKind};
    let dirs: Vec<PathBuf> = [fixtures_dir(), orbital_fixtures_dir()].into_iter().flatten().collect();
    if dirs.is_empty() {
        return;
    }
    let (mut tables, mut effao) = (0, 0);
    for dir in dirs {
        for json in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().ends_with(".apost.json")) {
            let name = json.file_name().unwrap().to_string_lossy().into_owned();
            let results = load_apost_results(&json).unwrap();
            for entry in results.entries(QuantityKind::Table) {
                let Ok(Quantity::Table(table)) = results.quantity(entry) else { panic!("{name}: {}", entry.id) };
                if table.rows != "orbital" {
                    continue;
                }
                tables += 1;
                assert!(table.has_column("fragments") && table.has_column("shares") && !table.has_column("fragment"), "{name}: {}", entry.id);
                for row in 0..table.n_rows {
                    let (fragments, shares) = (table.numbers("fragments", row).unwrap(), table.numbers("shares", row).unwrap());
                    assert_eq!(fragments.len(), shares.len(), "{name}: {} row {row}", entry.id);
                    assert!((shares.iter().sum::<f64>() - 1.0).abs() < 1e-6, "{name}: {} row {row} shares {shares:?}", entry.id);
                }
                assert_eq!(table.has_column("fchk_column"), table.fchk_orbitals.is_some(), "{name}: {}", entry.id);
                if entry.analysis.as_deref() == Some("effao") {
                    effao += 1;
                    assert!(table.fchk_orbitals.is_none(), "{name}: {}", entry.id);
                }
            }
        }
    }
    assert!(tables > 0 && effao > 0, "{tables} orbital tables, {effao} EFFAO");
}

/// ENPART (0.10, 0.11): every term matrix adds up by its rule, the IQA
/// matrix to the total energy, the additive energies too; the classical
/// interaction is total minus xc off the diagonal; exchange + correlation
/// = xc when split; multipolar pairs name real atoms.
#[test]
fn enpart_blocks_add_up() {
    use apost3dview_core::{Quantity, ENPART_TERMS};
    let Some(dir) = fixtures_dir() else { return };
    let (mut cases, mut fields) = (0, 0);
    for json in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().ends_with(".apost.json")) {
        let name = json.file_name().unwrap().to_string_lossy().into_owned();
        let results = load_apost_results(&json).unwrap();
        let Some(enpart) = results.enpart() else { continue };
        cases += 1;
        let Some(total_energy) = enpart.total_energy else {
            assert!(results.stopped(), "{name}: no total energy in a finished run");
            continue;
        };
        let matrix = |id: &str| match results.index.iter().find(|e| e.id == id).map(|e| results.quantity(e)) {
            Some(Ok(Quantity::Matrix(m))) => Some(m),
            _ => None,
        };
        let vector = |id: &str| match results.index.iter().find(|e| e.id == id).map(|e| results.quantity(e)) {
            Some(Ok(Quantity::Vector(v))) => Some(v.into_iter().map(|x| x.unwrap()).collect::<Vec<f64>>()),
            _ => None,
        };
        for term in &enpart.terms {
            assert!(ENPART_TERMS.contains(&term.as_str()));
            let m = matrix(&format!("enpart.{term}.atoms")).unwrap_or_else(|| panic!("{name}: {term}"));
            if let (Some(total), Some(by_rule)) = (m.total, m.total_by_rule()) {
                assert!((total - by_rule).abs() < 1e-8, "{name}: {term} total {total} vs {by_rule}");
            }
        }
        let total = matrix("enpart.total.atoms").unwrap();
        assert!((total.total_by_rule().unwrap() - total_energy).abs() < 1e-8, "{name}");
        let additive: f64 = vector("enpart.atomic_energies.atoms.additive_energy").unwrap().iter().sum();
        assert!((additive - total_energy).abs() < 1e-7, "{name}: additive {additive} vs {total_energy}");
        let (xc, classical) = (matrix("enpart.xc.atoms").unwrap(), matrix("enpart.classical_interaction.atoms").unwrap());
        let n = total.values.len();
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let expected = total.get(i, j).unwrap() - xc.get(i, j).unwrap();
                    assert!((classical.get(i, j).unwrap() - expected).abs() < 1e-7, "{name}: classical {i} {j}");
                }
            }
        }
        if let (Some(x), Some(c)) = (matrix("enpart.exchange.atoms"), matrix("enpart.correlation.atoms")) {
            for i in 0..n {
                for j in 0..n {
                    assert!((x.get(i, j).unwrap() + c.get(i, j).unwrap() - xc.get(i, j).unwrap()).abs() < 1e-8, "{name}: exchange + correlation");
                }
            }
        }
        for &[a, b] in &enpart.multipolar_pairs {
            assert!(a >= 1 && b >= 1 && a <= n && b <= n && a != b, "{name}: pair {a} {b}");
            assert!(enpart.is_multipolar(a - 1, b - 1) && enpart.is_multipolar(b - 1, a - 1));
        }
        // integration_error = total + field - reference (0.12), in every run.
        if let (Some(r), Some(e)) = (enpart.reference_energy, enpart.integration_error) {
            let field = enpart.field_energy.unwrap_or(0.0);
            assert!((total_energy + field - r - e).abs() < 1e-8, "{name}: {total_energy} + {field} - {r} != {e}");
            fields += usize::from(enpart.field_energy.is_some());
        }
    }
    assert!(cases >= 11, "{cases} ENPART cases");
    assert_eq!(fields, 2, "the two field runs");
}
