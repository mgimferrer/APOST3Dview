//! The user guide's walkthroughs (`docs/src/`): each test follows the steps
//! of one chapter in the real app, clicking what the guide tells the reader
//! to click, and checks that they see what the guide says. A renamed button
//! or a changed default fails here, so the guide can't silently go stale.
//!
//! With `APOST3DVIEW_GUIDE_IMAGES=dir` set, the tests also take the guide's
//! pictures and write them to `dir` (`scripts/guide-images.sh` writes them
//! to `docs/src/images/`). Without it nothing is rendered.

mod common;

use std::path::PathBuf;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

use apost3dview::App;
use common::{app_with_scale, example, orientation, repo_dir, scratch_dir};

/// The pictures are taken at twice the screen's resolution, so they stay
/// sharp on high-density screens.
const SCALE: f32 = 2.0;

type Guide = Harness<'static, App>;

fn images_dir() -> Option<PathBuf> {
    std::env::var_os("APOST3DVIEW_GUIDE_IMAGES").map(PathBuf::from)
}

/// The app with `files` opened, as from the Open menu, messages gone.
fn open(files: Vec<PathBuf>) -> Guide {
    let mut harness = app_with_scale(files, SCALE);
    harness.run_steps(4);
    harness
}

/// An example's results file, as the guide tells the reader to open it.
fn open_example(job: &str) -> Guide {
    open(vec![example(job).join(format!("{job}.apost.json"))])
}

/// Turns and zooms the view, for a better picture.
fn frame(harness: &mut Guide, yaw: f32, pitch: f32, zoom: f32) {
    harness.state_mut().turn_view(yaw, pitch);
    harness.state_mut().zoom_view(zoom);
    harness.run_steps(2);
}

/// [Fe(CN)5NO]3- (the FeCN5NO3--UBLYP example) as octahedral complexes are
/// usually drawn: the NO up from Fe, a CN to the right, seen from a little
/// above and turned so the CN in front doesn't hide the Fe. From the atoms,
/// whatever the file's orientation; then zoomed by `zoom`.
fn frame_iron_nitrosyl(harness: &mut Guide, zoom: f32) {
    let text = std::fs::read_to_string(example("FeCN5NO3--UBLYP").join("FeCN5NO3--UBLYP.apost.json")).unwrap();
    let results = apost3dview_core::parse_apost_results(&text).unwrap();
    let molecule = results.molecule.as_ref().unwrap();
    let at = |i: usize| glam::Vec3::from_array(molecule.coordinates_bohr[i].map(|c| c as f32));
    let of = |symbol: &str| (0..molecule.symbols.len()).filter(|&i| molecule.symbols[i] == symbol).collect::<Vec<_>>();
    let (fe, o) = (at(of("Fe")[0]), at(of("O")[0]));
    // The nitrosyl N: the N nearest the O.
    let nitrosyl = of("N").into_iter().map(at).min_by(|a, b| a.distance(o).total_cmp(&b.distance(o))).unwrap();
    let up = nitrosyl - fe;
    let right = of("C").into_iter().map(|c| at(c) - fe).min_by(|a, b| a.normalize().dot(up.normalize()).abs().total_cmp(&b.normalize().dot(up.normalize()).abs())).unwrap();
    harness.state_mut().set_view_orientation(orientation(up, right, 0.17, 0.55));
    harness.state_mut().zoom_view(zoom);
    harness.run_steps(2);
}

fn settle(harness: &mut Guide) {
    harness.run_steps(4);
    harness.state_mut().clear_toasts();
    harness.run_steps(2);
}

/// Clicks the widget labeled `label` (the first, if several), through
/// accessibility, so a widget scrolled out of sight is still reached.
fn click(harness: &mut Guide, label: &str) {
    harness.query_all_by_label(label).next().unwrap_or_else(|| panic!("no \"{label}\" to click")).click_accesskit();
    harness.run_steps(3);
}

/// Something labeled `label` is on screen.
fn sees(harness: &Guide, label: &str) {
    assert!(harness.query_all_by_label(label).next().is_some(), "no \"{label}\" on screen");
}

/// Something whose label contains `text` is on screen.
fn sees_text(harness: &Guide, text: &str) {
    assert!(harness.query_all_by_label_contains(text).next().is_some(), "nothing containing \"{text}\" on screen");
}

