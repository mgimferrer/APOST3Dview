//! The input generator's reader and writer against APOST-3D's own reference
//! inputs and results (`fixtures/apost3d/`, copied by
//! `scripts/sync-apost3d-fixtures.sh`; without them the tests print a
//! notice and pass).
//!
//! 1. Every results file holds its input text and APOST-3D's reading of
//!    it (atomic definition, fragments with `-1` expanded, analyses run):
//!    the viewer's reader must read every input the same way.
//! 2. Every reference input inside the generator's scope is rebuilt from
//!    its `.fchk` plus only the choices a user makes in the generator, and
//!    must come out the same input (by meaning, not text).
//! 3. The automatic fragments reproduce the hand-made ones.

use std::path::{Path, PathBuf};

use apost3dview_core::{
    fragments_around, load_apost_results, read_apost_input, read_fchk_facts, separate_molecules, ApostInput, DmFiles, FchkProgram, EnpartOptions, Molecule, OxidationStates,
};

fn fixtures_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/apost3d");
    if dir.join("CH3F.inp").is_file() {
        Some(dir)
    } else {
        eprintln!("fixtures/apost3d/ (with the reference inputs) not found; run scripts/sync-apost3d-fixtures.sh to enable these tests");
        None
    }
}

fn files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().ends_with(suffix)).collect();
    files.sort();
    files
}

fn job(path: &Path, suffix: &str) -> String {
    path.file_name().unwrap().to_string_lossy().trim_end_matches(suffix).to_string()
}

#[test]
fn reads_every_input_as_apost3d_did() {
    let Some(dir) = fixtures_dir() else { return };
    let mut checked = 0;
    for path in files(&dir, ".apost.json") {
        let job = job(&path, ".apost.json");
        let results = load_apost_results(&path).unwrap();
        let (Some(text), Some(molecule)) = (results.input.as_ref().and_then(|i| i.text.clone()), results.molecule.as_ref()) else { continue };
        let read = read_apost_input(&text, molecule.n_atoms);
        let input = read.input.normalized();

        let keyword = results.input.as_ref().and_then(|i| i.atomic_definition.as_ref()).and_then(|d| d.keyword.clone());
        assert_eq!(Some(input.aim.keyword().to_string()), keyword, "{job}: atomic definition");

        let fragments: Vec<Vec<usize>> = results.fragments().iter().map(|f| f.atoms.clone()).collect();
        assert_eq!(input.fragments.clone().unwrap_or_default(), fragments, "{job}: fragments");

        if !results.stopped() {
            let analyses = results.analysis_names();
            let ran = |name: &str| analyses.contains(&name);
            assert_eq!(input.oxidation == Some(OxidationStates::Eos), ran("eos"), "{job}: EOS");
            assert_eq!(input.oxidation == Some(OxidationStates::Geos), ran("geos"), "{job}: GEOS");
            assert_eq!(input.oslo, ran("oslo"), "{job}: OSLO");
            assert_eq!(input.enpart.is_some(), ran("enpart"), "{job}: ENPART");
            assert_eq!(input.spin, ran("local_spin"), "{job}: SPIN");
        }
        checked += 1;
    }
    println!("{checked} inputs read as APOST-3D read them");
    assert!(checked >= 50, "only {checked} results files with an input");
}

/// The choices a user makes in the generator, taken from a reference input;
/// everything the `.fchk` decides is left to `apply_fchk`.
fn user_choices(reference: &ApostInput) -> ApostInput {
    ApostInput {
        aim: reference.aim,
        oxidation: reference.oxidation,
        oslo: reference.oslo,
        spin: reference.spin,
        enpart: reference.enpart.as_ref().map(|e| EnpartOptions { method: None, fast_grid: e.fast_grid }),
        fragments: reference.fragments.clone(),
        dm: None,
        qchem: false,
    }
}

/// Reference inputs that leave out on purpose what the generator always
/// adds, to test APOST-3D without it.
const DELIBERATELY_INCOMPLETE: [(&str, &str); 1] =
    [("CH2-T-CAS66-NODM", "open-shell CASSCF without its RDMs (APOST-3D then leaves out the spin populations); the generator adds DM 2")];

