use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apost3dview_core::{
    element_data, extract_isosurface, format_coordinates, generate_mo_grids, measure, parse_cube, parse_fchk_wavefunction, parse_xyz, refine_grid,
    Bond, CoordinateFormat, LengthUnit, MeasurementKind, Molecule, MolecularOrbitals, Wavefunction,
};
use apost3dview_render::{
    glyph_scale_for_font_size, glyph_scale_for_world_size, layout_label, pick_atom, pick_bond, push_isosurface_vertices, ray_from_ndc,
    AoSettings, BondVisualStyle, DofSettings, ExportSettings, GlyphAtlas, GlyphInstance, IsosurfaceMaterial, IsosurfaceVertex, Material,
    OrbitCamera, SceneUniforms, ViewportCallback, ViewportResources,
};
use egui::{Color32, Slider};
use glam::{Vec3, Vec4};

/// Minimum time the splash screen stays up, regardless of how fast startup
/// actually finishes.
const SPLASH_MIN_DURATION: Duration = Duration::from_secs(4);

const WARNING_DURATION: Duration = Duration::from_secs(3);

fn load_texture(ctx: &egui::Context, name: &str, png_bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(png_bytes).expect("bundled image should be valid").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

/// One spin channel's tick-box list in the "Generate orbitals" section —
/// factored out since alpha and beta need identical rendering, just
/// against a different `MolecularOrbitals`/selection set. `spin_label`
/// ("alpha"/"beta") is folded into each row's occupancy tag rather than
/// shown as a separate heading, and into the `ScrollArea`'s id so two
/// instances (restricted files only ever show one) don't collide.
fn show_mo_checklist(ui: &mut egui::Ui, orbitals: &MolecularOrbitals, selected: &mut HashSet<usize>, spin_label: &str, height: f32) {
    let homo = orbitals.homo_index();
    let lumo = orbitals.lumo_index();
    egui::ScrollArea::vertical().id_salt(("mo_checklist", spin_label)).max_height(height).show(ui, |ui| {
        for mo_index in 0..orbitals.num_orbitals() {
            let mo_number = mo_index + 1;
            let energy = orbitals.orbital_energies[mo_index];
            let occ_tag = if mo_number <= orbitals.num_occupied { "occ" } else { "virt" };
            let special_tag = if mo_number == homo {
                "  HOMO"
            } else if mo_number == lumo {
                "  LUMO"
            } else {
                ""
            };
            let mut checked = selected.contains(&mo_index);
            if ui.checkbox(&mut checked, format!("MO {mo_number}  {energy:+.4} Hartree  ({spin_label}, {occ_tag}){special_tag}")).changed() {
                if checked {
                    selected.insert(mo_index);
                } else {
                    selected.remove(&mo_index);
                }
            }
        }
    });
}

fn toggle_selected(list: &mut Vec<usize>, index: usize) {
    if let Some(pos) = list.iter().position(|&i| i == index) {
        list.remove(pos);
    } else {
        list.push(index);
    }
}

fn find_bond_between(molecule: &Molecule, a: usize, b: usize) -> Option<usize> {
    molecule.bonds.iter().position(|bond| (bond.atom_a == a && bond.atom_b == b) || (bond.atom_a == b && bond.atom_b == a))
}

fn format_measurement(kind: MeasurementKind, value: f32, unit: LengthUnit, decimals: usize) -> String {
    match kind {
        MeasurementKind::Distance(..) => {
            let converted = unit.from_angstrom(value as f64);
            let unit_label = match unit {
                LengthUnit::Angstrom => "Å",
                LengthUnit::Bohr => "a.u.",
            };
            format!("{converted:.decimals$} {unit_label}")
        }
        MeasurementKind::Angle(..) | MeasurementKind::Dihedral(..) => format!("{value:.decimals$}\u{b0}"),
    }
}

/// "C1-N3-O5" style tag for the measurement list, so entries with
/// identical values (a common occurrence — symmetric structures, several
/// similar bonds) are still distinguishable at a glance.
fn format_measurement_atoms(molecule: &Molecule, kind: MeasurementKind) -> String {
    kind.atoms()
        .iter()
        .map(|&i| format!("{}{}", element_data(molecule.atomic_numbers[i]).symbol, i + 1))
        .collect::<Vec<_>>()
        .join("-")
}

/// Centroid of a measurement's involved atoms — the default label anchor
/// before the user's own drag offset is applied.
fn measurement_anchor(molecule: &Molecule, kind: MeasurementKind) -> Vec3 {
    let atoms = kind.atoms();
    let sum = atoms.iter().fold(Vec3::ZERO, |acc, &i| acc + molecule.positions[i]);
    sum / atoms.len() as f32
}

/// Projects a world-space point through the camera into screen pixels
/// within `rect`. `None` if it's behind the camera.
fn project_to_screen(camera: &OrbitCamera, aspect_ratio: f32, rect: egui::Rect, world_pos: Vec3) -> Option<egui::Pos2> {
    let clip = camera.view_projection_matrix(aspect_ratio) * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 1e-4 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let x = rect.left() + (ndc_x * 0.5 + 0.5) * rect.width();
    let y = rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height();
    Some(egui::pos2(x, y))
}

fn color32_to_rgb(color: Color32) -> [f32; 3] {
    [color.r() as f32 / 255.0, color.g() as f32 / 255.0, color.b() as f32 / 255.0]
}

/// Whether two molecules describe the same geometry (same atoms, same
/// positions within a small tolerance) — used to decide whether switching
/// the active structure should reframe the camera. `.cube` files sharing
/// one `.xyz`/`.fchk` geometry (several orbitals of one molecule) are the
/// motivating case: switching between them should feel like changing a
/// setting, not opening a different structure.
fn molecules_share_geometry(a: &Molecule, b: &Molecule) -> bool {
    if a.atomic_numbers != b.atomic_numbers {
        return false;
    }
    a.positions.iter().zip(&b.positions).all(|(pa, pb)| pa.distance(*pb) < 1e-3)
}

/// SDF edge-threshold constants controlling stroke weight (see
/// `GlyphInstance::edge_bias`) — lower reads as thicker, since more of the
/// distance field counts as "inside" the glyph. Sampling the field once at
/// a shifted threshold gives a uniformly thicker, still crisply
/// antialiased stroke; the previous approach (stamping several offset
/// copies of the glyph) visibly pixelated once labels could be zoomed in
/// this close, since each copy's edge lands on a different sub-pixel and
/// their overlap dithers instead of blending.
const EDGE_BIAS_NORMAL: f32 = 0.5;
const EDGE_BIAS_BOLD: f32 = 0.38;
/// Atom labels have no weight toggle — they're always rendered noticeably
/// thicker than plain body text so a single digit reads clearly against a
/// CPK-colored sphere.
const EDGE_BIAS_ATOM_LABEL: f32 = 0.30;

/// Lays out `text` as 3D glyph instances anchored at `anchor`, appending to
/// `instances`.
fn push_label(instances: &mut Vec<GlyphInstance>, atlas: &GlyphAtlas, text: &str, anchor: Vec3, scale: f32, color: [f32; 3], edge_bias: f32) {
    instances.extend(layout_label(atlas, text, anchor, scale, color, edge_bias));
}

/// What a left-click in the viewport does. Right-click always orbits, so
/// there's no need for a separate "off" state here — `Select` (atom if
/// one's under the cursor, else a bond) is the permanent default; `Measure`
/// is the one explicit override, toggled from the Analysis window, since
/// its click semantics genuinely differ (an ordered pick sequence, not a
/// toggle-selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionMode {
    Select,
    Measure,
}

/// A committed distance/angle/dihedral, with a screen-space label position
/// the user can drag away from the default (the involved atoms' centroid)
/// — for composing publication images without labels overlapping.
struct Measurement {
    kind: MeasurementKind,
    label_offset: egui::Vec2,
}

/// Shared across every measurement/structure — tune once, applies
/// everywhere, same reasoning as Style/Material.
struct MeasurementStyle {
    font_size: f32,
    decimal_places: usize,
    text_color: Color32,
    line_color: [f32; 3],
    bold: bool,
}

impl Default for MeasurementStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            decimal_places: 2,
            text_color: Color32::from_rgb(20, 20, 20),
            line_color: [0.05, 0.05, 0.05],
            bold: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomLabelMode {
    None,
    /// 1-based index, matching the XYZ window's atom numbering.
    Number,
    /// Element symbol only — H, C, Bi, ...
    Type,
    /// Symbol + number — C1, C2, H1, ... the common convention.
    NumberType,
}

/// Shared across every structure, same reasoning as MeasurementStyle.
/// Separate from it rather than reusing it — different typical needs (an
/// atom label sits on top of a CPK-colored sphere rather than on empty
/// space, so it usually wants a smaller size and its own contrast choice).
struct AtomLabelStyle {
    /// Label height as a fraction of the atom's own rendered radius. A
    /// real world-space size (not a screen-pixel one) so labels are true
    /// 3D geometry that scales with zoom exactly like the atom they're
    /// attached to, instead of holding a constant on-screen size the way
    /// a 2D overlay would.
    relative_size: f32,
    text_color: Color32,
}

impl Default for AtomLabelStyle {
    fn default() -> Self {
        Self { relative_size: 1.0, text_color: Color32::BLACK }
    }
}

/// Two ways to size a render: `Dpi` (the default) asks for a figure width
/// in inches and a DPI instead of a bare pixel count, matching how
/// journals actually specify figure requirements, then derives pixel
/// dimensions and embeds the physical size in the PNG itself (see
/// `App::write_png`) so image editors and submission systems read the
/// correct size automatically. `Custom` is the escape hatch for exact
/// pixel dimensions where DPI doesn't apply (slides, web, an exact spec
/// from somewhere that isn't inches-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderPreset {
    Dpi,
    Custom,
}

/// 300 DPI is the common journal minimum, but Martí found renders at 1000
/// DPI (2026-08-28, once AO/depth-of-field gave the image real depth to
/// resolve) look noticeably better, so that's the actual default now — the
/// figure-width quick-set buttons below are typical single/double-column
/// widths, still freely editable, and the DPI field itself stays
/// user-adjustable down to 300 or below for anyone who wants a smaller file.
const DEFAULT_PUBLICATION_DPI: u32 = 1000;
const FIGURE_WIDTH_SINGLE_COLUMN_IN: f64 = 3.25;
const FIGURE_WIDTH_DOUBLE_COLUMN_IN: f64 = 6.75;
/// Renders internally at this multiple of the final pixel size and box-
/// downsamples back — smooths edges beyond the live view's real-time MSAA,
/// independent of whatever DPI/size is chosen.
const PUBLICATION_SUPERSAMPLE: u32 = 2;

struct RenderExportState {
    preset: RenderPreset,
    custom_width: u32,
    custom_height: u32,
    custom_supersample: u32,
    transparent_background: bool,
    dpi: u32,
    figure_width_in: f64,
}

impl Default for RenderExportState {
    fn default() -> Self {
        Self {
            preset: RenderPreset::Dpi,
            custom_width: 1920,
            custom_height: 1080,
            custom_supersample: 2,
            transparent_background: false,
            dpi: DEFAULT_PUBLICATION_DPI,
            figure_width_in: FIGURE_WIDTH_SINGLE_COLUMN_IN,
        }
    }
}

/// Grid-density tier for generating an orbital's `.cube`-equivalent grid
/// straight from a `.fchk` wavefunction — the same one-click-preset-or-
/// exact-control shape as `RenderPreset`. Values are grid spacing in
/// Bohr (native to the GTO evaluator); Medium roughly matches Chemcraft's
/// own default spacing (~0.29 Bohr) for a familiar-looking result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrbitalAccuracyPreset {
    Low,
    Medium,
    High,
    Custom,
}

