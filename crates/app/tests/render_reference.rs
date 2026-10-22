//! Reference scenes: a fixed set of pictures covering every render
//! pipeline (atoms, bonds, highlights, labels, isosurfaces, densities,
//! envelopes, halos, value labels, legend, ambient occlusion, depth of
//! field; export and live window), for checking that a change to the
//! renderer leaves the pictures exactly as they were.
//!
//! Off unless `APOST3DVIEW_REFERENCE_DIR` is set:
//!
//! - `APOST3DVIEW_REFERENCE=write` saves every scene as `<dir>/<name>.png`;
//! - otherwise every scene is rendered again and compared with the saved
//!   one, pixel by pixel; differing scenes are reported with how many
//!   pixels differ and by how much, their new picture and a difference
//!   image are saved under `<dir>/compare/`, and the test fails.
//!
//! Typical use: write before changing the renderer, compare after.
//! Scenes whose input files are missing (the APOST-3D fixtures are not in
//! git, see `scripts/sync-apost3d-fixtures.sh`) are skipped with a notice.
//!
//! Each scene is a session: the files are opened, the session saved, the
//! scene's settings written into it (camera framing stays the app's own),
//! and the session opened in a fresh app and rendered.

mod common;

use std::path::{Path, PathBuf};

use apost3dview_render::Material;
use common::{app_with, repo_dir, scratch_dir};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;

struct Scene {
    name: &'static str,
    /// Relative to the repository.
    files: &'static [&'static str],
    /// TOML merged into the saved session: tables key by key, arrays of
    /// tables element by element, anything else replaced.
    patch: String,
    /// Atoms to select (0-based) after opening, for the highlight pass.
    select: &'static [usize],
    /// The whole window (the live view's draw path) instead of an export.
    window: bool,
    /// Camera distance relative to the app's own framing, to fit orbital
    /// lobes and labels of small molecules.
    zoom_out: f32,
}