#[test]
fn rebuilds_the_reference_inputs_from_their_fchk() {
    let Some(dir) = fixtures_dir() else { return };
    let (mut rebuilt, mut outside, mut functional_chosen, mut dm_added) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for path in files(&dir, ".inp") {
        let job = job(&path, ".inp");
        let fchk = dir.join(format!("{job}.fchk"));
        let molecule = Molecule::from_fchk(&fchk).unwrap();
        let n_atoms = molecule.atomic_numbers.len();
        let text = std::fs::read_to_string(&path).unwrap();
        let reference = read_apost_input(&text, n_atoms);

        // Round trip: what the reader handles, the writer writes back.
        let written = read_apost_input(&reference.input.to_text(n_atoms), n_atoms);
        assert!(written.other.is_empty(), "{job}: {:?}", written.other);
        assert_eq!(written.input.normalized(), reference.input.normalized(), "{job}: round trip");

        if let Some((_, why)) = DELIBERATELY_INCOMPLETE.iter().find(|(j, _)| *j == job) {
            outside.push(format!("{job} ({why})"));
            continue;
        }
        if !reference.other.is_empty() {
            outside.push(format!("{job} ({})", reference.other.join("; ")));
            continue;
        }
        let facts = read_fchk_facts(&fchk).unwrap();
        let mut input = user_choices(&reference.input);
        input.apply_fchk(&facts, &job);
        if let (Some(enpart), Some(wanted)) = (&mut input.enpart, &reference.input.enpart)
            && enpart.method.is_none()
        {
            // pySCF's "RKS"/"UKS" don't name the functional: the user picks it.
            enpart.method = wanted.method.clone();
            functional_chosen.push(job.clone());
        }
        // A correlated pySCF file always gets DM 2 with its RDM files
        // (APOST-3D, 2026-10-21), also where a reference input left it out.
        let mut expected = reference.input.clone();
        if facts.correlated && expected.dm.is_none() {
            let program = if facts.program == FchkProgram::Orca { "ORCA" } else { "pySCF" };
            expected.dm = Some(DmFiles { dm1: format!("{job}.dm1"), dm2: format!("{job}.dm2"), program: program.to_string() });
            dm_added.push(job.clone());
        }
        assert_eq!(input.normalized(), expected.normalized(), "{job}: rebuilt from {job}.fchk");
        rebuilt.push(job);
    }
    println!("rebuilt from their .fchk ({}): {}", rebuilt.len(), rebuilt.join(", "));
    if !functional_chosen.is_empty() {
        println!("functional picked by the user (pySCF): {}", functional_chosen.join(", "));
    }
    if !dm_added.is_empty() {
        println!("DM 2 added to the reference (correlated pySCF file): {}", dm_added.join(", "));
    }
    println!("outside the generator ({}):\n  {}", outside.len(), outside.join("\n  "));
    assert!(rebuilt.len() >= 25, "only {} inputs rebuilt", rebuilt.len());
    for case in ["FeCO2-PBEPBE", "FeO4-2", "H2O-Dimer-B3LYP", "HI-ECP-G16", "LiH-32-FCI-LINDEP", "CH2-T-CAS66", "NaBH3--UHF"] {
        assert!(rebuilt.contains(&case.to_string()), "{case} not rebuilt");
    }
}

/// Bonds as the viewer perceives them (all drawn).
fn bonds(molecule: &Molecule) -> Vec<(usize, usize)> {
    molecule.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect()
}

#[test]
fn automatic_fragments_reproduce_the_hand_made_ones() {
    let Some(dir) = fixtures_dir() else { return };
    // (input, centers clicked, 0-based); no centers: separate molecules.
    let cases: [(&str, &[usize]); 9] = [
        ("FeCN5NO3--UBLYP", &[0]),
        ("FeCO2-PBEPBE", &[0]),
        ("FeO4-2", &[0]),
        ("CH3F", &[1]),
        ("NaBH3--UHF", &[0]),
        ("NaBH3--B3LYP-GEOS", &[4]),
        ("LiH-32-FCI", &[0]),
        ("H2O-Dimer-B3LYP", &[]),
        ("H2O-Dimer-RHF", &[]),
    ];
    for (job, centers) in cases {
        let molecule = Molecule::from_fchk(&dir.join(format!("{job}.fchk"))).unwrap();
        let n_atoms = molecule.atomic_numbers.len();
        let text = std::fs::read_to_string(dir.join(format!("{job}.inp"))).unwrap();
        let wanted: Vec<Vec<usize>> = read_apost_input(&text, n_atoms).input.fragments.unwrap().iter().map(|f| f.iter().map(|a| a - 1).collect()).collect();
        let found = if centers.is_empty() { separate_molecules(n_atoms, &bonds(&molecule)) } else { fragments_around(n_atoms, &bonds(&molecule), centers) };
        assert_eq!(found, wanted, "{job}");
    }
}