const ORBITAL_SPACING_LOW_BOHR: f64 = 0.40;
const ORBITAL_SPACING_MEDIUM_BOHR: f64 = 0.28;
const ORBITAL_SPACING_HIGH_BOHR: f64 = 0.15;
/// Padding around the wavefunction's own shell centers on every side —
/// enough for an orbital's decaying tail to read as fully closed rather
/// than clipped at the grid boundary, matching the kind of margin
/// `cubegen`/Chemcraft/APOST-3D use by default.
const ORBITAL_GRID_PADDING_BOHR: f64 = 4.0;

struct OrbitalGenerationState {
    preset: OrbitalAccuracyPreset,
    custom_spacing_bohr: f64,
    /// 1-based, inclusive — state for the "Select range" convenience
    /// button in the MO picker.
    range_start: usize,
    range_end: usize,
    /// Applied to every orbital's `IsosurfaceState` at generation time —
    /// set once up front rather than per-orbital after the fact, since
    /// changing it one-by-one across e.g. 20 freshly generated structures
    /// would be tedious.
    isovalue: f32,
}

impl Default for OrbitalGenerationState {
    fn default() -> Self {
        Self {
            preset: OrbitalAccuracyPreset::Medium,
            custom_spacing_bohr: ORBITAL_SPACING_MEDIUM_BOHR,
            range_start: 1,
            range_end: 1,
            isovalue: DEFAULT_ISOSURFACE_ISOVALUE,
        }
    }
}

impl OrbitalGenerationState {
    fn resolve_spacing_bohr(&self) -> f64 {
        match self.preset {
            OrbitalAccuracyPreset::Low => ORBITAL_SPACING_LOW_BOHR,
            OrbitalAccuracyPreset::Medium => ORBITAL_SPACING_MEDIUM_BOHR,
            OrbitalAccuracyPreset::High => ORBITAL_SPACING_HIGH_BOHR,
            OrbitalAccuracyPreset::Custom => self.custom_spacing_bohr.max(0.01),
        }
    }
}

/// Per-structure isosurface state — only present for structures opened
/// from a `.cube` file. Isovalue/refinement/both-signs changes don't
/// re-extract automatically (marching tetrahedra over a refined grid is a
/// real cost, seconds in a debug build) — only an explicit "Update
/// surface" click (or the one-time automatic extraction the first time a
/// cube structure becomes active) does, tracked via `extracted`.
struct IsosurfaceState {
    grid: apost3dview_core::ScalarGrid,
    show: bool,
    isovalue: f32,
    /// 1x/2x/3x — see `apost3dview_core::refine_grid`.
    refinement: usize,
    both_signs: bool,
    positive_color: Color32,
    negative_color: Color32,
    /// Always applied directly — no separate "Transparent" toggle (there
    /// used to be one, gating a fully-opaque override that was also the
    /// default, which made the toggle+slider pair pure redundant
    /// complexity: 1.0 here already means the same "fully opaque" thing).
    /// Defaults fully opaque — Martí's explicit preference after trying
    /// the translucent look live (2026-08-29): with two competing lobes
    /// this size, one at less than 1.0 doesn't read as an appealing
    /// translucent field so much as a washed-out, lower-contrast version
    /// of the opaque look. Free to lower for anyone who does want to see
    /// through it.
    opacity: f32,
    cached_positive: Option<apost3dview_core::IsosurfaceMesh>,
    cached_negative: Option<apost3dview_core::IsosurfaceMesh>,
    extracted: bool,
}

/// Isovalue is a fixed constant rather than derived from each grid's own
/// data (an earlier approach: 25% of the grid's max |value|) — Martí's
/// explicit preference after tuning it live against the real EFFAO test
/// cubes, simpler and more predictable than a per-file heuristic.
const DEFAULT_ISOSURFACE_ISOVALUE: f32 = 0.075;
/// Fully opaque by default — tried translucent (0.5, then 0.75) first,
/// but a real hands-on check (2026-08-29) came back preferring solid
/// lobes; the opacity slider (Visualization panel) is still there for
/// anyone who wants to see through them.
const DEFAULT_ISOSURFACE_OPACITY: f32 = 1.0;

impl IsosurfaceState {
    fn new(grid: apost3dview_core::ScalarGrid) -> Self {
        Self {
            grid,
            show: true,
            isovalue: DEFAULT_ISOSURFACE_ISOVALUE,
            refinement: 1,
            both_signs: true,
            positive_color: Color32::from_rgb(60, 90, 230),
            negative_color: Color32::from_rgb(220, 70, 60),
            opacity: DEFAULT_ISOSURFACE_OPACITY,
            cached_positive: None,
            cached_negative: None,
            extracted: false,
        }
    }

    /// Resets every tunable appearance setting back to the defaults —
    /// the Isosurfaces panel's "Default" button, mirroring Style's. Only
    /// touches settings, not the grid or any already-extracted mesh (the
    /// caller re-extracts afterward so the shown surface matches the
    /// isovalue that just reset).
    fn reset_to_default(&mut self) {
        self.show = true;
        self.isovalue = DEFAULT_ISOSURFACE_ISOVALUE;
        self.refinement = 1;
        self.both_signs = true;
        self.positive_color = Color32::from_rgb(60, 90, 230);
        self.negative_color = Color32::from_rgb(220, 70, 60);
        self.opacity = DEFAULT_ISOSURFACE_OPACITY;
    }
}

/// A frozen isosurface snapshot taken by "Keep surface" — see
/// `App::kept_isosurfaces`.
struct KeptIsosurface {
    vertices: Vec<IsosurfaceVertex>,
}

/// One opened structure — its own geometry and its own hide/selection/
/// bond-style state. Deliberately does NOT own a Style/Material — that
/// stays a single value shared across every structure, so tuning it once
/// applies everywhere instead of needing to be redone per file.
struct LoadedStructure {
    label: String,
    molecule: Molecule,
    /// The file this structure was opened from, if any — used e.g. to
    /// default a "save coordinates as .xyz" export next to the source
    /// `.fchk`.
    source_path: Option<PathBuf>,
    hidden_atoms: HashSet<usize>,
    hidden_bonds: HashSet<usize>,
    bond_styles: Vec<BondVisualStyle>,
    selected_atoms: Vec<usize>,
    selected_bonds: Vec<usize>,
    measurements: Vec<Measurement>,
    /// Ordered atom picks awaiting a commit (via the Analysis window's
    /// "Add" button) into `measurements`.
    pending_measurement: Vec<usize>,
    /// `Some` only for structures opened from a `.cube` file.
    isosurface: Option<IsosurfaceState>,
    /// `Some` only for structures opened from a `.fchk` — the basis set +
    /// MO coefficients, parsed once up front so the "Generate orbitals"
    /// section can evaluate any MO on demand without re-parsing the file.
    /// `wavefunction.beta` is itself `Some` only for an unrestricted
    /// (open-shell) file — see `parse_fchk_wavefunction`.
    wavefunction: Option<Wavefunction>,
    /// Which MOs are ticked in the "Generate orbitals" picker — 0-based,
    /// only meaningful when `wavefunction` is `Some`. Beta is only ever
    /// populated for an unrestricted file (see above); it stays empty,
    /// unused, and hidden in the UI otherwise.
    selected_alpha_mos: HashSet<usize>,
    selected_beta_mos: HashSet<usize>,
}

impl LoadedStructure {
    fn new(label: String, molecule: Molecule, source_path: Option<PathBuf>) -> Self {
        let bond_styles = vec![BondVisualStyle::Single; molecule.bonds.len()];
        Self {
            label,
            molecule,
            source_path,
            hidden_atoms: HashSet::new(),
            hidden_bonds: HashSet::new(),
            bond_styles,
            selected_atoms: Vec::new(),
            selected_bonds: Vec::new(),
            measurements: Vec::new(),
            pending_measurement: Vec::new(),
            isosurface: None,
            wavefunction: None,
            selected_alpha_mos: HashSet::new(),
            selected_beta_mos: HashSet::new(),
        }
    }
}

pub struct App {
    camera: OrbitCamera,
    material: Material,
    structures: Vec<LoadedStructure>,
    active_structure: Option<usize>,
    logo_texture: egui::TextureHandle,
    /// Also handed to `ViewportResources` at construction (for the GPU
    /// texture) — kept here too since label layout is a CPU-side
    /// computation done every frame in this crate, not inside the
    /// renderer.
    glyph_atlas: Arc<GlyphAtlas>,
    start_time: Instant,
    render_state: egui_wgpu::RenderState,

    // Each tool panel is an independent floating window, toggled from the
    // top toolbar — this is the scalable structure: adding a new panel
    // later is one more bool + one more `show_*_window` function, no
    // restructuring of the others.
    show_style: bool,
    show_xyz: bool,
    show_visualization: bool,
    show_analysis: bool,
    show_structures: bool,
    show_about: bool,
    show_render: bool,

    coordinate_unit: LengthUnit,
    coordinate_format: CoordinateFormat,
    measurement_style: MeasurementStyle,
    atom_label_mode: AtomLabelMode,
    atom_label_style: AtomLabelStyle,
    render_export: RenderExportState,
    orbital_generation: OrbitalGenerationState,
    /// Updated every viewport repaint from the actual on-screen rect —
    /// the Render window reads this (rather than recomputing it itself,
    /// since it doesn't have direct access to the viewport rect) so the
    /// DPI preset can derive its height from the current on-screen view.
    last_aspect_ratio: f32,

    /// Isosurface lighting response — deliberately separate from
    /// `material` (the atom/bond one), shared across every structure the
    /// same way `material` is.
    isosurface_material: IsosurfaceMaterial,
    /// Ambient occlusion — a single shared toggle/settings pair driving
    /// both the live view and export (see `ao_render_material` and
    /// `ViewportResources::run_ao_passes`), so tuning the Style-window
    /// sliders shows the result live instead of only after a render.
    ao_enabled: bool,
    ao_settings: AoSettings,
    /// "Phase C" progressive settle quality — the camera/settings snapshot
    /// AO was last computed against, and whether that snapshot has
    /// already had its one-time full-quality recompute. Every frame
    /// either differs from this (the camera moved *or* a slider changed —
    /// both have to count, or dragging a slider without also touching the
    /// camera would just silently do nothing), AO reruns cheap
    /// (interactive framerate) and this gets updated to the new snapshot
    /// with `ao_settled = false`; the first frame afterward where neither
    /// has changed reruns AO once at full quality and sets
    /// `ao_settled = true`, after which further unchanged frames skip the
    /// AO recompute entirely (the texture's still valid — nothing moved).
    /// See the viewport panel's `ao_recompute_samples` computation.
    ao_last_camera: Option<OrbitCamera>,
    ao_last_settings: Option<AoSettings>,
    ao_settled: bool,
    /// Depth of field — same shared live+export toggle/settings shape as
    /// AO, but with no settle-tiering of its own: unlike AO's SSAO sample
    /// count, DoF's blur is cheap enough to just rerun at full quality
    /// every frame the viewport actually redraws (it reuses AO's own
    /// settle cadence only for the AO sub-pass it runs internally when
    /// both are on — see `ViewportResources::run_live_dof_pass`).
    dof_enabled: bool,
    dof_settings: DofSettings,
    /// Snapshots taken by the Isosurfaces "Keep surface" button — frozen
    /// geometry *and* color/opacity as they were at that moment, rendered
    /// every frame regardless of which structure is currently active, so
    /// several `.cube` files sharing one geometry (the common case:
    /// several orbitals of one molecule) can be composed into one image.
    /// "Clean" empties this back out.
    kept_isosurfaces: Vec<KeptIsosurface>,

    selection_mode: SelectionMode,
    warning: Option<(String, Color32, Instant)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("eframe must be running with the wgpu backend");

        let glyph_atlas = Arc::new(GlyphAtlas::new(&render_state.device, &render_state.queue));
        let resources = ViewportResources::new(&render_state.device, render_state.target_format, &glyph_atlas);
        render_state.renderer.write().callback_resources.insert(resources);