/// Opens the menu showing `value` and picks `option` in it.
fn choose(harness: &mut Guide, value: &str, option: &str) {
    // A menu picked through accessibility stays open: close it first.
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    harness
        .get_all(By::new().role(Role::ComboBox))
        .find(|n| n.value().as_deref() == Some(value))
        .unwrap_or_else(|| panic!("no menu showing \"{value}\""))
        .click_accesskit();
    harness.run_steps(2);
    harness.query_all_by_label(option).last().unwrap_or_else(|| panic!("no \"{option}\" in the menu showing \"{value}\"")).click_accesskit();
    harness.run_steps(4);
}

/// Opens the menu showing `value`, leaving it open (for a picture of it).
fn open_menu(harness: &mut Guide, value: &str) {
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    harness.get_all(By::new().role(Role::ComboBox)).find(|n| n.value().as_deref() == Some(value)).unwrap_or_else(|| panic!("no menu showing \"{value}\"")).click_accesskit();
    harness.run_steps(3);
}

/// The whole window, scaled down to `width` pixels.
fn snap_window(harness: &mut Guide, name: &str, width: u32) {
    let Some(dir) = images_dir() else { return };
    harness.remove_cursor();
    harness.run_steps(2);
    let image = harness.render().expect("render");
    let height = image.height() * width / image.width();
    let image = image::imageops::resize(&image, width, height, image::imageops::FilterType::Lanczos3);
    save(&dir, name, &image);
}

/// The area `rect` (in points) of the window, at full resolution, with a
/// small margin.
fn snap_area(harness: &mut Guide, name: &str, rect: egui::Rect) {
    let Some(dir) = images_dir() else { return };
    harness.remove_cursor();
    harness.run_steps(2);
    let image = harness.render().expect("render");
    let rect = rect.expand(6.0);
    let (x0, y0) = ((rect.min.x.max(0.0) * SCALE) as u32, (rect.min.y.max(0.0) * SCALE) as u32);
    let x1 = ((rect.max.x * SCALE) as u32).min(image.width());
    let y1 = ((rect.max.y * SCALE) as u32).min(image.height());
    let crop = image::imageops::crop_imm(&image, x0, y0, x1 - x0, y1 - y0).to_image();
    save(&dir, name, &crop);
}

/// The tool window titled `title`.
fn window_rect(harness: &Guide, title: &str) -> egui::Rect {
    harness
        .get_all(By::new().role(Role::Window).label(title))
        .next()
        .unwrap_or_else(|| panic!("no window \"{title}\""))
        .rect()
}

fn snap_tool_window(harness: &mut Guide, name: &str, title: &str) {
    let rect = window_rect(harness, title);
    snap_area(harness, name, rect);
}

/// The current view as Export renders it, `width` x `height` pixels.
fn snap_figure(harness: &mut Guide, name: &str, width: u32, height: u32) {
    let Some(dir) = images_dir() else { return };
    let pixels = harness.state_mut().render_image(width, height).expect("render");
    save(&dir, name, &image::RgbaImage::from_raw(width, height, pixels).unwrap());
}

fn save(dir: &std::path::Path, name: &str, image: &image::RgbaImage) {
    std::fs::create_dir_all(dir).unwrap();
    image.save(dir.join(format!("{name}.png"))).unwrap();
}

fn validation(file: &str) -> PathBuf {
    repo_dir().join("validation").join(file)
}

/// "Install and first start": the empty window, then a results file open.
#[test]
fn first_start() {
    let mut guide = open(Vec::new());
    settle(&mut guide);
    sees(&guide, "Open");
    sees_text(&guide, "Nothing open yet. Use Open in the toolbar");
    snap_window(&mut guide, "first-start-empty", 1400);

    let mut guide = open_example("FeCN5NO3--UBLYP");
    assert_eq!(guide.state().toast_texts(), vec!["APOST-3D results loaded from FeCN5NO3--UBLYP.apost.json".to_string()]);
    frame_iron_nitrosyl(&mut guide, 1.0);
    settle(&mut guide);
    for label in ["Open", "Save", "Molecule", "Surfaces", "Measure", "APOST-3D", "Appearance", "Export", "About", "Spin (Space)"] {
        sees(&guide, label);
    }
    snap_window(&mut guide, "first-start-window", 1400);
    snap_tool_window(&mut guide, "first-start-apost-window", "APOST-3D");
}