fn scene(name: &'static str, files: &'static [&'static str], patch: &str) -> Scene {
    Scene { name, files, patch: patch.to_string(), select: &[], window: false, zoom_out: 1.0 }
}

const BI: &[&str] = &["fixtures/bi-dianion-osd/Bi-dianion-OSD.fchk"];
const BICL3: &[&str] = &["validation/gaussian/BiCl3-def2TZVPP.fchk"];
const H2O: &[&str] = &["validation/gaussian/H2O-ccpVTZ.fchk"];
const H2O_T: &[&str] = &["validation/gaussian/H2O-T-ccpVTZ.fchk"];
const FE: &[&str] = &["fixtures/apost3d/FeCN5NO3--UBLYP.apost.json"];
const C2H6: &[&str] = &["fixtures/apost3d/C2H6-B3LYP.apost.json"];
const C2H6_OSLO: &[&str] = &["examples/C2H6-OSLO-2CH3/C2H6-OSLO-2CH3.apost.json"];
const CH3F: &[&str] = &["fixtures/apost3d/CH3F.apost.json"];
/// APOST-3D EOS (Löwdin) on the 83-atom Bi complex, run locally
/// (2026-10-19); not in git, so the scene is skipped without it.
const BI_EOS: &[&str] = &["fixtures/bi-eos-lowdin/Bi-EOS-LOWDIN.apost.json"];

const PLAIN: &str = "[appearance]\nambient_occlusion = false\ndepth_of_field = false\n";
const AO: &str = "[appearance]\nambient_occlusion = true\ndepth_of_field = false\n";
const DOF: &str = "[appearance]\nambient_occlusion = false\ndepth_of_field = true\n";
const AO_DOF: &str = "[appearance]\nambient_occlusion = true\ndepth_of_field = true\n";
/// The fragment envelopes (results open showing nothing).
const FRAGMENTS: &str = "[[structures]]\n[structures.results_view.shown]\nshow = \"fragments\"\n";

/// A generated-orbital surface (1-based orbital, Medium grid).
fn orbital(orbital: usize, opacity: f32, refinement: usize, set: Option<&str>, by_fragment: bool) -> String {
    let set = set.map(|s| format!("orbital_set = \"{s}\"\n")).unwrap_or_default();
    format!(
        "[[structures.surfaces]]\nlabel = \"orbital\"\nvisible = true\nisovalue = 0.075\nrefinement = {refinement}\nboth_signs = true\n\
         positive_color = [20, 95, 170, 255]\nnegative_color = [175, 25, 60, 255]\nopacity = {opacity}\nby_fragment = {by_fragment}\n\
         kind = \"orbital\"\nspin = \"alpha\"\norbital = {orbital}\ngrid_spacing_bohr = 0.28\nbox_padding_bohr = 4.0\n{set}"
    )
}

/// The total or spin density (opaque; see `translucent`).
fn density(spin: bool) -> String {
    let (isovalue, both, positive) = if spin { (0.01, true, "[20, 95, 170, 255]") } else { (0.1, false, "[120, 160, 210, 255]") };
    format!(
        "[[structures.surfaces]]\nlabel = \"density\"\nvisible = true\nisovalue = {isovalue}\nrefinement = 1\nboth_signs = {both}\n\
         positive_color = {positive}\nnegative_color = [175, 25, 60, 255]\nopacity = 1.0\nby_fragment = false\n\
         kind = \"density\"\nspin_density = {spin}\ngrid_spacing_bohr = 0.28\nbox_padding_bohr = 4.0\n"
    )
}

/// `surface` (from `orbital` or `density`) at `opacity`, with other colors
/// if given.
fn translucent(surface: String, opacity: f32, colors: Option<(&str, &str)>) -> String {
    let surface = surface.replace("opacity = 1.0", &format!("opacity = {opacity}")).replace("opacity = 1\n", &format!("opacity = {opacity}\n"));
    match colors {
        Some((positive, negative)) => surface
            .replace("positive_color = [20, 95, 170, 255]", &format!("positive_color = {positive}"))
            .replace("negative_color = [175, 25, 60, 255]", &format!("negative_color = {negative}")),
        None => surface,
    }
}

/// An APOST-3D results view showing `id` (focus 0-based).
fn results(id: &str, focus: usize, bond_thickness: bool, log_scale: bool) -> String {
    format!(
        "[structures.results_view]\nvisible = true\nfocus = {focus}\nlabels = true\nlabel_size = 26.0\nbond_thickness = {bond_thickness}\n\
         log_scale = {log_scale}\n[structures.results_view.shown]\nshow = \"quantity\"\nid = \"{id}\"\n"
    )
}

/// `scene`, zoomed out.
fn far(zoom_out: f32, scene: Scene) -> Scene {
    Scene { zoom_out, ..scene }
}

fn scenes() -> Vec<Scene> {
    let publication = toml::to_string(&Material::publication()).unwrap();
    let mut list = vec![
        // Ball-and-stick, every combination of the two post-processes.
        scene("bi-plain", BI, PLAIN),
        scene("bi-ao", BI, AO),
        scene("bi-dof", BI, DOF),
        scene("bi-ao-dof", BI, AO_DOF),
        scene("bi-publication", BI, &format!("{AO}[appearance.material]\n{publication}")),
        // Labels, measurements, a dashed bond and a selection highlight.
        Scene {
            select: &[1, 2],
            zoom_out: 1.8,
            ..scene(
                "bicl3-labels",
                BICL3,
                &format!(
                    "{AO}atom_labels = \"numbertype\"\n[[structures]]\ndashed_bonds = [[1, 4]]\n\
                     [[structures.measurements]]\natoms = [1, 2]\nlabel_offset = [0.0, 0.0]\n\
                     [[structures.measurements]]\natoms = [2, 1, 3]\nlabel_offset = [0.0, 0.0]\n"
                ),
            )
        },
        // Orbitals: opaque with ambient occlusion, translucent, refined
        // with depth of field.
        far(2.4, scene("h2o-homo-ao", H2O, &format!("{AO}[[structures]]\n{}", orbital(5, 1.0, 1, None, false)))),
        far(2.4, scene("h2o-homo-translucent", H2O, &format!("{PLAIN}[[structures]]\n{}", orbital(5, 0.6, 1, None, false)))),
        far(2.4, scene("h2o-lumo-ao-dof", H2O, &format!("{AO_DOF}[[structures]]\n{}", orbital(6, 1.0, 2, None, false)))),
        // Densities of an open shell.
        far(2.6, scene("triplet-densities", H2O_T, &format!("{AO}[[structures]]\n{}{}", density(false), density(true)))),
        // Translucent surfaces, sorted farthest first: two orbitals that
        // intersect, and the total density as it is first drawn (with
        // depth of field, which focuses through it).
        far(
            2.4,
            scene(
                "h2o-homo-lumo-translucent",
                H2O,
                &format!(
                    "{AO}[[structures]]\n{}{}",
                    translucent(orbital(5, 1.0, 1, None, false), 0.5, None),
                    translucent(orbital(6, 1.0, 1, None, false), 0.5, Some(("[60, 170, 90, 255]", "[235, 165, 40, 255]")))
                ),
            ),
        ),
        far(1.6, scene("triplet-total-density", H2O_T, &format!("{AO_DOF}[[structures]]\n{}", translucent(density(false), 0.35, None)))),
        // APOST-3D: fragment envelopes, with and without depth of field
        // (the value labels take another path).
        scene("fe-fragments-ao-dof", FE, &format!("{AO_DOF}{FRAGMENTS}")),
        scene("fe-fragments-ao", FE, &format!("{AO}{FRAGMENTS}")),
        scene("fe-fragment-charges", FE, &format!("{AO}[[structures]]\n{}", results("populations.fragments.charges", 0, false, false))),
        scene("fe-atom-charges", FE, &format!("{AO}[[structures]]\n{}", results("populations.atoms.charges", 0, false, false))),
        scene("fe-bond-order-focus", FE, &format!("{AO}[[structures]]\n{}", results("bond_orders.fragments.matrix", 0, true, false))),
        far(1.8, scene("c2h6-iqa-log", C2H6, &format!("{AO}[[structures]]\n{}", results("enpart.total.atoms", 0, false, true)))),
        // Crowded labels: measurements beside their bonds and inside their
        // angles; a value whose atom's center is covered by a hydrogen;
        // values on 83 atoms, overlapping ones moved or left out.
        far(
            1.3,
            scene(
                "bicl3-measurements",
                BICL3,
                &format!(
                    "{AO}[[structures]]\n{}",
                    [[1, 2], [1, 3]].iter().map(|a| format!("[[structures.measurements]]\natoms = {a:?}\nlabel_offset = [0.0, 0.0]\n")).collect::<String>()
                        + &[[2, 1, 3], [3, 1, 4]].iter().map(|a| format!("[[structures.measurements]]\natoms = {a:?}\nlabel_offset = [0.0, 0.0]\n")).collect::<String>()
                ),
            ),
        ),
        scene("ch3f-atom-charges", CH3F, &format!("{AO}[[structures]]\n{}", results("populations.atoms.charges", 0, false, false))),
        scene("bi-eos-atom-charges", BI_EOS, &format!("{AO}[[structures]]\n{}", results("populations.atoms.charges", 0, false, false))),
        // A shared OSLO in its two fragments' colors.
        scene("c2h6-oslo-shared", C2H6_OSLO, &format!("{AO}[[structures]]\n{}", orbital(9, 1.0, 1, Some("oslo"), true))),
    ];
    // The live view: direct draw, with ambient occlusion sampled at the
    // viewport's offset in the window, and the depth-of-field blit.
    for (name, files, patch) in [
        ("window-bi-plain", BI, PLAIN.to_string()),
        ("window-bi-ao", BI, AO.to_string()),
        ("window-bi-ao-dof", BI, AO_DOF.to_string()),
        ("window-h2o-homo-ao", H2O, format!("{AO}[[structures]]\n{}", orbital(5, 1.0, 1, None, false))),
        ("window-fe-fragments", FE, format!("{AO_DOF}{FRAGMENTS}")),
        ("window-h2o-homo-translucent", H2O, format!("{AO}[[structures]]\n{}", translucent(orbital(5, 1.0, 1, None, false), 0.5, None))),
    ] {
        let zoom_out = if files == H2O { 2.4 } else { 1.0 };
        list.push(Scene { window: true, zoom_out, ..scene(name, files, &patch) });
    }
    // Orthographic (parallel rays, the default since 2026-10-22): one of
    // each pipeline, export and live view.
    let orthographic = [
        "bi-ao-dof",
        "bicl3-labels",
        "h2o-homo-lumo-translucent",
        "fe-fragments-ao",
        "fe-atom-charges",
        "c2h6-oslo-shared",
        "window-bi-ao-dof",
        "window-h2o-homo-translucent",
    ];
    let copies: Vec<Scene> = list
        .iter()
        .filter(|s| orthographic.contains(&s.name))
        .map(|s| Scene {
            name: Box::leak(format!("ortho-{}", s.name).into_boxed_str()),
            files: s.files,
            patch: format!("[view]\nprojection = \"orthographic\"\n{}", s.patch),
            select: s.select,
            window: s.window,
            zoom_out: s.zoom_out,
        })
        .collect();
    list.extend(copies);
    list
}

/// Merges `patch` into `base` (see `Scene::patch`).
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

/// Renders one scene as RGBA rows, or `None` when a file is missing.
fn render(scene: &Scene, scratch: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let files: Vec<PathBuf> = scene.files.iter().map(|f| repo_dir().join(f)).collect();
    if let Some(missing) = files.iter().find(|f| !f.is_file()) {
        eprintln!("{}: skipped, {} not found", scene.name, missing.display());
        return None;
    }
    let session = scratch.join(format!("{}.a3dview", scene.name));
    let first = app_with(files);
    first.state().save_session_to(&session).unwrap();
    drop(first);

    let mut value: toml::Value = toml::from_str(&std::fs::read_to_string(&session).unwrap()).unwrap();
    // The scenes were made in perspective (the default until 2026-10-22);
    // the orthographic ones say so in their patch.
    value["view"]["projection"] = toml::Value::String("perspective".into());
    let patch: toml::Value = toml::from_str(&scene.patch).unwrap_or_else(|e| panic!("{}: bad patch: {e}\n{}", scene.name, scene.patch));
    merge(&mut value, &patch);
    let distance = &mut value["view"]["camera_distance"];
    *distance = toml::Value::Float(distance.as_float().unwrap() * f64::from(scene.zoom_out));
    std::fs::write(&session, toml::to_string(&value).unwrap()).unwrap();

    let mut harness = app_with(vec![session]);
    harness.run_steps(4);
    let toasts = harness.state().toast_texts();
    assert!(toasts.iter().all(|t| !t.contains(", but:")), "{}: {toasts:?}", scene.name);
    if !scene.select.is_empty() {
        harness.state_mut().select_atoms(scene.select);
    }
    // Enough frames for surfaces to be extracted and the live ambient
    // occlusion to settle at full quality.
    harness.run_steps(8);
    if scene.window {
        let image = harness.render().expect("render");
        Some((image.width(), image.height(), image.into_raw()))
    } else {
        Some((WIDTH, HEIGHT, harness.state_mut().render_image(WIDTH, HEIGHT).expect("render")))
    }
}

/// Pixels that differ, and the largest difference in any channel.
fn difference(a: &[u8], b: &[u8]) -> (usize, u8) {
    let mut count = 0;
    let mut largest = 0;
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        if p != q {
            count += 1;
            largest = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0).max(largest);
        }
    }
    (count, largest)
}

