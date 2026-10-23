//! The case studies of the Chemistry-Methods paper (`fixtures/case-studies/`,
//! local only: Martí's calculations, APOST-3D run on his cluster): every run
//! opens from its results file with nothing to warn about, and the orbital
//! files it names are read and accepted. Skipped without the folder. Run in
//! release (`cargo test --release -p apost3dview --test case_studies`): the
//! molecules have up to 105 atoms and 1900 basis functions.

mod common;

use std::path::PathBuf;

use egui_kittest::kittest::{By, Queryable};

use common::{app_with, orientation, repo_dir, toasts};

fn case_studies() -> Option<PathBuf> {
    let dir = repo_dir().join("fixtures/case-studies");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("fixtures/case-studies/ not found; this test needs the paper's calculations");
        None
    }
}

/// (folder, job, orbital sets in the Surfaces window's Orbitals menu).
const RUNS: [(&str, &str, &[&str]); 12] = [
    ("Nitroprusside", "NP-EOS", &["EOS EFOs"]),
    ("p-Benzyne", "pB-CAS22", &[]),
    ("FeCp2", "FeCp2-EOS-OSLO", &["EOS EFOs", "OSLOs"]),
    ("FeCp2", "FeCp2-IQA", &[]),
    ("FeCO6", "FeCO6-EOS", &["EOS EFOs"]),
    ("Be-cAAC-Dip", "Be-cAAC-Dip-EOS", &["EOS EFOs"]),
    ("Be-cAAC-Dip", "Be-cAAC-Dip-GEOS", &["GEOS EFOs"]),
    ("Mg-cAAC-Dip", "Mg-cAAC-Dip-EOS", &["EOS EFOs"]),
    ("Mg-cAAC-Dip", "Mg-cAAC-Dip-GEOS", &["GEOS EFOs"]),
    ("Pn-4c", "4c-Sb-EOS", &["EOS EFOs"]),
    ("Pn-4d", "4d-Bi-EOS", &["EOS EFOs"]),
    ("Pn-4d", "4d-Bi-GEOS", &["GEOS EFOs"]),
];

/// Picks `option` in the Orbitals menu of the Surfaces window.
fn choose_orbitals(harness: &mut egui_kittest::Harness<'_, apost3dview::App>, option: &str) {
    let combo = harness.get_all(By::new().role(egui::accesskit::Role::ComboBox)).find(|n| {
        n.value().as_deref().is_some_and(|v| {
            let v = v.to_lowercase();
            v.contains("orbitals") || v.contains("efos") || v.contains("oslos")
        })
    });
    combo.expect("Orbitals menu").click_accesskit();
    harness.run_steps(2);
    // The last match: the menu entry, drawn after anything else of that name.
    harness.query_all_by_label(option).last().unwrap_or_else(|| panic!("no {option} in the Orbitals menu")).click_accesskit();
    harness.run_steps(2);
    // A menu picked through accessibility stays open.
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
}

#[test]
fn every_case_study_opens_with_its_orbitals() {
    let Some(dir) = case_studies() else { return };
    for (folder, job, sets) in RUNS {
        let results = dir.join(folder).join(format!("{job}.apost.json"));
        let mut harness = app_with(vec![results]);
        harness.run_steps(2);
        assert_eq!(toasts(&harness), vec![format!("APOST-3D results loaded from {job}.apost.json")], "{job}");
        assert!(harness.query_by_label("Results not loaded").is_none(), "{job}: results refused");
        if sets.is_empty() {
            continue;
        }
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        for set in sets {
            harness.state_mut().clear_toasts();
            choose_orbitals(&mut harness, set);
            harness.state_mut().select_set_orbitals(&[0], &[]);
            harness.run_steps(2);
            harness.get_by_label("Generate 1 orbital(s)").click_accesskit();
            harness.run_steps(4);
            let messages = toasts(&harness);
            assert_eq!(messages.len(), 1, "{job} {set}: {messages:?}");
            assert!(messages[0].ends_with(" generated"), "{job} {set}: {messages:?}");
        }
        let surfaces = harness.state().surface_labels();
        eprintln!("{job}: {surfaces:?}");
        assert_eq!(surfaces.len(), sets.len(), "{job}: {surfaces:?}");
    }
}

// ---------------------------------------------------------------------------
// The paper's figures. Off unless `APOST3DVIEW_CASE_STUDY_FIGURES=dir` is set:
// each figure is built in the app, saved as a session, given the orientation
// of the published figure it redraws (computed from the atoms, so it doesn't
// depend on how the program wrote the coordinates), reopened and rendered;
// `<dir>/<name>.png` and `<dir>/<name>.a3dview` are written.

use apost3dview_core::apost::ApostResults;
use glam::Vec3;

fn figures_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APOST3DVIEW_CASE_STUDY_FIGURES")?);
    std::fs::create_dir_all(&dir).unwrap();
    Some(dir)
}

fn read_results(path: &std::path::Path) -> ApostResults {
    apost3dview_core::parse_apost_results(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The atoms of a run: symbols and positions (bohr; only directions matter).
struct Atoms {
    symbols: Vec<String>,
    positions: Vec<Vec3>,
    fragments: Vec<Vec<usize>>,
}

impl Atoms {
    fn of(results: &ApostResults) -> Self {
        let molecule = results.molecule.as_ref().unwrap();
        let positions = molecule.coordinates_bohr.iter().map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)).collect();
        let fragments = results.fragments.as_ref().map(|f| f.iter().map(|f| f.atoms.iter().map(|a| a - 1).collect()).collect()).unwrap_or_default();
        Self { symbols: molecule.symbols.clone(), positions, fragments }
    }
    fn of_element(&self, symbol: &str) -> Vec<usize> {
        (0..self.symbols.len()).filter(|&i| self.symbols[i] == symbol).collect()
    }
    fn centroid(&self, atoms: &[usize]) -> Vec3 {
        atoms.iter().map(|&a| self.positions[a]).sum::<Vec3>() / atoms.len() as f32
    }
    /// Atoms nearest to `atom`, nearest first (itself left out).
    fn nearest(&self, atom: usize) -> Vec<usize> {
        let mut others: Vec<usize> = (0..self.positions.len()).filter(|&i| i != atom).collect();
        others.sort_by(|&a, &b| self.positions[a].distance(self.positions[atom]).total_cmp(&self.positions[b].distance(self.positions[atom])));
        others
    }
    /// Hydrogens, 1-based (for `hidden_atoms`).
    fn hydrogens(&self) -> Vec<usize> {
        self.of_element("H").into_iter().map(|a| a + 1).collect()
    }
}