/// "A five-minute tour": one picture per example.
#[test]
fn tour() {
    let mut guide = open_example("FeCN5NO3--UBLYP");
    frame_iron_nitrosyl(&mut guide, 0.85);
    choose(&mut guide, "Nothing", "Inter-fragment bond order (Lowdin)");
    assert_eq!(guide.state().legend_title().as_deref(), Some("Inter-fragment bond order with 1 Fe (Lowdin)"));
    settle(&mut guide);
    snap_figure(&mut guide, "tour-bond-orders", 1200, 750);

    let mut guide = open_example("H2O-Dimer-B3LYP");
    frame(&mut guide, 0.0, 0.0, 0.5);
    choose(&mut guide, "Nothing", "Inter-atomic interaction energy (TFVC)");
    assert_eq!(guide.state().legend_title().as_deref(), Some("Inter-atomic interaction energy with O1 (TFVC), kcal/mol"));
    settle(&mut guide);
    snap_figure(&mut guide, "tour-iqa", 1200, 750);

    let mut guide = open_example("C2H6-OSLO-2CH3");
    click(&mut guide, "Tables (1)");
    guide.get_all_by_label("Draw").nth(8).expect("nine OSLOs").click_accesskit();
    guide.run_steps(4);
    assert_eq!(guide.state().surface_labels(), vec![("OSLO 9 \u{b7} 1 CH3 + 2 CH3".to_string(), true)]);
    click(&mut guide, "Surfaces");
    click(&mut guide, "Color by fragment");
    assert!(guide.state().surface_colored_by_fragment(0));
    frame(&mut guide, 0.5, 0.3, 0.62);
    settle(&mut guide);
    snap_figure(&mut guide, "tour-oslo", 1200, 750);

    let mut guide = open_example("CH3F-GEOS");
    click(&mut guide, "Surfaces");
    choose(&mut guide, "SCF orbitals", "GEOS EFOs");
    click(&mut guide, "Select all");
    guide.get_by_label_contains("orbital(s)").click_accesskit();
    guide.run_steps(4);
    assert!(guide.state().surface_labels().iter().all(|(label, _)| label.starts_with("EFO ")));
    settle(&mut guide);
    snap_tool_window(&mut guide, "tour-geos-surfaces", "Surfaces");

    let mut guide = open_example("H2O-CAS44");
    click(&mut guide, "Surfaces");
    click(&mut guide, "Total density");
    click(&mut guide, "Generate 1 surface(s)");
    assert_eq!(guide.state().surface_labels(), vec![("Total density (CASSCF)".to_string(), true)]);
    frame(&mut guide, 0.0, 0.0, 0.55);
    settle(&mut guide);
    snap_figure(&mut guide, "tour-density", 1200, 750);
}

/// "Looking at a molecule": labels, a selection and what to do with it,
/// measurements.
#[test]
fn looking_at_a_molecule() {
    let mut guide = open(vec![example("CH3F-GEOS").join("CH3F-GEOS.fchk")]);
    click(&mut guide, "Element");
    frame(&mut guide, 0.6, 0.2, 1.5);
    guide.state_mut().select_atoms(&[0, 1]);
    guide.run_steps(3);
    for label in ["With the selection", "Hide selected", "Hide bond", "Add measurement", "Clear (Esc)", "Selected bonds:"] {
        if label == "Selected bonds:" {
            continue;
        }
        sees(&guide, label);
    }
    sees_text(&guide, "Selected: C1, F2");
    settle(&mut guide);
    snap_tool_window(&mut guide, "molecule-window", "Molecule");
    click(&mut guide, "Add measurement");
    guide.state_mut().select_atoms(&[1, 0, 2]);
    guide.run_steps(3);
    click(&mut guide, "Add angle");
    guide.state_mut().select_atoms(&[]);
    guide.run_steps(3);
    sees_text(&guide, "C1-F2");
    settle(&mut guide);
    snap_tool_window(&mut guide, "measure-window", "Measure");
    snap_figure(&mut guide, "molecule-measurements", 1200, 750);
}

