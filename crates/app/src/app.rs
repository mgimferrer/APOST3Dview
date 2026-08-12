use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apost3dview_core::{element_data, format_coordinates, measure, parse_xyz, Bond, CoordinateFormat, LengthUnit, MeasurementKind, Molecule};
use apost3dview_render::{
    glyph_scale_for_font_size, glyph_scale_for_world_size, layout_label, pick_atom, pick_bond, ray_from_ndc, BondVisualStyle,
    GlyphAtlas, GlyphInstance, Material, OrbitCamera, ViewportCallback, ViewportResources,
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

/// One opened structure — its own geometry and its own hide/selection/
/// bond-style state. Deliberately does NOT own a Style/Material — that
/// stays a single value shared across every structure, so tuning it once
/// applies everywhere instead of needing to be redone per file.
struct LoadedStructure {
    label: String,
    molecule: Molecule,
    hidden_atoms: HashSet<usize>,
    hidden_bonds: HashSet<usize>,
    bond_styles: Vec<BondVisualStyle>,
    selected_atoms: Vec<usize>,
    selected_bonds: Vec<usize>,
    measurements: Vec<Measurement>,
    /// Ordered atom picks awaiting a commit (via the Analysis window's
    /// "Add" button) into `measurements`.
    pending_measurement: Vec<usize>,
}

impl LoadedStructure {
    fn new(label: String, molecule: Molecule) -> Self {
        let bond_styles = vec![BondVisualStyle::Single; molecule.bonds.len()];
        Self {
            label,
            molecule,
            hidden_atoms: HashSet::new(),
            hidden_bonds: HashSet::new(),
            bond_styles,
            selected_atoms: Vec::new(),
            selected_bonds: Vec::new(),
            measurements: Vec::new(),
            pending_measurement: Vec::new(),
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

    coordinate_unit: LengthUnit,
    coordinate_format: CoordinateFormat,
    measurement_style: MeasurementStyle,
    atom_label_mode: AtomLabelMode,
    atom_label_style: AtomLabelStyle,

    selection_mode: SelectionMode,
    warning: Option<(String, Instant)>,
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
            show_style: true,
            show_xyz: false,
            show_visualization: false,
            show_analysis: false,
            show_structures: true,
            show_about: false,
            coordinate_unit: LengthUnit::Angstrom,
            coordinate_format: CoordinateFormat::AtomicNumberTable,
            measurement_style: MeasurementStyle::default(),
            atom_label_mode: AtomLabelMode::None,
            atom_label_style: AtomLabelStyle::default(),
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

    fn show_warning(&mut self, message: impl Into<String>) {
        self.warning = Some((message.into(), Instant::now()));
    }

    fn clear_selection(&mut self) {
        if let Some(active) = self.active_structure {
            self.structures[active].selected_atoms.clear();
            self.structures[active].selected_bonds.clear();
        }
        self.rebuild_highlights();
    }

    /// Switches the active structure and re-frames the camera on it —
    /// different opened files can be wildly different sizes/positions
    /// (this is the main reason .xyz support exists: comparing unrelated
    /// structures side by side, not orbital sets sharing one geometry —
    /// that case, later, will want the opposite: freezing orientation).
    fn set_active(&mut self, index: usize) {
        self.active_structure = Some(index);
        if let Some(structure) = self.structures.get(index) {
            let (center, radius) = structure.molecule.bounding_sphere();
            self.camera.frame_bounds(center, radius);
        }
        self.rebuild_geometry();
        self.rebuild_highlights();
        self.rebuild_measurements();
    }

    fn open_fchk(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Gaussian checkpoint", &["fchk"]).pick_file() else { return };
        match Molecule::from_fchk(&path) {
            Ok(molecule) => {
                let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled.fchk".into());
                let index = self.structures.len();
                self.structures.push(LoadedStructure::new(label, molecule));
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
                    self.structures.push(LoadedStructure::new(label, molecule));
                    first_new_index.get_or_insert(index);
                }
                Err(err) => self.show_warning(format!("Could not load {}: {err}", path.display())),
            }
        }
        if let Some(index) = first_new_index {
            self.set_active(index);
        }
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
                    }
                    if ui.button("Publication").clicked() {
                        self.material = Material::publication();
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
                ui.add(Slider::new(&mut self.material.diffuse, 0.0..=1.0).text("diffuse"));
                ui.add(Slider::new(&mut self.material.specular, 0.0..=1.0).text("specular"));
                ui.add(Slider::new(&mut self.material.shininess, 1.0..=128.0).text("shininess"));

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
        let Some((message, shown_at)) = &self.warning else { return };
        if shown_at.elapsed() > WARNING_DURATION {
            self.warning = None;
            return;
        }

        egui::Area::new(egui::Id::new("warning_toast"))
            .fixed_pos(egui::pos2(rect.center().x - 90.0, rect.top() + 20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::from_rgb(196, 60, 40)).show(ui, |ui| {
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

                let callback = ViewportCallback {
                    camera: self.camera,
                    material: self.material,
                    aspect_ratio,
                    label_instances,
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