fn toml_array(values: &[usize]) -> String {
    format!("[{}]", values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", "))
}

/// Opens `files`, saves the session, merges `patch` and the camera into it,
/// reopens it and returns the app (for more steps or rendering).
fn build(
    files: Vec<PathBuf>,
    setup: impl FnOnce(&mut egui_kittest::Harness<'static, apost3dview::App>),
    session: &std::path::Path,
    patch: &str,
    camera: [f32; 4],
    zoom_out: f32,
) -> egui_kittest::Harness<'static, apost3dview::App> {
    let mut first = app_with(files);
    setup(&mut first);
    first.state().save_session_to(session).unwrap();
    drop(first);
    let mut value: toml::Value = toml::from_str(&std::fs::read_to_string(session).unwrap()).unwrap();
    let patch: toml::Value = toml::from_str(patch).unwrap_or_else(|e| panic!("bad patch: {e}\n{patch}"));
    merge(&mut value, &patch);
    value["view"]["camera_orientation"] = toml::Value::Array(camera.iter().map(|&c| toml::Value::Float(f64::from(c))).collect());
    let distance = &mut value["view"]["camera_distance"];
    *distance = toml::Value::Float(distance.as_float().unwrap() * f64::from(zoom_out));
    std::fs::write(session, toml::to_string(&value).unwrap()).unwrap();
    let mut harness = app_with(vec![session.to_path_buf()]);
    harness.run_steps(8);
    let toasts = harness.state().toast_texts();
    assert!(toasts.iter().all(|t| !t.contains(", but:")), "{toasts:?}");
    harness
}