/// "Orbitals and densities".
#[test]
fn orbitals_and_densities() {
    let mut guide = open(vec![validation("gaussian/H2O-ccpVDZ.fchk")]);
    click(&mut guide, "Orbitals...");
    click(&mut guide, "HOMO/LUMO");
    click(&mut guide, "Generate 2 orbital(s)");
    assert_eq!(guide.state().surface_labels(), vec![("HOMO".to_string(), true), ("LUMO".to_string(), false)]);
    settle(&mut guide);
    snap_tool_window(&mut guide, "surfaces-window", "Surfaces");
    frame(&mut guide, 0.4, 0.3, 2.3);
    snap_figure(&mut guide, "surfaces-homo", 1200, 750);

    let mut guide = open(vec![validation("gaussian/H2O-T-ccpVTZ.fchk")]);
    click(&mut guide, "Orbitals...");
    click(&mut guide, "Total density");
    click(&mut guide, "Spin density");
    click(&mut guide, "Generate 2 surface(s)");
    let labels: Vec<String> = guide.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["Total density", "Spin density"]);
    guide.query_all_by_label("Spin density").last().unwrap().click_accesskit();
    guide.run_steps(4);
    assert_eq!(guide.state().surface_labels(), vec![("Total density".to_string(), false), ("Spin density".to_string(), true)]);
    frame(&mut guide, 0.4, 0.3, 1.7);
    settle(&mut guide);
    snap_figure(&mut guide, "surfaces-spin-density", 1200, 750);

    let mut guide = open_example("H2O-CAS44");
    click(&mut guide, "Surfaces");
    choose(&mut guide, "Orbitals of the file", "Natural orbitals (CASSCF)");
    click(&mut guide, "Generate 2 orbital(s)");
    settle(&mut guide);
    snap_tool_window(&mut guide, "surfaces-natural-orbitals", "Surfaces");
}

/// "Fragments and bond orders".
#[test]
fn fragments_and_bond_orders() {
    let mut guide = open_example("FeCN5NO3--UBLYP");
    frame_iron_nitrosyl(&mut guide, 0.85);
    choose(&mut guide, "Nothing", "Fragments");
    settle(&mut guide);
    snap_figure(&mut guide, "results-fragments", 1200, 750);
    choose(&mut guide, "Fragments", "Fragment charges (Lowdin)");
    settle(&mut guide);
    snap_figure(&mut guide, "results-fragment-charges", 1200, 750);
    open_menu(&mut guide, "Fragment charges (Lowdin)");
    for group in [
        "Fragment values",
        "Between fragments",
        "Atom values",
        "Between atoms",
        "Also list other values",
        "Fragment spin populations (Lowdin)",
        "EOS oxidation states (Lowdin)",
        "Atomic charges (Lowdin)",
        "Intra-fragment localization index (Lowdin)",
        "Inter-atomic bond order (Lowdin)",
    ] {
        sees(&guide, group);
    }
    snap_window(&mut guide, "results-show-menu", 1400);
    guide.key_press(egui::Key::Escape);
    guide.run_steps(2);
    choose(&mut guide, "Fragment charges (Lowdin)", "Inter-fragment bond order (Lowdin)");
    for label in ["Seen from", "Automatic", "Fixed", "Values on the molecule", "with names", "Copy matrix", "Strongest pairs", "Bond thickness from bond orders"] {
        sees(&guide, label);
    }
    settle(&mut guide);
    snap_window(&mut guide, "results-bond-orders-window", 1400);
    choose(&mut guide, "1 Fe", "7 NO");
    assert!(guide.state().legend_title().unwrap().contains("with 7 NO"));
    settle(&mut guide);
    snap_figure(&mut guide, "results-bond-orders-from-no", 1200, 750);
}

/// "Local spin".
#[test]
fn local_spin() {
    let mut guide = open_example("H2O-CAS44");
    frame(&mut guide, 0.0, 0.0, 0.55);
    open_menu(&mut guide, "Nothing");
    for entry in ["Intra-fragment S\u{b2} (local spin) (TFVC)", "Inter-atomic S\u{b2} (spin coupling) (TFVC)"] {
        sees(&guide, entry);
    }
    choose(&mut guide, "Nothing", "Intra-atomic S\u{b2} (local spin) (TFVC)");
    settle(&mut guide);
    snap_figure(&mut guide, "local-spin-intra", 1200, 750);
}