#[test]
fn reference_scenes() {
    let Some(dir) = std::env::var_os("APOST3DVIEW_REFERENCE_DIR").map(PathBuf::from) else {
        eprintln!("APOST3DVIEW_REFERENCE_DIR not set; reference scenes not rendered");
        return;
    };
    let write = std::env::var("APOST3DVIEW_REFERENCE").is_ok_and(|m| m == "write");
    let scratch = scratch_dir("render-reference");
    std::fs::create_dir_all(&dir).unwrap();
    let compare_dir = dir.join("compare");
    let _ = std::fs::remove_dir_all(&compare_dir);

    let mut report = Vec::new();
    let mut failed = Vec::new();
    for scene in scenes() {
        let Some((width, height, pixels)) = render(&scene, &scratch) else { continue };
        let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
        let path = dir.join(format!("{}.png", scene.name));
        if write {
            image.save(&path).unwrap();
            report.push(format!("{:24} written", scene.name));
            continue;
        }
        let Ok(reference) = image::open(&path).map(|i| i.into_rgba8()) else {
            failed.push(scene.name);
            report.push(format!("{:24} no reference picture", scene.name));
            continue;
        };
        if reference.dimensions() != image.dimensions() {
            failed.push(scene.name);
            report.push(format!("{:24} size {:?}, reference {:?}", scene.name, image.dimensions(), reference.dimensions()));
            continue;
        }
        let (count, largest) = difference(reference.as_raw(), image.as_raw());
        if count == 0 {
            report.push(format!("{:24} identical", scene.name));
            continue;
        }
        failed.push(scene.name);
        report.push(format!("{:24} {count} pixels differ, by up to {largest}/255", scene.name));
        std::fs::create_dir_all(&compare_dir).unwrap();
        image.save(compare_dir.join(format!("{}.png", scene.name))).unwrap();
        // Differing pixels in red, scaled so a difference of 1 shows.
        let diff = image::RgbaImage::from_fn(width, height, |x, y| {
            let (p, q) = (reference.get_pixel(x, y), image.get_pixel(x, y));
            let d = p.0.iter().zip(q.0).map(|(a, b)| a.abs_diff(b)).max().unwrap_or(0);
            if d == 0 {
                image::Rgba([255, 255, 255, 255])
            } else {
                image::Rgba([255, 200u8.saturating_sub(d.saturating_mul(40)), 0, 255])
            }
        });
        diff.save(compare_dir.join(format!("{}-diff.png", scene.name))).unwrap();
    }
    eprintln!("Reference scenes ({}):\n{}", dir.display(), report.join("\n"));
    assert!(failed.is_empty(), "scenes differ from the reference: {failed:?}");
}