        let logo_texture = load_texture(&cc.egui_ctx, "apost3d_logo", include_bytes!("../assets/logo.png"));

        Self {
            camera: OrbitCamera::default(),
            material: Material::default(),
            structures: Vec::new(),
            active_structure: None,
            logo_texture,
            glyph_atlas,
            start_time: Instant::now(),
            render_state,
            show_style: false,
            show_xyz: false,
            show_visualization: false,
            show_analysis: false,
            show_structures: true,
            show_about: false,
            show_render: false,
            coordinate_unit: LengthUnit::Angstrom,
            coordinate_format: CoordinateFormat::AtomicNumberTable,
            measurement_style: MeasurementStyle::default(),
            atom_label_mode: AtomLabelMode::None,
            atom_label_style: AtomLabelStyle::default(),
            render_export: RenderExportState::default(),
            orbital_generation: OrbitalGenerationState::default(),
            last_aspect_ratio: 16.0 / 9.0,
            isosurface_material: IsosurfaceMaterial::default(),
            ao_enabled: true,
            ao_settings: AoSettings::default(),
            ao_last_camera: None,
            ao_last_settings: None,
            ao_settled: false,
            dof_enabled: true,
            dof_settings: DofSettings::default(),
            kept_isosurfaces: Vec::new(),
            selection_mode: SelectionMode::Select,
            warning: None,
        }
    }

    fn rebuild_geometry(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_geometry(
                &self.render_state.device,
                &structure.molecule,
                &structure.hidden_atoms,
                &structure.hidden_bonds,
                &structure.bond_styles,
            );
        }
    }

    fn rebuild_highlights(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_highlights(&self.render_state.device, &structure.molecule, &structure.selected_atoms, &structure.selected_bonds);
        }
    }

    fn rebuild_measurements(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let segments: Vec<(usize, usize)> = structure.measurements.iter().flat_map(|m| m.kind.segments()).collect();
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_measurements(&self.render_state.device, &structure.molecule, &segments, self.measurement_style.line_color);
        }
    }

    /// Combines the active structure's live isosurface (if it has one,
    /// shown, and already extracted) with every "kept" snapshot into one
    /// vertex buffer and uploads it — cheap (just concatenating already-
    /// computed vertex data), safe to call every time anything isosurface-
    /// related changes, unlike the actual extraction.
    fn rebuild_isosurface(&self) {
        let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
        for kept in &self.kept_isosurfaces {
            vertices.extend_from_slice(&kept.vertices);
        }
        if let Some(active) = self.active_structure {
            if let Some(iso) = self.structures.get(active).and_then(|s| s.isosurface.as_ref()) {
                if iso.show {
                    let opacity = iso.opacity;
                    if let Some(mesh) = &iso.cached_positive {
                        push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.positive_color), opacity);
                    }
                    if iso.both_signs {
                        if let Some(mesh) = &iso.cached_negative {
                            push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.negative_color), opacity);
                        }
                    }
                }
            }
        }
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_isosurface(&self.render_state.device, &vertices);
            resources.update_isosurface_material(&self.render_state.queue, &self.ao_render_isosurface_material());
        }
    }

    /// Re-runs marching tetrahedra for the active structure's isosurface
    /// at its current isovalue/refinement/both-signs settings — the real
    /// cost (a refined grid can take seconds in a debug build), so this
    /// only runs on an explicit "Update surface" click or the one-time
    /// automatic extraction the first time a `.cube` structure becomes
    /// active (see `set_active`), never continuously while adjusting a
    /// slider.
    fn extract_active_isosurface(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get_mut(active) else { return };
        let Some(iso) = &mut structure.isosurface else { return };

        let isovalue = iso.isovalue;
        let refinement = iso.refinement;
        let both_signs = iso.both_signs;

        let refined_grid;
        let grid_ref = if refinement > 1 {
            refined_grid = refine_grid(&iso.grid, refinement);
            &refined_grid
        } else {
            &iso.grid
        };

        let positive = extract_isosurface(grid_ref, isovalue);
        let negative = if both_signs { Some(extract_isosurface(&grid_ref.negated(), isovalue)) } else { None };

        iso.cached_positive = Some(positive);
        iso.cached_negative = negative;
        iso.extracted = true;

        self.rebuild_isosurface();
    }

    /// Resets the active structure's isosurface settings (isovalue,
    /// refinement, colors, opacity, ...) *and* the shared isosurface
    /// material (ambient/diffuse/specular/shininess) back to the defaults,
    /// then re-extracts immediately — the "Default" button in the
    /// Isosurfaces panel, mirroring Style's "Default" for the atom/bond
    /// material.
    fn reset_active_isosurface_to_default(&mut self) {
        self.isosurface_material = IsosurfaceMaterial::default();
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get_mut(active) else { return };
        let Some(iso) = &mut structure.isosurface else { return };
        iso.reset_to_default();
        self.extract_active_isosurface();
    }

    /// Freezes the active structure's current isosurface (geometry *and*
    /// its color/opacity at this exact moment) into `kept_isosurfaces`,
    /// so it stays visible even after switching to a different structure.
    /// The natural way to compose several `.cube` files sharing one
    /// geometry (several orbitals of one molecule) into a single image.
    fn keep_active_isosurface(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(iso) = self.structures.get(active).and_then(|s| s.isosurface.as_ref()) else { return };
        if !iso.show {
            self.show_warning("Isosurface is hidden — nothing to keep");
            return;
        }
        let mut vertices = Vec::new();
        let opacity = iso.opacity;
        if let Some(mesh) = &iso.cached_positive {
            push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.positive_color), opacity);
        }
        if iso.both_signs {
            if let Some(mesh) = &iso.cached_negative {
                push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.negative_color), opacity);
            }
        }
        if vertices.is_empty() {
            self.show_warning("Nothing extracted yet — click Update surface first");
            return;
        }
        self.kept_isosurfaces.push(KeptIsosurface { vertices });
        self.show_status("Isosurface kept");
    }

    /// Removes every kept isosurface *and* hides the active structure's
    /// own live one — a full reset back to no isosurfaces shown at all.
    fn clean_isosurfaces(&mut self) {
        self.kept_isosurfaces.clear();
        if let Some(active) = self.active_structure {
            if let Some(iso) = self.structures.get_mut(active).and_then(|s| s.isosurface.as_mut()) {
                iso.show = false;
            }
        }
        self.rebuild_isosurface();
        self.show_status("Cleared all isosurfaces");
    }

    fn show_warning(&mut self, message: impl Into<String>) {
        self.warning = Some((message.into(), Color32::from_rgb(196, 60, 40), Instant::now()));
    }

    fn show_status(&mut self, message: impl Into<String>) {
        self.warning = Some((message.into(), Color32::from_rgb(45, 130, 80), Instant::now()));
    }

    fn clear_selection(&mut self) {
        if let Some(active) = self.active_structure {
            self.structures[active].selected_atoms.clear();
            self.structures[active].selected_bonds.clear();
        }
        self.rebuild_highlights();
    }

    /// Switches the active structure. Re-frames the camera on it —
    /// different opened files can be wildly different sizes/positions
    /// (this is the main reason .xyz support exists: comparing unrelated
    /// structures side by side) — *unless* the structure being switched
    /// away from and the one being switched to share the same geometry
    /// (the common case for `.cube` files: several orbitals of one
    /// molecule), in which case reframing on every switch would be an
    /// annoying reset of a view you just set up. If the newly active
    /// structure is a `.cube` one that's never had its isosurface
    /// extracted yet, that happens now too — a one-time cost, not
    /// something that repeats on every subsequent switch back to it.
    fn set_active(&mut self, index: usize) {
        let should_reframe = match (self.active_structure.and_then(|p| self.structures.get(p)), self.structures.get(index)) {
            (Some(previous), Some(next)) => !molecules_share_geometry(&previous.molecule, &next.molecule),
            _ => true,
        };

        self.active_structure = Some(index);
        if should_reframe {
            if let Some(structure) = self.structures.get(index) {
                let (center, radius) = structure.molecule.bounding_sphere();
                self.camera.frame_bounds(center, radius);
            }
        }
        self.rebuild_geometry();
        self.rebuild_highlights();
        self.rebuild_measurements();

        let needs_initial_extraction = self.structures.get(index).and_then(|s| s.isosurface.as_ref()).is_some_and(|iso| !iso.extracted);
        if needs_initial_extraction {
            self.extract_active_isosurface();
        } else {
            self.rebuild_isosurface();
        }
    }

    fn open_fchk(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Gaussian checkpoint", &["fchk"]).pick_file() else { return };
        match Molecule::from_fchk(&path) {
            Ok(molecule) => {
                let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled.fchk".into());
                let mut structure = LoadedStructure::new(label, molecule, Some(path.clone()));
                // Parsing the wavefunction (basis set + MO coefficients) is
                // a separate, slower full-file pass from the geometry-only
                // one above — failure here just means no "Generate
                // orbitals" section for this structure, not a failure to
                // open it at all.
                match parse_fchk_wavefunction(&path) {
                    Ok(wfn) => {
                        // `evaluate_basis_functions` evaluates every shell
                        // in the basis regardless of which MO is being
                        // asked for (it computes the whole basis-function
                        // vector, then a separate dot product picks out
                        // one MO) — so a single h-or-higher shell anywhere
                        // in the file blocks *every* orbital, not just
                        // ones with real weight on it. Worth flagging
                        // right away rather than only on the first failed
                        // "Generate" click.
                        let max_angular_momentum = wfn.basis.shells.iter().map(|s| s.angular_momentum).max().unwrap_or(0);
                        if wfn.alpha.num_orbitals() > 0 {
                            structure.selected_alpha_mos.insert(wfn.alpha.homo_index() - 1);
                        }
                        if let Some(beta) = &wfn.beta {
                            if beta.num_orbitals() > 0 {
                                structure.selected_beta_mos.insert(beta.homo_index() - 1);
                            }
                        }
                        structure.wavefunction = Some(wfn);
                        if max_angular_momentum > 4 {
                            self.show_warning("This basis set includes h (or higher) shells — orbital generation isn't supported yet for this file.");
                        }
                    }
                    Err(err) => self.show_warning(format!("Geometry loaded, but orbitals unavailable: {err}")),
                }
                let index = self.structures.len();
                self.structures.push(structure);
                self.set_active(index);
            }
            Err(err) => self.show_warning(format!("Could not load {}: {err}", path.display())),
        }
    }

    fn open_xyz(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("XYZ", &["xyz"]).pick_files() else { return };
        let mut first_new_index = None;
        for path in paths {
            match parse_xyz(&path) {
                Ok(molecule) => {
                    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled.xyz".into());
                    let index = self.structures.len();
                    self.structures.push(LoadedStructure::new(label, molecule, Some(path)));
                    first_new_index.get_or_insert(index);
                }
                Err(err) => self.show_warning(format!("Could not load {}: {err}", path.display())),
            }
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
    }

    /// Opens one or more `.cube` files — same multi-select UX as `.xyz`.
    /// Each becomes its own Structures entry with both a molecule (parsed
    /// from the cube's own atom section) and an isosurface (see
    /// `IsosurfaceState`); `set_active` handles the shared-geometry
    /// camera-freeze and the one-time initial extraction.
    fn open_cube(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("Gaussian cube", &["cube"]).pick_files() else { return };
        let mut first_new_index = None;
        for path in paths {
            match parse_cube(&path) {
                Ok(cube_file) => {
                    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled.cube".into());
                    let mut structure = LoadedStructure::new(label, cube_file.molecule, Some(path));
                    structure.isosurface = Some(IsosurfaceState::new(cube_file.grid));
                    let index = self.structures.len();
                    self.structures.push(structure);
                    first_new_index.get_or_insert(index);
                }
                Err(err) => self.show_warning(format!("Could not load {}: {err}", path.display())),
            }
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
    }

    /// Generates a new structure (sharing the active structure's geometry)
    /// for each ticked MO in the active structure's "Generate orbitals"
    /// picker — the same shape of result as opening one `.cube` file per
    /// orbital (see `open_cube`), computed directly from the `.fchk`
    /// wavefunction instead of needing an external `cubegen`/Multiwfn/
    /// APOST-3D step first. `set_active`'s shared-geometry camera freeze
    /// applies here too, since every generated structure shares the
    /// source structure's own molecule.
    fn generate_selected_orbitals(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let Some(wfn) = &structure.wavefunction else { return };
        if structure.selected_alpha_mos.is_empty() && structure.selected_beta_mos.is_empty() {
            self.show_warning("No orbitals selected");
            return;
        }

        let spacing = self.orbital_generation.resolve_spacing_bohr();
        let base_label = structure.label.clone();
        let molecule = structure.molecule.clone();
        let is_unrestricted = wfn.beta.is_some();

        // One entry per spin channel that has anything selected — for a
        // restricted file there's only ever the alpha one (and its
        // generated labels stay unsuffixed, since there's no beta
        // channel to disambiguate from).
        let mut channels: Vec<(&MolecularOrbitals, Vec<usize>, &str)> = Vec::new();
        let mut alpha_indices: Vec<usize> = structure.selected_alpha_mos.iter().copied().collect();
        alpha_indices.sort_unstable();
        channels.push((&wfn.alpha, alpha_indices, if is_unrestricted { " (alpha)" } else { "" }));
        if let Some(beta) = &wfn.beta {
            let mut beta_indices: Vec<usize> = structure.selected_beta_mos.iter().copied().collect();
            beta_indices.sort_unstable();
            channels.push((beta, beta_indices, " (beta)"));
        }

        // Every ticked orbital, across both spin channels, is evaluated in
        // one batched `generate_mo_grids` call — the basis functions
        // themselves (the expensive part, hundreds of shells on a real
        // molecule) get computed once per grid point and shared across all
        // of them, rather than once per orbital as a separate full grid pass.
        let mut requests: Vec<(&MolecularOrbitals, usize)> = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        for (orbitals, mo_indices, spin_suffix) in &channels {
            let homo = orbitals.homo_index();
            let lumo = orbitals.lumo_index();
            for &mo_index in mo_indices {
                let mo_number = mo_index + 1;
                let tag = if mo_number == homo {
                    "HOMO".to_string()
                } else if mo_number == lumo {
                    "LUMO".to_string()
                } else {
                    format!("MO{mo_number}")
                };
                requests.push((*orbitals, mo_index));
                labels.push(format!("{base_label} — {tag}{spin_suffix}"));
            }
        }

        let mut generated = Vec::new();
        let mut failures = Vec::new();
        match generate_mo_grids(&wfn.basis, &requests, spacing, ORBITAL_GRID_PADDING_BOHR) {
            Ok(grids) => {
                for (grid, label) in grids.into_iter().zip(labels) {
                    let mut new_structure = LoadedStructure::new(label, molecule.clone(), None);
                    let mut iso = IsosurfaceState::new(grid);
                    iso.isovalue = self.orbital_generation.isovalue;
                    new_structure.isosurface = Some(iso);
                    generated.push(new_structure);
                }
            }
            Err(err) => failures.push(err),
        }

        let first_new_index = if generated.is_empty() { None } else { Some(self.structures.len()) };
        self.structures.extend(generated);
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
        if !failures.is_empty() {
            self.show_warning(format!("Some orbitals failed to generate: {}", failures.join("; ")));
        } else if first_new_index.is_some() {
            self.show_status("Orbitals generated");
        }
    }

    /// Writes the active structure's coordinates to a standard `.xyz` file
    /// next to its source `.fchk`, same base name, `.xyz` extension —
    /// always in Angstrom regardless of the viewer's current unit toggle,
    /// since that's what makes it a valid, portable `.xyz` file.
    fn export_active_xyz(&mut self, target: std::path::PathBuf) {
        let Some(active) = self.active_structure else { return };
        let structure = &self.structures[active];
        let text = format_coordinates(&structure.molecule, LengthUnit::Angstrom, CoordinateFormat::XyzFile, &structure.label);
        match std::fs::write(&target, text) {
            Ok(()) => self.show_status(format!("Saved {}", target.display())),
            Err(err) => self.show_warning(format!("Could not save {}: {err}", target.display())),
        }
    }

    /// Turns the current preset (plus the live view's aspect ratio, for
    /// `Dpi`) into concrete pixel dimensions and background.
    fn resolve_export_settings(&self) -> ExportSettings {
        let background = if self.render_export.transparent_background {
            None
        } else {
            let [r, g, b] = self.material.background;
            Some([r, g, b, 1.0])
        };
        match self.render_export.preset {
            RenderPreset::Dpi => self.publication_export_settings(background),
            RenderPreset::Custom => ExportSettings {
                width: self.render_export.custom_width.max(1),
                height: self.render_export.custom_height.max(1),
                supersample: self.render_export.custom_supersample.max(1),
                background,
                ambient_occlusion: self.ao_enabled.then_some(self.ao_settings),
                depth_of_field: self.dof_enabled.then_some(self.dof_settings),
                dof_focus_distance: self.camera.distance,
            },
        }
    }

    /// Turns the requested figure width (inches) and DPI into pixel
    /// dimensions. The physical width always maps to the horizontal
    /// dimension specifically — that's what a journal's column-width
    /// requirement actually constrains, regardless of the molecule's own
    /// aspect ratio. Height is then derived from the live view's aspect
    /// ratio, so the export still frames exactly what's on screen.
    fn publication_export_settings(&self, background: Option<[f32; 4]>) -> ExportSettings {
        let width = ((self.render_export.figure_width_in * self.render_export.dpi as f64).round() as u32).max(1);
        let aspect_ratio = if self.last_aspect_ratio > 0.0 { self.last_aspect_ratio } else { 1.0 };
        let height = ((width as f32 / aspect_ratio).round() as u32).max(1);
        ExportSettings {
            width,
            height,
            supersample: PUBLICATION_SUPERSAMPLE,
            background,
            ambient_occlusion: self.ao_enabled.then_some(self.ao_settings),
            depth_of_field: self.dof_enabled.then_some(self.dof_settings),
            dof_focus_distance: self.camera.distance,
        }
    }

    /// The material to actually render with — identical to the Style
    /// panel's own `material` unless ambient occlusion is on, in which
    /// case it's dampened (reduced reflectance and light intensity, raised
    /// ambient floor). AO reads as a much stronger, more "sculpted" effect
    /// against flatter shading — a strong specular highlight competes with
    /// and dilutes the occlusion darkening, the same reason Speck's own
    /// atoms run zero lighting at all and let AO + outline do the whole
    /// job. Eased twice now from the old Blinn-Phong version of this (was
    /// diffuse ×0.4, specular ×0.05): GGX's own low-reflectance dielectric
    /// default is already far more grounded than Blinn-Phong's hot
    /// specular=0.45 default was, so it doesn't need nearly as much
    /// suppression — and the first (×0.6/×0.65) pass turned out to still
    /// be a bit heavy-handed, leaving atoms/bonds reading flatter than the
    /// isosurface sitting right next to them once *that* got a properly
    /// defined GGX highlight (2026-08-29 hands-on pass). Shared by the
    /// live view and export so what you see while tuning the AO sliders
    /// is what you actually get — `self.material` itself (the Style
    /// panel's own live value) is never touched.
    fn ao_render_material(&self) -> Material {
        if !self.ao_enabled {
            return self.material;
        }
        Material {
            ambient: (self.material.ambient + 0.15).min(0.75),
            reflectance: self.material.reflectance * 0.8,
            light_intensity: self.material.light_intensity * 0.8,
            ..self.material
        }
    }

    /// Same idea as `ao_render_material`, for the isosurface's own
    /// (entirely separate) material — a real gap found by hands-on
    /// testing (2026-08-29): unlike the atom/bond material, the
    /// isosurface's was never dampened when AO turns on at all, so it
    /// could read as noticeably shinier than the (deliberately dampened)
    /// atoms/bonds sitting right next to it. Same dampening factors as
    /// `ao_render_material` for consistency between the two materials.
    fn ao_render_isosurface_material(&self) -> IsosurfaceMaterial {
        if !self.ao_enabled {
            return self.isosurface_material;
        }
        let m = self.isosurface_material.material;
        IsosurfaceMaterial { material: [(m[0] + 0.15).min(0.75), m[1], m[2] * 0.8, m[3] * 0.8], fresnel: self.isosurface_material.fresnel }
    }

    /// The DPI to embed in the exported PNG's physical-size metadata
    /// (`pHYs` chunk) — only meaningful for `Dpi`, since `Custom` has no
    /// attached physical size to be consistent with.
    fn resolve_export_dpi(&self) -> Option<u32> {
        match self.render_export.preset {
            RenderPreset::Dpi => Some(self.render_export.dpi),
            RenderPreset::Custom => None,
        }
    }

    /// Writes RGBA8 pixels to a PNG, optionally embedding a physical-size
    /// (`pHYs`) chunk so image editors and journal submission systems read
    /// the correct print size automatically. `image::save_buffer` (used
    /// when `dpi` is `None`) has no metadata hook, so the DPI case goes
    /// through the lower-level `png` crate directly instead.
    fn write_png(path: &std::path::Path, pixels: &[u8], width: u32, height: u32, dpi: Option<u32>) -> Result<(), String> {
        let Some(dpi) = dpi else {
            return image::save_buffer(path, pixels, width, height, image::ColorType::Rgba8).map_err(|err| err.to_string());
        };
        let file = std::fs::File::create(path).map_err(|err| err.to_string())?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels_per_meter = (dpi as f64 / 0.0254).round() as u32;
        encoder.set_pixel_dims(Some(png::PixelDimensions { xppu: pixels_per_meter, yppu: pixels_per_meter, unit: png::Unit::Meter }));
        let mut writer = encoder.write_header().map_err(|err| err.to_string())?;
        writer.write_image_data(pixels).map_err(|err| err.to_string())
    }

    /// Builds the same atom/measurement label geometry the live view
    /// does (see the central-panel label-building block), sized for
    /// `target_height_px` — the export's own output height, not whatever
    /// the on-screen viewport happens to be, so measurement labels (which
    /// hold a constant *apparent* size relative to the render they're in)
    /// come out proportioned exactly like the live view once the export
    /// is viewed at its own native resolution. Deliberately a separate,
    /// simplified copy rather than sharing code with the live-view block:
    /// that block also does interactive drag hit-testing (needs `ui`,
    /// mutates measurement state), which a one-shot export has no use for.
    fn build_export_label_instances(&self, active: usize, target_height_px: f32) -> Vec<GlyphInstance> {
        let mut label_instances: Vec<GlyphInstance> = Vec::new();
        let structure = &self.structures[active];

        if self.atom_label_mode != AtomLabelMode::None {
            let color = color32_to_rgb(self.atom_label_style.text_color);
            for (index, (&z, &position)) in structure.molecule.atomic_numbers.iter().zip(&structure.molecule.positions).enumerate() {
                if structure.hidden_atoms.contains(&index) {
                    continue;
                }
                let radius = element_data(z).vdw_radius * self.material.atom_scale;
                let scale = glyph_scale_for_world_size(radius * self.atom_label_style.relative_size);
                let text = match self.atom_label_mode {
                    AtomLabelMode::Number => format!("{}", index + 1),
                    AtomLabelMode::Type => element_data(z).symbol.to_string(),
                    AtomLabelMode::NumberType => format!("{}{}", element_data(z).symbol, index + 1),
                    AtomLabelMode::None => unreachable!(),
                };
                let to_camera = (self.camera.eye() - position).normalize_or_zero();
                let label_anchor = position + to_camera * (radius * 1.15 + 0.02);
                push_label(&mut label_instances, &self.glyph_atlas, &text, label_anchor, scale, color, EDGE_BIAS_ATOM_LABEL);
            }
        }

        let (camera_right, camera_up) = self.camera.screen_basis();
        let measurement_color = color32_to_rgb(self.measurement_style.text_color);
        for measurement in &structure.measurements {
            let anchor = measurement_anchor(&structure.molecule, measurement.kind);
            let distance = (anchor - self.camera.eye()).length();
            let world_per_pixel = self.camera.world_units_per_pixel(distance, target_height_px);
            let world_offset = camera_right * (measurement.label_offset.x * world_per_pixel)
                - camera_up * (measurement.label_offset.y * world_per_pixel);
            let final_anchor = anchor + world_offset;

            let value = measure(&structure.molecule, measurement.kind);
            let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
            let scale = glyph_scale_for_font_size(self.measurement_style.font_size, world_per_pixel);
            let edge_bias = if self.measurement_style.bold { EDGE_BIAS_BOLD } else { EDGE_BIAS_NORMAL };
            push_label(&mut label_instances, &self.glyph_atlas, &text, final_anchor, scale, measurement_color, edge_bias);
        }

        label_instances
    }

    /// Renders the active structure offscreen at `settings` and saves it
    /// as a PNG, letting the user pick the name/location. Blocks the UI
    /// thread briefly (see `ViewportResources::render_offscreen`) — an
    /// acceptable one-shot cost for an explicit, infrequent action.
    fn export_render_png(&mut self, settings: ExportSettings) {
        let Some(active) = self.active_structure else {
            self.show_warning("Open a structure first.");
            return;
        };

        let default_name = self
            .structures[active]
            .source_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| format!("{}.png", s.to_string_lossy()))
            .unwrap_or_else(|| "render.png".to_string());
        let Some(path) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name(default_name).save_file() else {
            return;
        };

        let export_aspect = settings.width as f32 / settings.height.max(1) as f32;
        let mut uniforms = SceneUniforms::new(&self.camera, export_aspect, &self.ao_render_material());
        let target_format = self.render_state.target_format;
        uniforms.set_srgb_target(target_format.is_srgb());
        let label_instances = self.build_export_label_instances(active, settings.height as f32);

        let mut renderer = self.render_state.renderer.write();
        let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() else {
            drop(renderer);
            self.show_warning("Renderer not ready.");
            return;
        };
        let result = resources.render_offscreen(&self.render_state.device, &self.render_state.queue, target_format, &uniforms, &label_instances, &settings);
        drop(renderer);

        match result {
            Ok(mut pixels) => {
                // The swapchain format on some platforms/backends is
                // BGRA rather than RGBA — the offscreen texture (and so
                // the readback) is in whatever channel order that format
                // uses, since the pipelines are fixed to it. Swap back to
                // RGB order before handing to the PNG encoder, which
                // always expects RGBA.
                if matches!(target_format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
                    for px in pixels.chunks_mut(4) {
                        px.swap(0, 2);
                    }
                }
                let dpi = self.resolve_export_dpi();
                match Self::write_png(&path, &pixels, settings.width, settings.height, dpi) {
                    Ok(()) => self.show_status(format!("Saved {}", path.display())),
                    Err(err) => self.show_warning(format!("Could not write {}: {err}", path.display())),
                }
            }
            Err(err) => self.show_warning(format!("Render failed: {err}")),
        }
    }

    fn show_render_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_render;
        egui::Window::new("Render")
            .open(&mut open)
            .default_pos([320.0, 460.0])
            .default_width(280.0)
            .show(ctx, |ui| {
                if self.active_structure.is_none() {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                }

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.render_export.preset, RenderPreset::Dpi, "DPI");
                    ui.selectable_value(&mut self.render_export.preset, RenderPreset::Custom, "Custom (pixels)");
                });
                ui.add_space(4.0);

                match self.render_export.preset {
                    RenderPreset::Dpi => {
                        ui.horizontal(|ui| {
                            ui.label("DPI:");
                            ui.add(egui::DragValue::new(&mut self.render_export.dpi).range(72..=1200));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Figure width:");
                            ui.add(egui::DragValue::new(&mut self.render_export.figure_width_in).range(1.0..=20.0).speed(0.05).suffix(" in"));
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Single column (3.25 in)").clicked() {
                                self.render_export.figure_width_in = FIGURE_WIDTH_SINGLE_COLUMN_IN;
                            }
                            if ui.button("Double column (6.75 in)").clicked() {
                                self.render_export.figure_width_in = FIGURE_WIDTH_DOUBLE_COLUMN_IN;
                            }
                        });
                        ui.label(
                            egui::RichText::new("300 DPI is the common journal minimum — raise it for larger print sizes. Embeds the physical size in the PNG.")
                                .small()
                                .weak(),
                        );
                    }
                    RenderPreset::Custom => {
                        ui.horizontal(|ui| {
                            ui.label("Width:");
                            ui.add(egui::DragValue::new(&mut self.render_export.custom_width).range(64..=16384));
                            ui.label("Height:");
                            ui.add(egui::DragValue::new(&mut self.render_export.custom_height).range(64..=16384));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Supersample:");
                            ui.selectable_value(&mut self.render_export.custom_supersample, 1, "1x");
                            ui.selectable_value(&mut self.render_export.custom_supersample, 2, "2x");
                            ui.selectable_value(&mut self.render_export.custom_supersample, 4, "4x");
                        });
                    }
                }

                ui.add_space(6.0);
                ui.checkbox(&mut self.render_export.transparent_background, "Transparent background");
                if self.ao_enabled {
                    ui.label(
                        egui::RichText::new("Ambient occlusion is on (Style window) — this export uses far more samples than the live preview.")
                            .small()
                            .weak(),
                    );
                }

                ui.add_space(8.0);
                ui.separator();
                let settings = self.resolve_export_settings();
                ui.label(egui::RichText::new(format!("Output: {} x {} px", settings.width, settings.height)).small().weak());
                ui.add_space(6.0);

                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Render and save PNG...")).clicked() {
                    self.export_render_png(settings);
                }
            });
        self.show_render = open;
    }

    fn show_splash(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);

        let logo_size = self.logo_texture.size_vec2();
        let max_width = (rect.width() * 0.4).min(logo_size.x);
        let scale = max_width / logo_size.x;
        let display_size = logo_size * scale;

        let logo_rect = egui::Rect::from_center_size(rect.center(), display_size);
        ui.painter().image(
            self.logo_texture.id(),
            logo_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("APOST3Dview").strong());
                ui.separator();
                if ui.selectable_label(self.show_structures, "Structures").clicked() {
                    self.show_structures = !self.show_structures;
                }
                if ui.selectable_label(self.show_style, "Style").clicked() {
                    self.show_style = !self.show_style;
                }
                if ui.selectable_label(self.show_xyz, "XYZ").clicked() {
                    self.show_xyz = !self.show_xyz;
                }
                if ui.selectable_label(self.show_visualization, "Visualization").clicked() {
                    self.show_visualization = !self.show_visualization;
                }
                if ui.selectable_label(self.show_analysis, "Analysis").clicked() {
                    self.show_analysis = !self.show_analysis;
                }
                if ui.selectable_label(self.show_render, "Render").clicked() {
                    self.show_render = !self.show_render;
                }
                if ui.selectable_label(self.show_about, "About").clicked() {
                    self.show_about = !self.show_about;
                }
            });
        });
    }

    fn show_structures_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_structures;
        egui::Window::new("Structures")
            .open(&mut open)
            .default_pos([40.0, 60.0])
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Open .fchk...").clicked() {
                        self.open_fchk();
                    }
                    if ui.button("Open .xyz...").clicked() {
                        self.open_xyz();
                    }
                    if ui.button("Open .cube...").clicked() {
                        self.open_cube();
                    }
                });

                ui.add_space(8.0);
                if self.structures.is_empty() {
                    ui.label(egui::RichText::new("No structures loaded yet.").weak());
                } else {
                    ui.separator();
                    let mut switch_to = None;
                    for (index, structure) in self.structures.iter().enumerate() {
                        let selected = self.active_structure == Some(index);
                        if ui.selectable_label(selected, &structure.label).clicked() {
                            switch_to = Some(index);
                        }
                    }
                    if let Some(index) = switch_to {
                        self.set_active(index);
                    }
                }
            });
        self.show_structures = open;
    }

    fn show_style_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_style;
        egui::Window::new("Style")
            .open(&mut open)
            .default_pos([ctx.content_rect().right() - 280.0, 60.0])
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Default").clicked() {
                        self.material = Material::default();
                        self.ao_settings = AoSettings::default();
                        self.dof_settings = DofSettings::default();
                        self.reset_active_isosurface_to_default();
                    }
                    if ui.button("Publication").clicked() {
                        self.material = Material::publication();
                        self.ao_settings = AoSettings::default();
                        self.dof_settings = DofSettings::default();
                        self.reset_active_isosurface_to_default();
                        // Same reasoning as `Material::publication`'s own
                        // reflectance cut — the isosurface sits under the
                        // same dead-on light and would otherwise be the
                        // one part of the scene still showing the harsher
                        // "flash" hotspot.
                        self.isosurface_material.material[2] *= 0.6;
                        self.rebuild_isosurface();
                    }
                    if ui.button("Space-filling").clicked() {
                        // Not a new rendering mode — atom_scale near 1.0
                        // (real van der Waals radius) already makes
                        // neighboring spheres overlap, and the impostor
                        // renderer's own silhouette already carves a real
                        // seam at that overlap (see `sphere.wgsl` — no
                        // texture/bump map involved). Bonds need no special
                        // handling either: at this scale a normal covalent
                        // bond's cylinder sits entirely inside the two
                        // overlapping spheres already, so it's naturally
                        // hidden. AO tightened to match — a much smaller
                        // radius so it darkens right at sphere-sphere
                        // contact instead of broad ambient shading, and
                        // higher contrast so that darkening reads as a
                        // crisp line (confirmed 2026-08-29 via a real
                        // side-by-side render, see git history).
                        self.material = Material { atom_scale: 0.95, ..Material::default() };
                        self.ao_settings = AoSettings { radius: 0.45, strength: 1.0, bias: 0.01, contrast_power: 4.5, outline_strength: AoSettings::default().outline_strength };
                        self.ao_enabled = true;
                        self.dof_settings = DofSettings::default();
                        self.reset_active_isosurface_to_default();
                    }
                });

                ui.add_space(8.0);
                ui.label("Geometry");
                ui.add(Slider::new(&mut self.material.atom_scale, 0.1..=1.5).text("atom scale"));
                ui.add(Slider::new(&mut self.material.bond_radius, 0.02..=0.5).text("bond radius"));

                ui.add_space(12.0);
                ui.label("Atom labels");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::None, "None");
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::Number, "Number");
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::Type, "Type");
                    ui.selectable_value(&mut self.atom_label_mode, AtomLabelMode::NumberType, "Number+Type");
                });
                if self.atom_label_mode != AtomLabelMode::None {
                    ui.add(Slider::new(&mut self.atom_label_style.relative_size, 0.2..=2.5).text("label size (× atom radius)"));
                    ui.horizontal(|ui| {
                        ui.label("Label color:");
                        ui.color_edit_button_srgba(&mut self.atom_label_style.text_color);
                    });
                }

                ui.add_space(12.0);
                ui.label("Material");
                ui.add(Slider::new(&mut self.material.ambient, 0.0..=1.0).text("ambient"));
                ui.add(Slider::new(&mut self.material.roughness, 0.05..=1.0).text("roughness"));
                ui.add(Slider::new(&mut self.material.reflectance, 0.0..=0.3).text("reflectance"));
                ui.add(Slider::new(&mut self.material.light_intensity, 0.5..=8.0).text("light intensity"));
                ui.add(Slider::new(&mut self.material.exposure, 0.3..=2.5).text("exposure"));

                ui.add_space(12.0);
                ui.label("Isosurface material");
                ui.label(egui::RichText::new("Independent from the atom/bond material above.").small().weak());
                let mut isosurface_material_changed = false;
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[0], 0.0..=1.0).text("ambient")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[1], 0.05..=1.0).text("roughness")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[2], 0.0..=0.3).text("reflectance")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.material[3], 0.5..=8.0).text("light intensity")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.fresnel[0], 0.5..=8.0).text("rim power")).changed();
                isosurface_material_changed |= ui.add(Slider::new(&mut self.isosurface_material.fresnel[1], 0.0..=2.0).text("rim glow")).changed();
                if isosurface_material_changed {
                    self.rebuild_isosurface();
                }

                ui.add_space(12.0);
                if ui.checkbox(&mut self.ao_enabled, "Ambient occlusion").changed() {
                    // The isosurface material buffer isn't re-uploaded every
                    // frame the way atom/bond material is (see
                    // `ao_render_isosurface_material`'s doc) — without this,
                    // toggling AO alone (no other isosurface setting touched)
                    // would leave it stale until something else happened to
                    // trigger a rebuild.
                    self.rebuild_isosurface();
                }
                ui.label(
                    egui::RichText::new("Real per-pixel contact shading on atoms and bonds — live preview uses far fewer samples than export.")
                        .small()
                        .weak(),
                );
                if self.ao_enabled {
                    ui.add(Slider::new(&mut self.ao_settings.radius, 0.1..=3.0).text("radius (\u{c5}ngstrom)"));
                    ui.add(Slider::new(&mut self.ao_settings.strength, 0.0..=1.0).text("strength"));
                    ui.add(Slider::new(&mut self.ao_settings.contrast_power, 0.5..=6.0).text("contrast"));
                    ui.add(Slider::new(&mut self.ao_settings.outline_strength, 0.0..=6.0).text("outline"));
                    if ui.button("Default").clicked() {
                        self.ao_settings = AoSettings::default();
                    }
                }

                ui.add_space(12.0);
                ui.checkbox(&mut self.dof_enabled, "Depth of field");
                ui.label(
                    egui::RichText::new("Blurs whatever's far from the focal plane (always the current orbit target) — the finishing touch on a render, without changing any colors.")
                        .small()
                        .weak(),
                );
                if self.dof_enabled {
                    ui.add(Slider::new(&mut self.dof_settings.strength, 0.0..=1.0).text("strength"));
                    ui.add(Slider::new(&mut self.dof_settings.focus_range, 0.05..=2.0).text("focus range"));
                    if ui.button("Default").clicked() {
                        self.dof_settings = DofSettings::default();
                    }
                }

                ui.add_space(12.0);
                ui.label("Lighting");
                ui.add(
                    Slider::new(&mut self.material.light_yaw, -std::f32::consts::PI..=std::f32::consts::PI)
                        .text("light yaw"),
                );
                ui.add(Slider::new(&mut self.material.light_pitch, -1.5..=1.5).text("light pitch"));

                ui.add_space(12.0);
                ui.label("Background");
                let mut background = Color32::from_rgb(
                    (self.material.background[0] * 255.0) as u8,
                    (self.material.background[1] * 255.0) as u8,
                    (self.material.background[2] * 255.0) as u8,
                );
                if ui.color_edit_button_srgba(&mut background).changed() {
                    self.material.background = [
                        background.r() as f32 / 255.0,
                        background.g() as f32 / 255.0,
                        background.b() as f32 / 255.0,
                    ];
                }

                ui.add_space(16.0);
                ui.separator();
                ui.label("Right-drag to orbit, shift+right-drag to pan, scroll/arrows to zoom/rotate. Left-click selects.");
                ui.label(egui::RichText::new("Shared across every loaded structure.").small().weak());
            });
        self.show_style = open;
    }

    fn show_xyz_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_xyz;
        egui::Window::new("XYZ")
            .open(&mut open)
            .default_pos([320.0, 60.0])
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Unit:");
                    ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Angstrom, "Ang (Å)");
                    ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Bohr, "Bohr (a.u.)");
                });
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::AtomicNumberTable, "Standard");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::XyzFile, "Symbol XYZ");
                });
                let export_target = self
                    .active_structure
                    .and_then(|i| self.structures.get(i))
                    .and_then(|s| s.source_path.as_ref())
                    .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("fchk")))
                    .map(|p| p.with_extension("xyz"));

                if let Some(target) = &export_target {
                    ui.add_space(4.0);
                    let file_name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if ui
                        .add_sized([ui.available_width(), 28.0], egui::Button::new(format!("Save coordinates as {file_name}")))
                        .on_hover_text(target.display().to_string())
                        .clicked()
                    {
                        self.export_active_xyz(target.clone());
                    }
                }

                ui.separator();

                match self.active_structure.and_then(|i| self.structures.get(i)) {
                    Some(structure) => {
                        let text = format_coordinates(&structure.molecule, self.coordinate_unit, self.coordinate_format, &structure.label);
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(text).monospace())
                                    .selectable(true)
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                    }
                    None => {
                        ui.label("No structure loaded.");
                    }
                }
            });
        self.show_xyz = open;
    }

    fn show_visualization_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_visualization;
        egui::Window::new("Visualization")
            .open(&mut open)
            .default_pos([40.0, 460.0])
            .default_width(260.0)
            .show(ctx, |ui| {
                if self.selection_mode == SelectionMode::Measure {
                    ui.label(
                        egui::RichText::new("Measure mode is active (see Analysis) — clicking builds a measurement instead.")
                            .small()
                            .italics(),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Click atoms/bonds in the viewport to select them.")
                            .small()
                            .italics(),
                    );
                }

                ui.add_space(10.0);
                ui.separator();

                let Some(active) = self.active_structure else {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                };

                ui.label(egui::RichText::new("Atoms").strong());
                let summary = self.structures[active]
                    .selected_atoms
                    .iter()
                    .map(|&i| format!("{}{}", element_data(self.structures[active].molecule.atomic_numbers[i]).symbol, i + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(format!("Selected atoms: {}", self.structures[active].selected_atoms.len()));
                if !summary.is_empty() {
                    ui.label(egui::RichText::new(summary).small());
                }
                ui.horizontal(|ui| {
                    if ui.button("Hide atoms").clicked() {
                        if self.structures[active].selected_atoms.is_empty() {
                            self.show_warning("No atoms selected");
                        } else {
                            let selected = std::mem::take(&mut self.structures[active].selected_atoms);
                            self.structures[active].hidden_atoms.extend(selected);
                            self.rebuild_geometry();
                            self.rebuild_highlights();
                        }
                    }
                    if ui.button("Clear selection").clicked() {
                        self.clear_selection();
                    }
                });

                // Manual bond management: pick exactly two atoms and
                // either create a bond between them (useful for depicting
                // a forming/breaking contact too long for automatic
                // perception to catch) or toggle the visibility of one
                // that already exists.
                if self.structures[active].selected_atoms.len() == 2 {
                    let a = self.structures[active].selected_atoms[0];
                    let b = self.structures[active].selected_atoms[1];
                    let existing_bond = find_bond_between(&self.structures[active].molecule, a, b);

                    ui.add_space(6.0);
                    match existing_bond {
                        None => {
                            if ui.button("Create bond").clicked() {
                                self.structures[active].molecule.bonds.push(Bond { atom_a: a, atom_b: b });
                                self.structures[active].bond_styles.push(BondVisualStyle::Single);
                                self.rebuild_geometry();
                            }
                        }
                        Some(bond_index) => {
                            let is_hidden = self.structures[active].hidden_bonds.contains(&bond_index);
                            let label = if is_hidden { "Show bond" } else { "Hide bond" };
                            if ui.button(label).clicked() {
                                if is_hidden {
                                    self.structures[active].hidden_bonds.remove(&bond_index);
                                } else {
                                    self.structures[active].hidden_bonds.insert(bond_index);
                                }
                                self.rebuild_geometry();
                            }
                        }
                    }
                }

                ui.add_space(10.0);
                ui.separator();

                ui.label(egui::RichText::new("Bonds").strong());
                ui.label(format!("Selected bonds: {}", self.structures[active].selected_bonds.len()));
                ui.horizontal(|ui| {
                    if ui.button("Single").clicked() {
                        self.apply_bond_style(BondVisualStyle::Single);
                    }
                    if ui.button("TS").clicked() {
                        self.apply_bond_style(BondVisualStyle::TransitionState);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Hide bonds").clicked() {
                        if self.structures[active].selected_bonds.is_empty() {
                            self.show_warning("No bonds selected");
                        } else {
                            let selected = std::mem::take(&mut self.structures[active].selected_bonds);
                            self.structures[active].hidden_bonds.extend(selected);
                            self.rebuild_geometry();
                            self.rebuild_highlights();
                        }
                    }
                    if ui.button("Clear selection").clicked() {
                        self.clear_selection();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label("Global");
                ui.horizontal(|ui| {
                    if ui.button("Hide H-atoms").clicked() {
                        let hydrogens: Vec<usize> = self.structures[active]
                            .molecule
                            .atomic_numbers
                            .iter()
                            .enumerate()
                            .filter(|&(_, &z)| z == 1)
                            .map(|(i, _)| i)
                            .collect();
                        self.structures[active].hidden_atoms.extend(hydrogens);
                        self.rebuild_geometry();
                        self.rebuild_highlights();
                    }
                    if ui.button("Show all").clicked() {
                        let structure = &mut self.structures[active];
                        structure.hidden_atoms.clear();
                        structure.hidden_bonds.clear();
                        structure.bond_styles.iter_mut().for_each(|s| *s = BondVisualStyle::Single);
                        structure.selected_atoms.clear();
                        structure.selected_bonds.clear();
                        self.rebuild_geometry();
                        self.rebuild_highlights();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Generate orbitals").strong());

                if self.structures[active].wavefunction.is_some() {
                    let num_orbitals = self.structures[active].wavefunction.as_ref().unwrap().alpha.num_orbitals();
                    let is_unrestricted = self.structures[active].wavefunction.as_ref().unwrap().beta.is_some();
                    self.orbital_generation.range_start = self.orbital_generation.range_start.clamp(1, num_orbitals.max(1));
                    self.orbital_generation.range_end = self.orbital_generation.range_end.clamp(1, num_orbitals.max(1));

                    ui.horizontal(|ui| {
                        if ui.button("HOMO/LUMO").clicked() {
                            let wfn = self.structures[active].wavefunction.as_ref().unwrap();
                            let (alpha_homo, alpha_lumo, alpha_num) = (wfn.alpha.homo_index(), wfn.alpha.lumo_index(), wfn.alpha.num_orbitals());
                            let beta_homo_lumo_num = wfn.beta.as_ref().map(|b| (b.homo_index(), b.lumo_index(), b.num_orbitals()));
                            let structure = &mut self.structures[active];
                            structure.selected_alpha_mos.clear();
                            structure.selected_alpha_mos.insert(alpha_homo - 1);
                            if alpha_lumo <= alpha_num {
                                structure.selected_alpha_mos.insert(alpha_lumo - 1);
                            }
                            structure.selected_beta_mos.clear();
                            if let Some((beta_homo, beta_lumo, beta_num)) = beta_homo_lumo_num {
                                structure.selected_beta_mos.insert(beta_homo - 1);
                                if beta_lumo <= beta_num {
                                    structure.selected_beta_mos.insert(beta_lumo - 1);
                                }
                            }
                        }
                        if ui.button("Clear").clicked() {
                            let structure = &mut self.structures[active];
                            structure.selected_alpha_mos.clear();
                            structure.selected_beta_mos.clear();
                        }
                        ui.label("Isovalue:");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.isovalue).speed(0.001).range(0.0..=f32::MAX));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Range:");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.range_start).range(1..=num_orbitals.max(1)));
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut self.orbital_generation.range_end).range(1..=num_orbitals.max(1)));
                        if ui.button("Select range").clicked() {
                            let (lo, hi) = (self.orbital_generation.range_start.min(self.orbital_generation.range_end), self.orbital_generation.range_start.max(self.orbital_generation.range_end));
                            let structure = &mut self.structures[active];
                            for mo_number in lo..=hi {
                                structure.selected_alpha_mos.insert(mo_number - 1);
                                if is_unrestricted {
                                    structure.selected_beta_mos.insert(mo_number - 1);
                                }
                            }
                        }
                    });

                    let list_height = if is_unrestricted { 120.0 } else { 160.0 };
                    // Destructuring through the `&mut LoadedStructure`
                    // borrows `wavefunction`/`selected_alpha_mos`/
                    // `selected_beta_mos` as separate, disjoint fields —
                    // lets `wfn` (immutable) and the selection sets
                    // (mutable) coexist without needing two separate
                    // indexing expressions into `self.structures`, which
                    // the borrow checker can't prove disjoint on its own.
                    let LoadedStructure { wavefunction, selected_alpha_mos, selected_beta_mos, .. } = &mut self.structures[active];
                    let wfn = wavefunction.as_ref().unwrap();
                    show_mo_checklist(ui, &wfn.alpha, selected_alpha_mos, "alpha", list_height);
                    if let Some(beta) = &wfn.beta {
                        show_mo_checklist(ui, beta, selected_beta_mos, "beta", list_height);
                    }

                    ui.add_space(4.0);
                    ui.label("Accuracy:");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Low, "Low");
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Medium, "Medium");
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::High, "High");
                        ui.selectable_value(&mut self.orbital_generation.preset, OrbitalAccuracyPreset::Custom, "Custom");
                    });
                    if self.orbital_generation.preset == OrbitalAccuracyPreset::Custom {
                        ui.horizontal(|ui| {
                            ui.label("Spacing (Bohr):");
                            ui.add(egui::DragValue::new(&mut self.orbital_generation.custom_spacing_bohr).speed(0.01).range(0.02..=2.0));
                        });
                    } else {
                        ui.label(
                            egui::RichText::new(format!("Grid spacing: {:.2} Bohr", self.orbital_generation.resolve_spacing_bohr()))
                                .small()
                                .weak(),
                        );
                    }

                    ui.add_space(4.0);
                    let selected_count = self.structures[active].selected_alpha_mos.len() + self.structures[active].selected_beta_mos.len();
                    if ui
                        .add_sized([ui.available_width(), 26.0], egui::Button::new(format!("Generate {selected_count} orbital(s)")))
                        .clicked()
                    {
                        self.generate_selected_orbitals();
                    }
                } else {
                    ui.label(egui::RichText::new("This structure has no orbital data (open an .fchk with orbital output).").small().weak());
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Isosurfaces").strong());

                let has_isosurface = self.structures[active].isosurface.is_some();
                if has_isosurface {
                    if ui.button("Default").clicked() {
                        self.reset_active_isosurface_to_default();
                    }

                    let mut needs_recomposite = false;
                    {
                        let iso = self.structures[active].isosurface.as_mut().unwrap();
                        if ui.checkbox(&mut iso.show, "Show isosurface").changed() {
                            needs_recomposite = true;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Isovalue:");
                            ui.add(egui::DragValue::new(&mut iso.isovalue).speed(0.001).range(0.0..=f32::MAX));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Refinement:");
                            ui.selectable_value(&mut iso.refinement, 1, "1x");
                            ui.selectable_value(&mut iso.refinement, 2, "2x");
                            ui.selectable_value(&mut iso.refinement, 3, "3x");
                        });
                        if ui.checkbox(&mut iso.both_signs, "Both signs (+/-)").changed() {
                            needs_recomposite = true;
                        }
                        if ui.add(Slider::new(&mut iso.opacity, 0.05..=1.0).text("opacity")).changed() {
                            needs_recomposite = true;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Positive:");
                            if ui.color_edit_button_srgba(&mut iso.positive_color).changed() {
                                needs_recomposite = true;
                            }
                            ui.label("Negative:");
                            if ui.color_edit_button_srgba(&mut iso.negative_color).changed() {
                                needs_recomposite = true;
                            }
                        });
                        if ui.button("Invert colors").clicked() {
                            std::mem::swap(&mut iso.positive_color, &mut iso.negative_color);
                            needs_recomposite = true;
                        }
                    }
                    if needs_recomposite {
                        self.rebuild_isosurface();
                    }

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Isovalue/refinement/both-signs changes need Update surface (re-extraction is a real cost).")
                            .small()
                            .weak(),
                    );
                    if ui.add_sized([ui.available_width(), 26.0], egui::Button::new("Update surface")).clicked() {
                        self.extract_active_isosurface();
                    }
                } else {
                    ui.label(egui::RichText::new("This structure has no .cube isosurface.").small().weak());
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Keep surface").clicked() {
                        self.keep_active_isosurface();
                    }
                    if ui.button("Clean").clicked() {
                        self.clean_isosurfaces();
                    }
                });
                if !self.kept_isosurfaces.is_empty() {
                    ui.label(egui::RichText::new(format!("{} isosurface(s) kept", self.kept_isosurfaces.len())).small().weak());
                }

            });
        self.show_visualization = open;
    }

    fn show_analysis_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_analysis;
        egui::Window::new("Analysis")
            .open(&mut open)
            .default_pos([320.0, 460.0])
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.label("Selection mode");
                ui.horizontal(|ui| {
                    let previous_mode = self.selection_mode;
                    ui.selectable_value(&mut self.selection_mode, SelectionMode::Select, "Off");
                    ui.selectable_value(&mut self.selection_mode, SelectionMode::Measure, "Measure");
                    // Leaving Measure abandons whatever incomplete pick
                    // was in progress rather than carrying it over.
                    if previous_mode == SelectionMode::Measure && self.selection_mode != SelectionMode::Measure {
                        if let Some(active) = self.active_structure {
                            self.structures[active].pending_measurement.clear();
                        }
                    }
                });
                if self.selection_mode == SelectionMode::Measure {
                    ui.label(
                        egui::RichText::new("Click 2 atoms for a distance, 3 for an angle, 4 for a dihedral.")
                            .small()
                            .italics(),
                    );
                }

                ui.add_space(10.0);
                ui.separator();

                let Some(active) = self.active_structure else {
                    ui.label(egui::RichText::new("Open a structure first.").weak());
                    return;
                };

                let pending_len = self.structures[active].pending_measurement.len();
                let pending_summary = self.structures[active]
                    .pending_measurement
                    .iter()
                    .map(|&i| format!("{}{}", element_data(self.structures[active].molecule.atomic_numbers[i]).symbol, i + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(format!("Picking: {pending_len} atom(s)"));
                if !pending_summary.is_empty() {
                    ui.label(egui::RichText::new(pending_summary).small());
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(pending_len >= 2, egui::Button::new("Add")).clicked() {
                        let picks = std::mem::take(&mut self.structures[active].pending_measurement);
                        if let Some(kind) = MeasurementKind::from_picks(&picks) {
                            self.structures[active].measurements.push(Measurement { kind, label_offset: egui::Vec2::ZERO });
                            self.rebuild_measurements();
                        }
                    }
                    if ui.button("Clear picking").clicked() {
                        self.structures[active].pending_measurement.clear();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label("Measurements");
                if self.structures[active].measurements.is_empty() {
                    ui.label(egui::RichText::new("None yet.").weak());
                } else {
                    let mut remove_index = None;
                    for (index, measurement) in self.structures[active].measurements.iter().enumerate() {
                        let value = measure(&self.structures[active].molecule, measurement.kind);
                        let atoms = format_measurement_atoms(&self.structures[active].molecule, measurement.kind);
                        let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
                        ui.horizontal(|ui| {
                            ui.label(format!("{atoms}: {text}"));
                            if ui.small_button("×").clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove_index {
                        self.structures[active].measurements.remove(index);
                        self.rebuild_measurements();
                    }
                    if ui.button("Clear all").clicked() {
                        self.structures[active].measurements.clear();
                        self.rebuild_measurements();
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label("Label style");
                ui.add(Slider::new(&mut self.measurement_style.font_size, 8.0..=32.0).text("font size"));
                ui.checkbox(&mut self.measurement_style.bold, "Bold");
                let mut decimals = self.measurement_style.decimal_places as u32;
                if ui.add(Slider::new(&mut decimals, 0..=4).text("decimal places")).changed() {
                    self.measurement_style.decimal_places = decimals as usize;
                }
                ui.horizontal(|ui| {
                    ui.label("Text color:");
                    ui.color_edit_button_srgba(&mut self.measurement_style.text_color);
                });
                let mut line_color = Color32::from_rgb(
                    (self.measurement_style.line_color[0] * 255.0) as u8,
                    (self.measurement_style.line_color[1] * 255.0) as u8,
                    (self.measurement_style.line_color[2] * 255.0) as u8,
                );
                ui.horizontal(|ui| {
                    ui.label("Line color:");
                    if ui.color_edit_button_srgba(&mut line_color).changed() {
                        self.measurement_style.line_color =
                            [line_color.r() as f32 / 255.0, line_color.g() as f32 / 255.0, line_color.b() as f32 / 255.0];
                        self.rebuild_measurements();
                    }
                });
                ui.label(egui::RichText::new("Drag a label in the viewport to reposition it.").small().weak());
            });
        self.show_analysis = open;
    }

    fn apply_bond_style(&mut self, style: BondVisualStyle) {
        let Some(active) = self.active_structure else { return };
        if self.structures[active].selected_bonds.is_empty() {
            self.show_warning("No bonds selected");
            return;
        }
        let selected = self.structures[active].selected_bonds.clone();
        for index in selected {
            if let Some(entry) = self.structures[active].bond_styles.get_mut(index) {
                *entry = style;
            }
        }
        self.rebuild_geometry();
    }

    fn show_about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("About APOST3Dview")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let logo_size = self.logo_texture.size_vec2();
                    let display_size = logo_size * (240.0 / logo_size.x);
                    ui.image((self.logo_texture.id(), display_size));

                    ui.add_space(8.0);
                    ui.label(format!("APOST3Dview v{}", env!("CARGO_PKG_VERSION")));
                    ui.label("A molecular visualizer for APOST-3D.");
                    ui.add_space(8.0);
                    ui.label("Martí Gimferrer");
                    ui.hyperlink_to("mgimferrer18@gmail.com", "mailto:mgimferrer18@gmail.com");
                    ui.add_space(8.0);
                    ui.label(
                        "Sister project to APOST-3D, a software to extract state-of-the-art \
                         chemical bonding indicators from wavefunction analysis",
                    );
                });
            });
        self.show_about = open;
    }

    fn show_warning_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let Some((message, color, shown_at)) = &self.warning else { return };
        if shown_at.elapsed() > WARNING_DURATION {
            self.warning = None;
            return;
        }

        egui::Area::new(egui::Id::new("warning_toast"))
            .fixed_pos(egui::pos2(rect.center().x - 90.0, rect.top() + 20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(*color).show(ui, |ui| {
                    ui.label(egui::RichText::new(message).color(Color32::WHITE).strong());
                });
            });

        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn show_empty_state(&self, ui: &egui::Ui, rect: egui::Rect) {
        let logo_size = self.logo_texture.size_vec2();
        let max_width = (rect.width() * 0.22).min(logo_size.x);
        let scale = max_width / logo_size.x;
        let display_size = logo_size * scale;

        let logo_rect = egui::Rect::from_center_size(rect.center() - egui::vec2(0.0, 16.0), display_size);
        ui.painter().image(
            self.logo_texture.id(),
            logo_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::from_white_alpha(220),
        );

        ui.painter().text(
            egui::pos2(rect.center().x, logo_rect.bottom() + 28.0),
            egui::Align2::CENTER_CENTER,
            "Open a .fchk or .xyz file from the Structures panel to get started",
            egui::FontId::proportional(15.0),
            Color32::from_gray(130),
        );
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.start_time.elapsed() < SPLASH_MIN_DURATION {
            self.show_splash(ui);
            return;
        }

        self.show_toolbar(ui);
        self.show_structures_window(ui.ctx());
        self.show_style_window(ui.ctx());
        self.show_xyz_window(ui.ctx());
        self.show_visualization_window(ui.ctx());
        self.show_analysis_window(ui.ctx());
        self.show_render_window(ui.ctx());
        self.show_about_window(ui.ctx());

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.max_rect();

                let background = self.material.background;
                let bg_color = Color32::from_rgb(
                    (background[0] * 255.0) as u8,
                    (background[1] * 255.0) as u8,
                    (background[2] * 255.0) as u8,
                );
                ui.painter().rect_filled(rect, 0.0, bg_color);

                let Some(active) = self.active_structure else {
                    self.show_empty_state(ui, rect);
                    return;
                };

                let response = ui.interact(
                    rect,
                    ui.id().with("viewport"),
                    egui::Sense::click_and_drag(),
                );

                // Right-click drags the camera; left-click is reserved
                // entirely for selection (below) — `dragged()` alone
                // doesn't distinguish which button caused it, so this has
                // to be `dragged_by` the specific button.
                let camera_dragging = response.dragged_by(egui::PointerButton::Secondary);
                let drag_delta = response.drag_delta();
                if camera_dragging {
                    if ui.input(|i| i.modifiers.shift) {
                        self.camera.pan(-drag_delta.x * 0.01, drag_delta.y * 0.01);
                    } else {
                        self.camera.orbit(-drag_delta.x * 0.005, -drag_delta.y * 0.005);
                    }
                }
                let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll_delta != 0.0 {
                    self.camera.zoom(scroll_delta * 0.02);
                }

                // Arrow keys orbit continuously while held, at a speed
                // independent of frame rate.
                const ARROW_ROTATE_SPEED: f32 = 1.2;
                let (key_yaw, key_pitch, dt) = ui.input(|i| {
                    let mut yaw = 0.0;
                    let mut pitch = 0.0;
                    if i.key_down(egui::Key::ArrowLeft) {
                        yaw -= 1.0;
                    }
                    if i.key_down(egui::Key::ArrowRight) {
                        yaw += 1.0;
                    }
                    if i.key_down(egui::Key::ArrowUp) {
                        pitch += 1.0;
                    }
                    if i.key_down(egui::Key::ArrowDown) {
                        pitch -= 1.0;
                    }
                    (yaw, pitch, i.stable_dt)
                });
                let keyboard_rotating = key_yaw != 0.0 || key_pitch != 0.0;
                if keyboard_rotating {
                    self.camera.orbit(key_yaw * ARROW_ROTATE_SPEED * dt, key_pitch * ARROW_ROTATE_SPEED * dt);
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.clear_selection();
                }

                let aspect_ratio = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };
                self.last_aspect_ratio = aspect_ratio;

                // Left-click always does something useful: in Measure
                // mode it extends the pending pick, otherwise it selects
                // whatever's under the cursor — an atom if one's there,
                // a bond if not — for Visualization's hide/style actions.
                // No separate "Atoms/Bonds/Off" mode needed for that
                // anymore, since a click can only ever mean one thing at
                // a time regardless.
                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let ndc_x = ((pos.x - rect.left()) / rect.width()) * 2.0 - 1.0;
                        let ndc_y = 1.0 - ((pos.y - rect.top()) / rect.height()) * 2.0;
                        let (origin, direction) = ray_from_ndc(&self.camera, aspect_ratio, ndc_x, ndc_y);

                        let structure = &self.structures[active];
                        let atom_hit = pick_atom(&structure.molecule, origin, direction, self.material.atom_scale, &structure.hidden_atoms);
                        let bond_hit = if atom_hit.is_none() && self.selection_mode != SelectionMode::Measure {
                            pick_bond(
                                &structure.molecule,
                                origin,
                                direction,
                                self.material.bond_radius,
                                &structure.hidden_atoms,
                                &structure.hidden_bonds,
                            )
                        } else {
                            None
                        };

                        if self.selection_mode == SelectionMode::Measure {
                            if let Some(index) = atom_hit {
                                let pending = &mut self.structures[active].pending_measurement;
                                if pending.len() < 4 {
                                    pending.push(index);
                                }
                                self.rebuild_highlights();
                            }
                        } else if let Some(index) = atom_hit {
                            toggle_selected(&mut self.structures[active].selected_atoms, index);
                            self.rebuild_highlights();
                        } else if let Some(index) = bond_hit {
                            toggle_selected(&mut self.structures[active].selected_bonds, index);
                            self.rebuild_highlights();
                        }
                    }
                }

                // True 3D labels: real depth-tested billboard glyph quads
                // (see crates/render/src/{glyphs,label,shaders/text.wgsl}),
                // not a 2D UI overlay — so a bond or atom nearer the camera
                // correctly, even partially, occludes them. Built fresh
                // each repaint and handed to the wgpu callback below.
                // Atom labels are sized in world units (scale with zoom,
                // like the atom itself); measurement labels still hold a
                // constant apparent screen size, which depends on distance
                // from the camera and so is also recomputed every frame.
                let mut label_instances: Vec<GlyphInstance> = Vec::new();

                if self.atom_label_mode != AtomLabelMode::None {
                    let structure = &self.structures[active];
                    let color = color32_to_rgb(self.atom_label_style.text_color);
                    for (index, (&z, &position)) in structure.molecule.atomic_numbers.iter().zip(&structure.molecule.positions).enumerate()
                    {
                        if structure.hidden_atoms.contains(&index) {
                            continue;
                        }
                        // A real world-space size, tied to this atom's own
                        // rendered radius — labels are true 3D geometry, so
                        // they grow and shrink with zoom exactly like the
                        // atom they're attached to, rather than holding a
                        // constant on-screen size the way a 2D overlay would.
                        let radius = element_data(z).vdw_radius * self.material.atom_scale;
                        let scale = glyph_scale_for_world_size(radius * self.atom_label_style.relative_size);
                        let text = match self.atom_label_mode {
                            AtomLabelMode::Number => format!("{}", index + 1),
                            AtomLabelMode::Type => element_data(z).symbol.to_string(),
                            AtomLabelMode::NumberType => format!("{}{}", element_data(z).symbol, index + 1),
                            AtomLabelMode::None => unreachable!(),
                        };
                        // The label anchor starts at the atom's own 3D
                        // center, but the sphere impostor's near surface
                        // renders a full radius closer to the camera than
                        // that — so a label sitting exactly at the center
                        // is depth-occluded by its own atom. Push the
                        // anchor toward the camera past the sphere's near
                        // surface (radius + a little clearance) so the
                        // label wins the depth test against its own atom.
                        let to_camera = (self.camera.eye() - position).normalize_or_zero();
                        let label_anchor = position + to_camera * (radius * 1.15 + 0.02);
                        push_label(&mut label_instances, &self.glyph_atlas, &text, label_anchor, scale, color, EDGE_BIAS_ATOM_LABEL);
                    }
                }

                // Measurement labels still live in screen space for the
                // *offset* the user drags them by (simplest way to keep
                // "nudge it N pixels clear of the clutter" behavior
                // whatever the current zoom/rotation) — but that offset
                // is converted to a world-space position every frame
                // before rendering, same as the atom labels above, so
                // it's still real, depth-tested 3D geometry.
                let mut label_updates: Vec<(usize, egui::Vec2)> = Vec::new();
                let (camera_right, camera_up) = self.camera.screen_basis();
                let measurement_color = color32_to_rgb(self.measurement_style.text_color);
                for (index, measurement) in self.structures[active].measurements.iter().enumerate() {
                    let anchor = measurement_anchor(&self.structures[active].molecule, measurement.kind);
                    let distance = (anchor - self.camera.eye()).length();
                    let world_per_pixel = self.camera.world_units_per_pixel(distance, rect.height());
                    let world_offset = camera_right * (measurement.label_offset.x * world_per_pixel)
                        - camera_up * (measurement.label_offset.y * world_per_pixel);
                    let final_anchor = anchor + world_offset;

                    let value = measure(&self.structures[active].molecule, measurement.kind);
                    let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
                    let scale = glyph_scale_for_font_size(self.measurement_style.font_size, world_per_pixel);
                    let edge_bias = if self.measurement_style.bold { EDGE_BIAS_BOLD } else { EDGE_BIAS_NORMAL };
                    push_label(&mut label_instances, &self.glyph_atlas, &text, final_anchor, scale, measurement_color, edge_bias);

                    // Hit-test area for dragging: approximate (text length
                    // × font size), since there's no 2D layout pass to get
                    // an exact box from anymore.
                    if let Some(screen_pos) = project_to_screen(&self.camera, aspect_ratio, rect, final_anchor) {
                        let approx_width = text.chars().count() as f32 * self.measurement_style.font_size * 0.55;
                        let approx_height = self.measurement_style.font_size * 1.3;
                        let hit_rect = egui::Rect::from_center_size(screen_pos, egui::vec2(approx_width, approx_height));
                        let label_response = ui.interact(hit_rect, ui.id().with(("measurement_label", active, index)), egui::Sense::drag());
                        if label_response.dragged() {
                            label_updates.push((index, measurement.label_offset + label_response.drag_delta()));
                        }
                    }
                }
                let any_label_dragged = !label_updates.is_empty();
                for (index, offset) in label_updates {
                    if let Some(measurement) = self.structures[active].measurements.get_mut(index) {
                        measurement.label_offset = offset;
                    }
                }

                // "Phase C" progressive AO quality: cheap every frame the
                // camera is actually moving (orbit/pan/zoom) *or* a slider
                // just changed, a one-time full-quality recompute the
                // instant both stop changing, then no recompute at all on
                // further idle frames — see the `ao_last_camera`/
                // `ao_last_settings`/`ao_settled` field docs. The extra
                // `request_repaint` on a just-changed frame is what
                // guarantees a follow-up frame actually happens to catch
                // "it settled" — egui doesn't keep repainting once input
                // stops, so without this the settle transition would only
                // ever fire by coincidence.
                let ao_recompute_samples = if !self.ao_enabled {
                    None
                } else if self.ao_last_camera != Some(self.camera) || self.ao_last_settings != Some(self.ao_settings) {
                    self.ao_last_camera = Some(self.camera);
                    self.ao_last_settings = Some(self.ao_settings);
                    self.ao_settled = false;
                    ui.ctx().request_repaint();
                    Some(apost3dview_render::AO_LIVE_SAMPLE_COUNT)
                } else if !self.ao_settled {
                    self.ao_settled = true;
                    Some(apost3dview_render::AO_KERNEL_SIZE as u32)
                } else {
                    None
                };

                let pixels_per_point = ui.ctx().pixels_per_point();
                let callback = ViewportCallback {
                    camera: self.camera,
                    material: self.ao_render_material(),
                    aspect_ratio,
                    label_instances,
                    ambient_occlusion: self.ao_enabled.then_some(self.ao_settings),
                    viewport_size_px: [(rect.width() * pixels_per_point).round() as u32, (rect.height() * pixels_per_point).round() as u32],
                    // `rect` is the 3D viewport's own position within the
                    // full window (in egui's logical points) — the AO
                    // shaders need it in physical pixels, matching what
                    // `@builtin(position)` actually reports (see
                    // `ViewportCallback::viewport_offset_px`).
                    viewport_offset_px: [rect.min.x * pixels_per_point, rect.min.y * pixels_per_point],
                    ao_recompute_samples,
                    depth_of_field: self.dof_enabled.then_some(self.dof_settings),
                    dof_focus_distance: self.camera.distance,
                    background: self.material.background,
                };
                ui.painter()
                    .add(egui_wgpu::Callback::new_paint_callback(rect, callback));

                self.show_warning_overlay(ui.ctx(), rect);

                if camera_dragging || scroll_delta != 0.0 || keyboard_rotating || any_label_dragged {
                    ui.ctx().request_repaint();
                }
            });
    }
}