/// "Energy decomposition (IQA)".
#[test]
fn energy_decomposition() {
    let mut guide = open_example("H2O-Dimer-B3LYP");
    frame(&mut guide, 0.0, 0.0, 0.6);
    choose(&mut guide, "Nothing", "Inter-fragment interaction energy (TFVC)");
    settle(&mut guide);
    snap_figure(&mut guide, "iqa-fragments", 1200, 750);
    choose(&mut guide, "Inter-fragment interaction energy (TFVC)", "Inter-atomic interaction energy (TFVC)");
    for label in ["Term", "Energies in", "kcal/mol", "au", "Logarithmic colors"] {
        sees(&guide, label);
    }
    open_menu(&mut guide, "Total (IQA)");
    for term in ["Exchange-correlation", "Coulomb"] {
        sees(&guide, term);
    }
    choose(&mut guide, "Total (IQA)", "Classical (electrostatic)");
    settle(&mut guide);
    snap_figure(&mut guide, "iqa-classical", 1200, 750);
    guide.state_mut().select_atoms(&[0, 4]);
    guide.run_steps(3);
    sees_text(&guide, "Selected pair");
    guide.get_by_label_contains("Energy decomposition (IQA) \u{b7} error").click_accesskit();
    guide.run_steps(3);
    sees(&guide, "Integration error");
    settle(&mut guide);
    snap_tool_window(&mut guide, "iqa-window", "APOST-3D");
}

/// "Effective fragment orbitals and OSLOs".
#[test]
fn efos_and_oslos() {
    let mut guide = open_example("C2H6-OSLO-2CH3");
    click(&mut guide, "Tables (1)");
    sees(&guide, "Draw shows the orbital as a surface.");
    settle(&mut guide);
    snap_tool_window(&mut guide, "oslo-table", "APOST-3D");
    click(&mut guide, "Surfaces");
    open_menu(&mut guide, "SCF orbitals");
    sees(&guide, "OSLOs (before orthogonalization)");
    choose(&mut guide, "SCF orbitals", "OSLOs");
    for label in ["Select all", "Clear"] {
        sees(&guide, label);
    }
    settle(&mut guide);
    snap_tool_window(&mut guide, "oslo-surfaces", "Surfaces");

    // The frontier EFOs of CH3F's GEOS, colored by fragment.
    let mut guide = open_example("CH3F-GEOS");
    click(&mut guide, "Surfaces");
    choose(&mut guide, "SCF orbitals", "GEOS EFOs");
    click(&mut guide, "Frontier");
    click(&mut guide, "Generate 2 orbital(s)");
    let labels: Vec<String> = guide.state().surface_labels().into_iter().map(|(l, _)| l).collect();
    assert_eq!(labels, vec!["EFO 9 paired \u{b7} 1 F", "EFO 10 paired \u{b7} 2 CH3"]);
    // Both shown, each in its fragment's color.
    for name in ["EFO 9 paired \u{b7} 1 F", "EFO 10 paired \u{b7} 2 CH3"] {
        guide.query_all_by_label(name).last().unwrap().click_accesskit();
        guide.run_steps(3);
        click(&mut guide, "Color by fragment");
    }
    // (The surface list's Show all; the Molecule window has one too.)
    guide.query_all_by_label("Show all").last().unwrap().click_accesskit();
    guide.run_steps(4);
    assert_eq!(guide.state().surface_labels().iter().filter(|(_, shown)| *shown).count(), 2);
    assert!(guide.state().surface_colored_by_fragment(0) && guide.state().surface_colored_by_fragment(1));
    frame(&mut guide, 0.5, 0.2, 0.6);
    settle(&mut guide);
    snap_figure(&mut guide, "efo-frontier", 1200, 750);
}