/// TOML tables merged key by key, arrays of tables element by element.
fn merge(base: &mut toml::Value, patch: &toml::Value) {
    match (base, patch) {
        (toml::Value::Table(base), toml::Value::Table(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (toml::Value::Array(base), toml::Value::Array(patch)) if patch.iter().all(toml::Value::is_table) && !patch.is_empty() => {
            for (i, value) in patch.iter().enumerate() {
                match base.get_mut(i) {
                    Some(existing) if existing.is_table() => merge(existing, value),
                    _ => base.push(value.clone()),
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

fn save_png(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, path: &std::path::Path, width: u32, height: u32) {
    let pixels = harness.state_mut().render_image(width, height).expect("render");
    image::RgbaImage::from_raw(width, height, pixels).unwrap().save(path).unwrap();
}

/// The orientations of the published figures, per system: (up, right,
/// elevation, turn), from the atoms.
fn ferrocene_view(atoms: &Atoms) -> [f32; 4] {
    // JCP 2024 Figs 3-4: Cp-Fe-Cp vertical, rings seen from a little above,
    // one carbon of the lower ring pointing at the viewer.
    let (top, bottom) = (&atoms.fragments[1], &atoms.fragments[2]);
    let up = atoms.centroid(top) - atoms.centroid(bottom);
    let front = bottom.iter().copied().find(|&a| atoms.symbols[a] == "C").unwrap();
    let toward = atoms.positions[front] - atoms.centroid(bottom);
    orientation(up, toward.cross(up), 0.33, std::f32::consts::PI / 5.0)
}

/// Be/Mg cAAC (Chem. Sci. 2022 Fig 2): C-M-C horizontal, looking along
/// the direction in which the heavy atoms spread least, so the Dipp groups
/// spread up and down and nothing stands in front of the C-M-C core.
fn carbene_view(atoms: &Atoms) -> [f32; 4] {
    let core = carbene_core(atoms);
    let axis = (atoms.positions[core[2]] - atoms.positions[core[0]]).normalize();
    let center = atoms.positions[core[1]];
    let e1 = axis.any_orthonormal_vector();
    let e2 = axis.cross(e1);
    let (mut a, mut b, mut c) = (0.0f32, 0.0f32, 0.0f32);
    for i in (0..atoms.symbols.len()).filter(|&i| atoms.symbols[i] != "H") {
        let d = atoms.positions[i] - center;
        let (x, y) = (d.dot(e1), d.dot(e2));
        a += x * x;
        b += x * y;
        c += y * y;
    }
    let angle = 0.5 * (2.0 * b).atan2(a - c);
    let up = e1 * angle.cos() + e2 * angle.sin();
    orientation(up, axis, 0.0, 0.0)
}

/// Be/Mg cAAC as in Chem. Sci. 2022 Fig 2: C-M-C horizontal and the pi
/// system vertical (the mean normal of the two carbene planes as up), seen
/// from `elevation` radians above.
fn carbene_pi_view(atoms: &Atoms, elevation: f32) -> [f32; 4] {
    let core = carbene_core(atoms);
    let metal = core[1];
    let normal = |c: usize| {
        let neighbors: Vec<usize> = atoms.nearest(c).into_iter().filter(|&a| a != metal).take(2).collect();
        let p = atoms.positions[c];
        (atoms.positions[neighbors[0]] - p).cross(atoms.positions[neighbors[1]] - p).normalize()
    };
    let (n1, mut n2) = (normal(core[0]), normal(core[2]));
    if n1.dot(n2) < 0.0 {
        n2 = -n2;
    }
    orientation(n1 + n2, atoms.positions[core[2]] - atoms.positions[core[0]], elevation, 0.0)
}

/// The matched atoms for aligning one cAAC complex onto the other: the
/// metal, the two carbene carbons and their nitrogens (0-based, C-M-C
/// core first).
fn carbene_core(atoms: &Atoms) -> Vec<usize> {
    let metal = (0..atoms.symbols.len()).find(|&i| atoms.symbols[i] == "Be" || atoms.symbols[i] == "Mg").unwrap();
    let near = atoms.nearest(metal);
    let (c1, c2) = (near[0], near[1]);
    let n_of = |c: usize| atoms.nearest(c).into_iter().find(|&a| atoms.symbols[a] == "N").unwrap();
    vec![c1, metal, c2, n_of(c1), n_of(c2)]
}

/// Fp pnictoranides (Chem. Sci. 2025 Fig 9): Fe above Pn, the two arms of
/// the ONNO ligand to the left and right, its O atoms toward the viewer.
fn pnictoranide_view(atoms: &Atoms) -> [f32; 4] {
    let fe = atoms.of_element("Fe")[0];
    let pn = (0..atoms.symbols.len()).find(|&i| atoms.symbols[i] == "Sb" || atoms.symbols[i] == "Bi").unwrap();
    let up = atoms.positions[fe] - atoms.positions[pn];
    let ligand = &atoms.fragments[5];
    let (n, o) = pnictoranide_arms(atoms, ligand);
    let arm = |i: usize| (atoms.positions[n[i]] + atoms.positions[o[i]]) / 2.0;
    let mut right = arm(1) - arm(0);
    let toward = right.normalize().cross(up.normalize());
    let o_side = (atoms.positions[o[0]] + atoms.positions[o[1]] - atoms.positions[n[0]] - atoms.positions[n[1]]) / 2.0;
    if toward.dot(o_side) < 0.0 {
        right = -right;
    }
    orientation(up, right, 0.17, 0.0)
}

/// The N and O donors of the ONNO ligand, paired by arm (each N with the
/// O nearest to it).
fn pnictoranide_arms(atoms: &Atoms, ligand: &[usize]) -> ([usize; 2], [usize; 2]) {
    let n: Vec<usize> = ligand.iter().copied().filter(|&a| atoms.symbols[a] == "N").collect();
    let o: Vec<usize> = ligand.iter().copied().filter(|&a| atoms.symbols[a] == "O").collect();
    let nearest_o = |a: usize| *o.iter().min_by(|&&x, &&y| atoms.positions[x].distance(atoms.positions[a]).total_cmp(&atoms.positions[y].distance(atoms.positions[a]))).unwrap();
    ([n[0], n[1]], [nearest_o(n[0]), nearest_o(n[1])])
}

/// The matched atoms for aligning 4d' onto 4c: Fe, Pn and the four donors.
fn pnictoranide_core(atoms: &Atoms) -> Vec<usize> {
    let fe = atoms.of_element("Fe")[0];
    let pn = (0..atoms.symbols.len()).find(|&i| atoms.symbols[i] == "Sb" || atoms.symbols[i] == "Bi").unwrap();
    let (n, o) = pnictoranide_arms(atoms, &atoms.fragments[5]);
    vec![fe, pn, n[0], n[1], o[0], o[1]]
}

/// `pnictoranide_core` of `atoms` with its two arms in the order that
/// matches `reference`'s best (the files list the arms in either order).
fn pnictoranide_core_matching(reference: &Atoms, atoms: &Atoms) -> Vec<usize> {
    let target: Vec<Vec3> = pnictoranide_core(reference).iter().map(|&a| reference.positions[a]).collect();
    let core = pnictoranide_core(atoms);
    let swapped = vec![core[0], core[1], core[3], core[2], core[5], core[4]];
    let rmsd = |c: &[usize]| {
        let moving: Vec<Vec3> = c.iter().map(|&a| atoms.positions[a]).collect();
        apost3dview_core::superpose(&target, &moving).map(|s| s.rmsd).unwrap_or(f64::MAX)
    };
    if rmsd(&swapped) < rmsd(&core) { swapped } else { core }
}

/// [Fe(CO)6]2+ (TCA 2026 Fig 1): the CO of fragment 2 pointing up from Fe,
/// a perpendicular CO to the right, turned a little so the COs in front
/// and behind show.
fn carbonyl_view(atoms: &Atoms) -> [f32; 4] {
    let fe = atoms.of_element("Fe")[0];
    let carbon = |f: usize| atoms.fragments[f].iter().copied().find(|&a| atoms.symbols[a] == "C").unwrap();
    let up = atoms.positions[carbon(1)] - atoms.positions[fe];
    let right = (2..atoms.fragments.len())
        .map(|f| atoms.positions[carbon(f)] - atoms.positions[fe])
        .min_by(|a, b| a.normalize().dot(up.normalize()).abs().total_cmp(&b.normalize().dot(up.normalize()).abs()))
        .unwrap();
    orientation(up, right, 0.10, 0.55)
}

/// The look of the papers' figures: hydrogens left out of the large
/// complexes (drawn thinner, as sticks), metal-Cp bonds left out of
/// ferrocene.
fn structure_patch(atoms: &Atoms, large: bool) -> String {
    let mut patch = String::new();
    if large {
        patch.push_str("[appearance.material]\natom_scale = 0.17\nbond_radius = 0.12\n");
    }
    patch.push_str("[[structures]]\n");
    if large {
        patch.push_str(&format!("hidden_atoms = {}\n", toml_array(&atoms.hydrogens())));
    }
    if let Some(&fe) = atoms.of_element("Fe").first()
        && atoms.fragments.len() == 3
    {
        let pairs: Vec<String> = atoms.of_element("C").iter().map(|&c| format!("[{}, {}]", fe.min(c) + 1, fe.max(c) + 1)).collect();
        patch.push_str(&format!("hidden_bonds = [{}]\n", pairs.join(", ")));
    }
    patch
}

#[test]
fn orientation_previews() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else {
        eprintln!("APOST3DVIEW_CASE_STUDY_FIGURES not set (or no case studies); previews not rendered");
        return;
    };
    // (folder, job, name suffix, orientation, large complex, zoom).
    type Preview = (&'static str, &'static str, &'static str, fn(&Atoms) -> [f32; 4], bool, f32);
    let previews: [Preview; 11] = [
        ("FeCp2", "FeCp2-EOS-OSLO", "", ferrocene_view, false, 0.85),
        ("Be-cAAC-Dip", "Be-cAAC-Dip-EOS", "-a", |a: &Atoms| carbene_pi_view(a, 0.0), true, 0.62),
        ("Be-cAAC-Dip", "Be-cAAC-Dip-EOS", "-b", |a: &Atoms| carbene_pi_view(a, 0.35), true, 0.62),
        ("Be-cAAC-Dip", "Be-cAAC-Dip-EOS", "-c", carbene_view, true, 0.62),
        ("Mg-cAAC-Dip", "Mg-cAAC-Dip-EOS", "-a", |a: &Atoms| carbene_pi_view(a, 0.0), true, 0.62),
        ("Mg-cAAC-Dip", "Mg-cAAC-Dip-EOS", "-b", |a: &Atoms| carbene_pi_view(a, 0.35), true, 0.62),
        ("Mg-cAAC-Dip", "Mg-cAAC-Dip-EOS", "-c", carbene_view, true, 0.62),
        ("Pn-4c", "4c-Sb-EOS", "", pnictoranide_view, true, 0.62),
        ("Pn-4d", "4d-Bi-EOS", "", pnictoranide_view, true, 0.62),
        ("FeCO6", "FeCO6-EOS", "", carbonyl_view, false, 0.62),
        ("FeCO6", "CO", "", |a: &Atoms| orientation(a.positions[a.of_element("O")[0]] - a.positions[a.of_element("C")[0]], Vec3::X, 0.0, 0.0), false, 1.4),
    ];
    for (folder, job, variant, view, hide_h, zoom) in previews {
        let results_path = dir.join(folder).join(format!("{job}.apost.json"));
        let (files, atoms) = if results_path.is_file() {
            let results = read_results(&results_path);
            (vec![results_path], Atoms::of(&results))
        } else {
            let fchk = dir.join(folder).join(format!("{job}.fchk"));
            let molecule = apost3dview_core::Molecule::from_fchk(&fchk).unwrap();
            let symbols = molecule.atomic_numbers.iter().map(|&z| apost3dview_core::element_data(z).symbol.to_string()).collect();
            (vec![fchk], Atoms { symbols, positions: molecule.positions.clone(), fragments: Vec::new() })
        };
        let mut hidden = structure_patch(&atoms, hide_h);
        if folder.ends_with("cAAC-Dip") {
            // The paper's isovalue for the natural orbitals.
            hidden.push_str("[[structures.surfaces]]\nisovalue = 0.05\n[[structures.surfaces]]\nisovalue = 0.05\nvisible = false\n");
        }
        let name = format!("orientation-{job}{variant}");
        let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
            // The frontier natural orbitals of the CASSCF complexes, to check
            // that the pi lobes point up and down as in the paper.
            if folder.ends_with("cAAC-Dip") {
                harness.get_by_label("Surfaces").click_accesskit();
                harness.run_steps(2);
                choose_orbitals(harness, "Natural orbitals (CASSCF)");
                harness.get_by_label("Frontier").click_accesskit();
                harness.run_steps(2);
                harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
                harness.run_steps(4);
            }
        };
        let mut harness = build(files, setup, &out.join(format!("{name}.a3dview")), &hidden, view(&atoms), zoom);
        save_png(&mut harness, &out.join(format!("{name}.png")), 900, 700);
    }
}

/// Opens a tool window from the toolbar, unless it is already open (the
/// toolbar button toggles it).
fn open_window(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, title: &str) {
    let ctx = harness.ctx.clone();
    if !harness.state().window_expanded(&ctx, title) {
        click(harness, title);
    }
}

fn click(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, label: &str) {
    harness.query_all_by_label(label).next().unwrap_or_else(|| panic!("no \"{label}\" to click")).click_accesskit();
    harness.run_steps(3);
}

/// Generates orbitals `rows` (0-based, alpha) of `set` and puts every
/// surface into a panel figure.
fn set_orbitals_as_panels(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, set: &str, rows: &[usize]) {
    open_window(harness, "Surfaces");
    choose_orbitals(harness, set);
    harness.state_mut().select_set_orbitals(rows, &[]);
    harness.run_steps(2);
    click(harness, &format!("Generate {} orbital(s)", rows.len()));
    harness.run_steps(4);
    open_window(harness, "Export");
    click(harness, "Panels");
}

fn save_figure(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, out: &std::path::Path, name: &str) {
    let (width, height, pixels) = harness.state_mut().render_panel_figure().expect("figure");
    image::RgbaImage::from_raw(width, height, pixels).unwrap().save(out.join(format!("{name}.png"))).unwrap();
    harness.state().save_session_to(&out.join(format!("{name}.a3dview"))).unwrap();
}

/// The paper's look for every figure: no depth of field (atoms in front
/// would blur), ambient occlusion on, white background.
const FIGURE_LOOK: &str = "[appearance]\ndepth_of_field = false\nambient_occlusion = true\n";

/// Panel captions for the paper (Martí, 2026-10-22: the 8 pt default reads
/// too small under full-page panels), in `[figure.settings]`.
const CAPTION_SIZE: &str = "font_size_pt = 11.0\n";

#[test]
fn ferrocene_oslos() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("FeCp2/FeCp2-EOS-OSLO.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    // JCP 2024 Fig 3, row by row: C 1s combinations of the top Cp, inner
    // Fe orbitals, the Cp pi orbitals, the last three Fe OSLOs.
    let oslos = [6, 7, 10, 21, 22, 27, 40, 42, 43, 46, 47, 48].map(|o| o - 1);
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| set_orbitals_as_panels(harness, "OSLOs", &oslos);
    let mut patch = format!("{FIGURE_LOOK}{}", structure_patch(&atoms, false));
    for _ in oslos {
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.085\n");
    }
    // Smaller atoms than the default, so the C 1s combinations show.
    patch.push_str("[appearance.material]\natom_scale = 0.15\nbond_radius = 0.10\n");
    patch.push_str(&format!("[figure.settings]\ncaption_style = \"value-and-fragment\"\nfixed_layout = true\nrows = 4\ncolumns = 3\n{CAPTION_SIZE}"));
    // "Cp" for the cyclopentadienyl rings, as in the paper.
    let foli = ["1.000", "1.000", "1.001", "1.010", "1.010", "1.018", "1.155", "1.367", "1.368", "1.380", "1.382", "1.548"];
    for (i, value) in foli.iter().enumerate() {
        let owner = if (3..6).contains(&i) || i >= 9 { "Fe" } else { "Cp" };
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{value} ({owner})\"\n"));
    }
    let session = out.join("ferrocene-oslos.a3dview");
    let mut harness = build(vec![results], setup, &session, &patch, ferrocene_view(&atoms), 0.62);
    save_figure(&mut harness, &out, "ferrocene-oslos");
}

/// Shows structure `label` (a click on it in the Molecule window).
fn show_structure(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, label: &str) {
    let ctx = harness.ctx.clone();
    harness.state().set_window_collapsed(&ctx, "Molecule", false);
    harness.run_steps(2);
    if harness.query_by_label("Keep the view when switching structures").is_none() {
        click(harness, "Molecule");
    }
    click(harness, label);
    // Collapsed again, so the windows below keep their room.
    let ctx = harness.ctx.clone();
    harness.state().set_window_collapsed(&ctx, "Molecule", true);
    harness.run_steps(2);
}

/// Generates the frontier natural orbitals (HONO, LUNO) of the active
/// structure.
fn frontier_natural_orbitals(harness: &mut egui_kittest::Harness<'static, apost3dview::App>) {
    open_window(harness, "Surfaces");
    choose_orbitals(harness, "Natural orbitals (CASSCF)");
    click(harness, "Frontier");
    click(harness, "Generate 2 orbital(s)");
    harness.run_steps(4);
}

#[test]
fn beryllium_and_magnesium_aligned() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    be_mg_natural_orbitals(&dir, &out);
}

/// The user guide's picture of aligned structures (molecule.md, "Aligning
/// related molecules"): the Be/Mg natural-orbital figure at the guide's
/// width. Made by `scripts/guide-images.sh` when the case studies are
/// here; kept (committed) otherwise.
#[test]
fn guide_aligned_comparison() {
    let (Some(dir), Some(images)) = (case_studies(), std::env::var_os("APOST3DVIEW_GUIDE_IMAGES").map(PathBuf::from)) else { return };
    let scratch = common::scratch_dir("case-guide");
    be_mg_natural_orbitals(&dir, &scratch);
    let figure = image::open(scratch.join("be-mg-natural-orbitals.png")).unwrap();
    let width = 1400;
    let height = figure.height() * width / figure.width();
    figure.resize_exact(width, height, image::imageops::FilterType::Lanczos3).save(images.join("case-be-mg-aligned.png")).unwrap();
}

fn be_mg_natural_orbitals(dir: &std::path::Path, out: &std::path::Path) {
    let be = dir.join("Be-cAAC-Dip/Be-cAAC-Dip-EOS.apost.json");
    let mg = dir.join("Mg-cAAC-Dip/Mg-cAAC-Dip-EOS.apost.json");
    let (be_atoms, mg_atoms) = (Atoms::of(&read_results(&be)), Atoms::of(&read_results(&mg)));
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
        let labels = harness.state().structure_labels();
        let (be_label, mg_label) = (labels[0].clone(), labels[1].clone());
        show_structure(harness, &be_label);
        frontier_natural_orbitals(harness);
        show_structure(harness, &be_label);
        harness.state_mut().select_atoms(&carbene_core(&be_atoms));
        show_structure(harness, &mg_label);
        frontier_natural_orbitals(harness);
        show_structure(harness, &mg_label);
        harness.state_mut().select_atoms(&carbene_core(&mg_atoms));
        let ctx = harness.ctx.clone();
        harness.state().set_window_collapsed(&ctx, "Molecule", false);
        harness.run_steps(3);
        click(harness, &format!("Align onto {be_label}"));
        let toast = harness.state().toast_texts().into_iter().find(|t| t.contains("aligned onto")).expect("aligned");
        eprintln!("{toast}");
        harness.state_mut().select_atoms(&[]);
        // Be's surfaces first (Panels fills the list with the active
        // structure's), then Mg's; the camera is left on Be.
        show_structure(harness, &be_label);
        harness.state_mut().select_atoms(&[]);
        open_window(harness, "Export");
        click(harness, "Panels");
        show_structure(harness, &mg_label);
        open_window(harness, "Export");
        click(harness, "All surfaces");
        show_structure(harness, &be_label);
    };
    let mut patch = format!("{FIGURE_LOOK}[appearance.material]\natom_scale = 0.17\nbond_radius = 0.12\n");
    for atoms in [&be_atoms, &mg_atoms] {
        patch.push_str(&format!("[[structures]]\nhidden_atoms = {}\n", toml_array(&atoms.hydrogens())));
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\n[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\n");
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 2\ncolumns = 2\npanel_shape = \"view\"\n{CAPTION_SIZE}"));
    for caption in ["Be: HONO 1.640", "Be: LUNO 0.362", "Mg: HONO 1.208", "Mg: LUNO 0.794"] {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    let session = out.join("be-mg-natural-orbitals.a3dview");
    let mut harness = build(vec![be, mg], setup, &session, &patch, carbene_pi_view(&be_atoms, 0.0), 0.5);
    eprintln!("panels {:?}", harness.state().panel_captions());
    save_figure(&mut harness, out, "be-mg-natural-orbitals");
}

/// An APOST-3D results view showing `id` from `focus` (0-based).
fn results_view(id: &str, focus: usize) -> String {
    format!("[structures.results_view]\nvisible = true\nfocus = {focus}\nlabels = true\n[structures.results_view.shown]\nshow = \"quantity\"\nid = \"{id}\"\n")
}

#[test]
fn ferrocene_iqa() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("FeCp2/FeCp2-IQA.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    // A Cp as focus, colors on a logarithmic scale: Fe-Cp strongly
    // attractive, Cp-Cp weakly repulsive (JCP 2024 Table II). Fe as focus
    // between atoms: each Fe-C (Table III), values only.
    let views = [
        ("ferrocene-iqa-fragments", "enpart.total.fragments", 1, "log_scale = true\n"),
        ("ferrocene-iqa-atoms", "enpart.total.atoms", 0, "names = false\n"),
    ];
    for (name, id, focus, extra) in views {
        let view = results_view(id, focus).replace("labels = true\n", &format!("labels = true\n{extra}"));
        let patch = format!("{FIGURE_LOOK}{}{view}", structure_patch(&atoms, false));
        let session = out.join(format!("{name}.a3dview"));
        let mut harness = build(vec![results.clone()], |_| {}, &session, &patch, ferrocene_view(&atoms), 1.05);
        eprintln!("{name}: {:?}", harness.state().legend_title());
        save_png(&mut harness, &out.join(format!("{name}.png")), 1200, 1000);
        harness.state().save_session_to(&session).unwrap();
    }
}

/// Generates analysis orbitals of `set` one at a time, in this order, so
/// that the surface list (and so the panels) follows it; `(row, beta)`
/// 0-based in the channel's list.
fn generate_in_order(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, set: &str, orbitals: &[(usize, bool)]) {
    open_window(harness, "Surfaces");
    choose_orbitals(harness, set);
    for &(row, beta) in orbitals {
        if beta {
            harness.state_mut().select_set_orbitals(&[], &[row]);
        } else {
            harness.state_mut().select_set_orbitals(&[row], &[]);
        }
        harness.run_steps(2);
        click(harness, "Generate 1 orbital(s)");
        harness.run_steps(3);
    }
}

/// Two related molecules in one panel figure: `generate` run in each, the
/// second aligned onto the first on the matched atoms `cores`, the panels
/// of both, the first one's camera (`view`) for all.
#[allow(clippy::too_many_arguments)]
fn aligned_figure(
    out: &std::path::Path,
    name: &str,
    files: [PathBuf; 2],
    cores: [Vec<usize>; 2],
    generate: fn(&mut egui_kittest::Harness<'static, apost3dview::App>, usize),
    patch: &str,
    view: [f32; 4],
    zoom: f32,
) -> egui_kittest::Harness<'static, apost3dview::App> {
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
        let labels = harness.state().structure_labels();
        for (i, label) in labels.iter().enumerate() {
            show_structure(harness, label);
            generate(harness, i);
            show_structure(harness, label);
            harness.state_mut().select_atoms(&cores[i]);
        }
        let ctx = harness.ctx.clone();
        harness.state().set_window_collapsed(&ctx, "Molecule", false);
        harness.run_steps(3);
        click(harness, &format!("Align onto {}", labels[0]));
        let toast = harness.state().toast_texts().into_iter().find(|t| t.contains("aligned onto")).expect("aligned");
        eprintln!("{name}: {toast}");
        for label in &labels {
            show_structure(harness, label);
            harness.state_mut().select_atoms(&[]);
        }
        // The first structure's surfaces first, then the second's; the
        // camera is left on the first.
        show_structure(harness, &labels[0]);
        open_window(harness, "Export");
        click(harness, "Panels");
        show_structure(harness, &labels[1]);
        open_window(harness, "Export");
        click(harness, "All surfaces");
        show_structure(harness, &labels[0]);
    };
    let [a, b] = files;
    let mut harness = build(vec![a, b], setup, &out.join(format!("{name}.a3dview")), patch, view, zoom);
    eprintln!("{name}: {:?}", harness.state().panel_captions());
    save_figure(&mut harness, out, name);
    harness
}

#[test]
fn ferrocene_efos() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("FeCp2/FeCp2-EOS-OSLO.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    // JCP 2024 Fig 4c: the occupied Fe d EFOs, the upper Cp's pi EFOs, and
    // the two Fe d EFOs with 0.310 (the first unoccupied), at 0.1.
    let rows = [43, 44, 42, 40, 45, 47, 49, 50];
    let captions = ["0.817 (Fe)", "0.817 (Fe)", "0.890 (Fe)", "0.899 (Cp)", "0.795 (Cp)", "0.795 (Cp)", "0.310 (Fe)", "0.310 (Fe)"];
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
        generate_in_order(harness, "EOS EFOs", &rows.map(|r| (r - 1, false)));
        open_window(harness, "Export");
        click(harness, "Panels");
    };
    let mut patch = format!("{FIGURE_LOOK}{}", structure_patch(&atoms, false));
    for _ in rows {
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.1\n");
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 3\ncolumns = 3\n{CAPTION_SIZE}"));
    for caption in captions {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    let mut harness = build(vec![results], setup, &out.join("ferrocene-efos.a3dview"), &patch, ferrocene_view(&atoms), 0.62);
    let session = std::fs::read_to_string(out.join("ferrocene-efos.a3dview")).unwrap();
    assert_eq!(session.matches("fragment_weighted = true").count(), rows.len(), "EFOs drawn times their fragment's weight");
    save_figure(&mut harness, &out, "ferrocene-efos");
}

#[test]
fn beryllium_and_magnesium_frontier_efos() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let be = dir.join("Be-cAAC-Dip/Be-cAAC-Dip-EOS.apost.json");
    let mg = dir.join("Mg-cAAC-Dip/Mg-cAAC-Dip-EOS.apost.json");
    let (be_atoms, mg_atoms) = (Atoms::of(&read_results(&be)), Atoms::of(&read_results(&mg)));
    // Chem. Sci. 2022 Fig 3: the two LO EFOs on the cAACs and the FU EFO on
    // the metal, from the results file's frontier lists.
    fn generate(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, structure: usize) {
        let rows: [usize; 3] = if structure == 0 { [160, 161, 162] } else { [164, 165, 166] };
        generate_in_order(harness, "EOS EFOs", &rows.map(|r| (r - 1, false)));
    }
    let captions = ["Be: LO 0.435 (cAAC)", "Be: LO 0.435 (cAAC)", "Be: FU 0.175 (Be)", "Mg: LO 0.469 (cAAC)", "Mg: LO 0.467 (cAAC)", "Mg: FU 0.177 (Mg)"];
    let mut patch = format!("{FIGURE_LOOK}[appearance.material]\natom_scale = 0.17\nbond_radius = 0.12\n");
    for atoms in [&be_atoms, &mg_atoms] {
        patch.push_str(&format!("[[structures]]\nhidden_atoms = {}\n", toml_array(&atoms.hydrogens())));
        for _ in 0..3 {
            patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\n");
        }
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 2\ncolumns = 3\npanel_shape = \"view\"\n{CAPTION_SIZE}"));
    for caption in captions {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    aligned_figure(&out, "be-mg-frontier-efos", [be, mg], [carbene_core(&be_atoms), carbene_core(&mg_atoms)], generate, &patch, carbene_pi_view(&be_atoms, 0.0), 0.5);
}

#[test]
fn pnictoranide_frontier_efos() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let sb = dir.join("Pn-4c/4c-Sb-EOS.apost.json");
    let bi = dir.join("Pn-4d/4d-Bi-EOS.apost.json");
    let (sb_atoms, bi_atoms) = (Atoms::of(&read_results(&sb)), Atoms::of(&read_results(&bi)));
    // Chem. Sci. 2025 Fig 9: the frontier EFOs of the Fe-Pn bond (4d′: the
    // alpha ones; beta 0.468 / 0.404).
    fn generate(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, _: usize) {
        generate_in_order(harness, "EOS EFOs", &[(194, false), (195, false)]);
    }
    let captions = ["4c: LO 0.636 (Sb)", "4c: FU 0.447 (Fe)", "4d\u{2032}: LO 0.474 (Fe)", "4d\u{2032}: FU 0.408 (Bi)"];
    let mut patch = format!("{FIGURE_LOOK}[appearance.material]\natom_scale = 0.17\nbond_radius = 0.12\n");
    for atoms in [&sb_atoms, &bi_atoms] {
        patch.push_str(&format!("[[structures]]\nhidden_atoms = {}\n", toml_array(&atoms.hydrogens())));
        for _ in 0..2 {
            patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\n");
        }
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 2\ncolumns = 2\npanel_shape = \"view\"\n{CAPTION_SIZE}"));
    for caption in captions {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    aligned_figure(&out, "pnictoranide-frontier-efos", [sb, bi], [pnictoranide_core(&sb_atoms), pnictoranide_core_matching(&sb_atoms, &bi_atoms)], generate, &patch, pnictoranide_view(&sb_atoms), 0.62);
}

/// Generates SCF orbitals of the active structure one at a time, in this
/// order (1-based MO numbers, alpha), so the panels follow it.
fn generate_scf_in_order(harness: &mut egui_kittest::Harness<'static, apost3dview::App>, mos: &[usize]) {
    open_window(harness, "Surfaces");
    for &mo in mos {
        click(harness, "Clear");
        harness.query_all_by_label_contains(&format!("MO {mo}  ")).next().unwrap_or_else(|| panic!("no MO {mo} row")).click_accesskit();
        harness.run_steps(2);
        click(harness, "Generate 1 orbital(s)");
        harness.run_steps(3);
    }
}

#[test]
fn carbonyl_efos() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("FeCO6/FeCO6-EOS.apost.json");
    let co = dir.join("FeCO6/CO.fchk");
    let atoms = Atoms::of(&read_results(&results));
    // TCA 2026 Fig 1: the pi*, sigma and pi EFOs of the CO pointing up in
    // [Fe(CO)6]2+ (fragment 2), above the same orbitals of free CO, its
    // canonical MOs (neutral CO: 1pi = MOs 5, 6; 5sigma, the HOMO, = 7;
    // 2pi* = 8, 9), at 0.05.
    let rows = [65, 66, 47, 32, 33];
    let mos = [8, 9, 7, 5, 6];
    let captions = [
        "\u{3c0}* 0.065", "\u{3c0}* 0.065", "\u{3c3} 0.799", "\u{3c0} 0.994", "\u{3c0} 0.994",
        "CO \u{3c0}*", "CO \u{3c0}*", "CO \u{3c3} (HOMO)", "CO \u{3c0}", "CO \u{3c0}",
    ];
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
        let labels = harness.state().structure_labels();
        show_structure(harness, &labels[0]);
        generate_in_order(harness, "EOS EFOs", &rows.map(|r| (r - 1, false)));
        show_structure(harness, &labels[1]);
        generate_scf_in_order(harness, &mos);
        show_structure(harness, &labels[0]);
        open_window(harness, "Export");
        click(harness, "Panels");
        show_structure(harness, &labels[1]);
        open_window(harness, "Export");
        click(harness, "All surfaces");
        show_structure(harness, &labels[0]);
    };
    // Free CO in its own view: C below, O above, at the scale of a CO in
    // the complex's panels.
    let molecule = apost3dview_core::Molecule::from_fchk(&co).unwrap();
    let (c, o) = (molecule.positions[0], molecule.positions[1]);
    let co_view = orientation(o - c, Vec3::X, 0.0, 0.0);
    let center = (c + o) / 2.0;
    let mut patch = format!("{FIGURE_LOOK}[view]\nkeep_view = false\n[[structures]]\n");
    for _ in rows {
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\n");
    }
    patch.push_str(&format!(
        "[[structures]]\n[structures.view]\ntarget = [{}, {}, {}]\ndistance = 6.5\norientation = [{}, {}, {}, {}]\n",
        center.x, center.y, center.z, co_view[0], co_view[1], co_view[2], co_view[3]
    ));
    for _ in mos {
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\n");
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 2\ncolumns = 5\n{CAPTION_SIZE}"));
    for caption in captions {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    let mut harness = build(vec![results, co], setup, &out.join("carbonyl-efos.a3dview"), &patch, carbonyl_view(&atoms), 1.0);
    eprintln!("carbonyl: {:?}", harness.state().panel_captions());
    save_figure(&mut harness, &out, "carbonyl-efos");
}

/// Octahedral complexes as usually drawn: the ligand on fragment `axial`
/// (1-based) up from the metal, an equatorial ligand to the right, seen
/// from a little above and turned so the ligand in front doesn't hide the
/// metal (as `carbonyl_view`, for any first atom of a fragment).
fn octahedral_view(atoms: &Atoms, axial: usize) -> [f32; 4] {
    let metal = atoms.fragments[0][0];
    let donor = |f: usize| atoms.fragments[f][0];
    let up = atoms.positions[donor(axial - 1)] - atoms.positions[metal];
    let right = (1..atoms.fragments.len())
        .filter(|&f| f != axial - 1)
        .map(|f| atoms.positions[donor(f)] - atoms.positions[metal])
        .min_by(|a, b| a.normalize().dot(up.normalize()).abs().total_cmp(&b.normalize().dot(up.normalize()).abs()))
        .unwrap();
    orientation(up, right, 0.17, 0.55)
}

#[test]
fn nitroprusside_figure() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("Nitroprusside/NP-EOS.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    let view = octahedral_view(&atoms, 2);
    // (b) EOS oxidation states, (c) fragment bond orders from Fe.
    for (name, id) in [("np-oxidation-states", "eos.fragments.oxidation_states"), ("np-bond-orders", "bond_orders.fragments.matrix")] {
        let patch = format!("{FIGURE_LOOK}[[structures]]\n{}", results_view(id, 0));
        let session = out.join(format!("{name}.a3dview"));
        let mut harness = build(vec![results.clone()], |_| {}, &session, &patch, view, 0.95);
        eprintln!("{name}: {:?}", harness.state().legend_title());
        save_png(&mut harness, &out.join(format!("{name}.png")), 1400, 1400);
        harness.state().save_session_to(&session).unwrap();
    }
    // (d, e) the frontier EFOs: Fe d (last occupied) and NO pi* (first
    // unoccupied), rows 54 and 57 of the results file's frontier lists.
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
        generate_in_order(harness, "EOS EFOs", &[(53, false), (56, false)]);
        open_window(harness, "Export");
        click(harness, "Panels");
    };
    let mut patch = format!("{FIGURE_LOOK}[[structures]]\n");
    for _ in 0..2 {
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\n");
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 1\ncolumns = 2\n{CAPTION_SIZE}"));
    for caption in ["LO 0.632 (Fe)", "FU 0.338 (NO)"] {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    let mut harness = build(vec![results.clone()], setup, &out.join("np-frontier-efos.a3dview"), &patch, view, 0.95);
    save_figure(&mut harness, &out, "np-frontier-efos");

    // (a) The whole window: the fragment bond orders seen from Fe (the
    // focus view) on the molecule, the APOST-3D window with its matrix
    // tools beside it.
    let mut harness = common::app_with_scale(vec![results], 2.0);
    harness.run_steps(4);
    harness.state_mut().set_view_orientation(view);
    harness.state_mut().zoom_view(1.15);
    assert!(harness.state_mut().show_on_molecule("bond_orders.fragments.matrix"));
    harness.state_mut().clear_toasts();
    harness.run_steps(8);
    harness.remove_cursor();
    let image = harness.render().expect("render");
    image.save(out.join("np-window.png")).unwrap();
}

#[test]
fn p_benzyne_figure() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("p-Benzyne/pB-CAS22.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    // The ring facing the viewer, the two radical carbons (C1, C4) left and
    // right.
    let p = &atoms.positions;
    let view = orientation((p[1] + p[2]) - (p[4] + p[5]), p[0] - p[3], 0.0, 0.0);
    // The frontier natural orbitals (HONO, LUNO).
    let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| {
        frontier_natural_orbitals(harness);
        open_window(harness, "Export");
        click(harness, "Panels");
    };
    let mut patch = format!("{FIGURE_LOOK}[[structures]]\n");
    for _ in 0..2 {
        patch.push_str("[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\n");
    }
    patch.push_str(&format!("[figure.settings]\nfixed_layout = true\nrows = 1\ncolumns = 2\n{CAPTION_SIZE}"));
    for caption in ["HONO 1.200", "LUNO 0.800"] {
        patch.push_str(&format!("[[figure.panels]]\ncaption = \"{caption}\"\n"));
    }
    let mut harness = build(vec![results.clone()], setup, &out.join("pb-natural-orbitals.a3dview"), &patch, view, 1.3);
    eprintln!("pb: {:?}", harness.state().panel_captions());
    save_figure(&mut harness, &out, "pb-natural-orbitals");
    // The local spin: each atom's own <S^2> (the diagonal), as halos.
    let local = results_view("local_spin.atoms.s2", 0).replace("labels = true\n", "labels = true\ndiagonal = true\nnames = false\n");
    let patch = format!("{FIGURE_LOOK}[[structures]]\n{local}");
    let session = out.join("pb-local-spin.a3dview");
    let mut harness = build(vec![results], |_| {}, &session, &patch, view, 1.35);
    eprintln!("pb local spin: {:?}", harness.state().legend_title());
    save_png(&mut harness, &out.join("pb-local-spin.png"), 1400, 1100);
    harness.state().save_session_to(&session).unwrap();
}

/// Supporting Information: the rendering options on one scene, Be(cAAC)2
/// with its HONO: (a) no ambient occlusion, perspective; (b) ambient
/// occlusion, perspective; (c) ambient occlusion, orthographic (the
/// default); (d) translucent orbital; (e) depth of field.
#[test]
fn rendering_options() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("Be-cAAC-Dip/Be-cAAC-Dip-EOS.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    let hydrogens = format!("[[structures]]\nhidden_atoms = {}\n", toml_array(&atoms.hydrogens()));
    let orbital = |opacity: f32| format!("[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\nopacity = {opacity}\n[[structures.surfaces]]\nvisible = false\n");
    let variants = [
        ("render-a-plain", "[appearance]\nambient_occlusion = false\ndepth_of_field = false\n[view]\nprojection = \"perspective\"\n", 1.0),
        ("render-b-ao", "[appearance]\nambient_occlusion = true\ndepth_of_field = false\n[view]\nprojection = \"perspective\"\n", 1.0),
        ("render-c-orthographic", "[appearance]\nambient_occlusion = true\ndepth_of_field = false\n[view]\nprojection = \"orthographic\"\n", 1.0),
        ("render-d-translucent", "[appearance]\nambient_occlusion = true\ndepth_of_field = false\n[view]\nprojection = \"orthographic\"\n", 0.5),
        ("render-e-dof", "[appearance]\nambient_occlusion = true\ndepth_of_field = true\n[view]\nprojection = \"orthographic\"\n", 1.0),
    ];
    for (name, look, opacity) in variants {
        let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| frontier_natural_orbitals(harness);
        let patch = format!("{look}[appearance.material]\natom_scale = 0.17\nbond_radius = 0.12\n{hydrogens}{}", orbital(opacity));
        let session = out.join(format!("{name}.a3dview"));
        let mut harness = build(vec![results.clone()], setup, &session, &patch, carbene_view(&atoms), 0.62);
        save_png(&mut harness, &out.join(format!("{name}.png")), 1200, 1000);
        harness.state().save_session_to(&session).unwrap();
    }
}

/// Pictures for the table-of-contents graphic: nitroprusside with its
/// NO-Fe-CN axis horizontal (NO to the right), oxidation states on the
/// envelopes; and the two aligned HONOs of the Be and Mg complexes alone.
#[test]
fn toc_pictures() {
    let (Some(dir), Some(out)) = (case_studies(), figures_dir()) else { return };
    let results = dir.join("Nitroprusside/NP-EOS.apost.json");
    let atoms = Atoms::of(&read_results(&results));
    let metal = atoms.positions[atoms.fragments[0][0]];
    let axis = atoms.positions[atoms.fragments[1][0]] - metal;
    let up = (2..atoms.fragments.len())
        .map(|f| atoms.positions[atoms.fragments[f][0]] - metal)
        .min_by(|a, b| a.normalize().dot(axis.normalize()).abs().total_cmp(&b.normalize().dot(axis.normalize()).abs()))
        .unwrap();
    let view = orientation(up, axis, 0.75, 0.5);
    // Large labels: the picture is printed about 2.5 cm tall.
    let patch = format!(
        "{FIGURE_LOOK}[[structures]]\n{}",
        results_view("eos.fragments.oxidation_states", 0).replace("labels = true\n", "labels = true\nlabel_size = 70.0\n")
    );
    let session = out.join("toc-np.a3dview");
    let mut harness = build(vec![results], |_| {}, &session, &patch, view, 0.8);
    save_png(&mut harness, &out.join("toc-np.png"), 2400, 1300);
    // Be and Mg, aligned, each structure's HONO alone.
    for (folder, job, name) in [("Be-cAAC-Dip", "Be-cAAC-Dip-EOS", "toc-be"), ("Mg-cAAC-Dip", "Mg-cAAC-Dip-EOS", "toc-mg")] {
        let results = dir.join(format!("{folder}/{job}.apost.json"));
        let atoms = Atoms::of(&read_results(&results));
        let setup = |harness: &mut egui_kittest::Harness<'static, apost3dview::App>| frontier_natural_orbitals(harness);
        let patch = format!(
            "{FIGURE_LOOK}[appearance.material]\natom_scale = 0.17\nbond_radius = 0.12\n[[structures]]\nhidden_atoms = {}\n[[structures.surfaces]]\nisovalue = 0.05\nvisible = true\n[[structures.surfaces]]\nvisible = false\n",
            toml_array(&atoms.hydrogens())
        );
        let session = out.join(format!("{name}.a3dview"));
        let mut harness = build(vec![results], setup, &session, &patch, carbene_pi_view(&atoms, 0.0), 0.5);
        save_png(&mut harness, &out.join(format!("{name}.png")), 1400, 1200);
    }
}