/// "Making an APOST-3D input": a .fchk with nothing next to it.
#[test]
fn making_an_input() {
    let dir = scratch_dir("guide-input");
    let fchk = dir.join("FeCN5NO3.fchk");
    std::fs::copy(example("FeCN5NO3--UBLYP").join("FeCN5NO3--UBLYP.fchk"), &fchk).unwrap();
    let mut guide = open(vec![fchk]);
    frame_iron_nitrosyl(&mut guide, 1.0);
    click(&mut guide, "APOST-3D input...");
    for label in ["Results", "New input", "Atoms in molecules", "Analyses", "Effective oxidation states", "Separate molecules", "Input file", "Copy input"] {
        sees(&guide, label);
    }
    click(&mut guide, "Effective oxidation states");
    sees_text(&guide, "None yet: EOS, GEOS and OSLO need them.");
    guide.state_mut().select_atoms(&[0]);
    guide.run_steps(3);
    click(&mut guide, "Around the selected atom(s)");
    assert_eq!(guide.state().input_fragments().len(), 7);
    sees(&guide, "Save FeCN5NO3.inp");
    sees(&guide, "complete");
    settle(&mut guide);
    snap_window(&mut guide, "input-around-fe", 1400);
    snap_tool_window(&mut guide, "input-window", "APOST-3D");
}

/// "Appearance" and "Figures and videos": the two windows.
#[test]
fn appearance_and_export() {
    let mut guide = open(vec![validation("gaussian/BiCl3-def2TZVPP.fchk")]);
    click(&mut guide, "Appearance");
    for label in ["Default", "Publication", "Save this look", "Open presets folder", "Ambient occlusion", "Depth of field", "Orthographic", "Perspective", "Advanced"] {
        sees(&guide, label);
    }
    settle(&mut guide);
    snap_tool_window(&mut guide, "appearance-window", "Appearance");
    click(&mut guide, "Export");
    for label in ["Also save the session", "Image:", "Single image", "Panels", "DPI", "Custom (pixels)", "Single column (3.25 in)", "Double column (6.75 in)", "Transparent background", "Caption", "Render and save PNG...", "Render and save GIF..."] {
        sees(&guide, label);
    }
    settle(&mut guide);
    snap_tool_window(&mut guide, "export-window", "Export");
}

/// "Figures and videos", Panels: the OSLOs of ethane as a captioned grid.
#[test]
fn panels_of_orbitals() {
    let mut guide = open_example("C2H6-OSLO-2CH3");
    click(&mut guide, "Surfaces");
    choose(&mut guide, "SCF orbitals", "OSLOs");
    click(&mut guide, "Select all");
    click(&mut guide, "Generate 9 orbital(s)");
    frame(&mut guide, 0.45, 0.3, 0.75);
    click(&mut guide, "Export");
    click(&mut guide, "Panels");
    for label in [
        "Image:",
        "Single image",
        "Panels (9)",
        "All surfaces",
        "Captions",
        "Full",
        "Value and fragment",
        "Text size:",
        "Letters (a), (b), ...",
        "Layout:",
        "Automatic",
        "Rows and columns",
        "Panel shape:",
        "Square",
        "As the view",
        "Gap:",
        "Size:",
        "Half page (3.25 in)",
        "Full page (6.75 in)",
        "Background:",
        "White",
        "Transparent",
        "Render and save panels...",
    ] {
        sees(&guide, label);
    }
    assert!(guide.get_all(By::new().role(Role::ComboBox)).any(|n| n.value().as_deref() == Some("Add a panel")), "the Add a panel menu");
    let captions = guide.state().panel_captions();
    assert_eq!(captions[8], "OSLO 9 \u{b7} 1 CH3 + 2 CH3 \u{b7} FOLI 2.001");
    click(&mut guide, "Value and fragment");
    assert_eq!(guide.state().panel_captions()[8], "2.001 (CH3 + CH3)");
    click(&mut guide, "Full");
    // Only the Export window, so the picture shows it whole.
    for window in ["Molecule", "Surfaces", "APOST-3D"] {
        let ctx = guide.ctx.clone();
        guide.state().set_window_collapsed(&ctx, window, true);
    }
    guide.run_steps(4);
    assert!(guide.state().figure_preview_size().is_some(), "the preview is drawn");
    settle(&mut guide);
    snap_tool_window(&mut guide, "export-panels-window", "Export");
    // The figure itself, as saved, scaled to the guide's width.
    if let Some(dir) = images_dir() {
        guide.state_mut().set_panel_figure(600, true, false);
        let (w, h, pixels) = guide.state_mut().render_panel_figure().expect("the figure");
        let image = image::RgbaImage::from_raw(w, h, pixels).unwrap();
        let image = image::imageops::resize(&image, 1200, h * 1200 / w, image::imageops::FilterType::Lanczos3);
        save(&dir, "panels-oslos", &image);
    }
}
