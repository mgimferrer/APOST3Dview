//! The application state and frame loop.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apost3dview_core::{CoordinateFormat, LengthUnit};
use apost3dview_render::{AoSettings, DofSettings, GlyphAtlas, IsosurfaceMaterial, Material, OrbitCamera, ViewportResources};
use egui::{Color32, RichText};
use glam::Vec2;

use crate::export::RenderExportState;
use crate::files::FileKind;
use crate::isosurfaces::SurfaceSettings;
use crate::orbitals::OrbitalGenerationState;
use crate::presets::list_style_presets;
use crate::state::{AtomLabelMode, AtomLabelStyle, LoadedStructure, MeasurementStyle};
use crate::windows::ToolWindow;
use crate::viewport::DEFAULT_AUTO_SPIN_SPEED;

/// How long the splash screen stays up at most. A click dismisses it
/// earlier.
pub(crate) const SPLASH_DURATION: Duration = Duration::from_millis(1500);

/// Warnings stay up longer than plain status messages: they usually say
/// something the user has to act on.
const WARNING_TOAST_DURATION: Duration = Duration::from_secs(7);
const STATUS_TOAST_DURATION: Duration = Duration::from_secs(3);
const MAX_TOASTS: usize = 4;

/// A short message over the top of the viewport. Several can be up at
/// once, newest at the bottom.
pub(crate) struct Toast {
    text: String,
    color: Color32,
    shown_at: Instant,
    duration: Duration,
}

/// Choices made before the window opens (`main.rs`, tests).
pub struct StartupOptions {
    /// Opened as soon as the splash screen is gone, as if dropped onto the
    /// window — the command-line arguments.
    pub files: Vec<PathBuf>,
    pub splash: bool,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self { files: Vec::new(), splash: true }
    }
}

pub(crate) fn load_texture(ctx: &egui::Context, name: &str, png_bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(png_bytes).expect("bundled image should be valid").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

pub struct App {
    pub(crate) camera: OrbitCamera,
    /// Continuous rotation, toggled with the Space bar or the
    /// status bar. Pauses while the user is rotating by hand.
    pub(crate) auto_spin: bool,
    /// Radians per second.
    pub(crate) auto_spin_speed: f32,
    /// Screen-space direction of auto-spin (x = horizontal, y = vertical),
    /// unit length — follows whichever arrow key(s) were pressed last, so
    /// tapping e.g. right+up then Space spins diagonally.
    pub(crate) spin_direction: Vec2,
    pub(crate) material: Material,
    pub(crate) structures: Vec<LoadedStructure>,
    pub(crate) active_structure: Option<usize>,
    pub(crate) logo_texture: egui::TextureHandle,
    /// Also handed to `ViewportResources` at construction (for the GPU
    /// texture) — kept here too since label layout is a CPU-side
    /// computation done every frame in this crate, not inside the
    /// renderer.
    pub(crate) glyph_atlas: Arc<GlyphAtlas>,
    pub(crate) start_time: Instant,
    pub(crate) splash: bool,
    /// Files to open once the splash screen is gone (see `StartupOptions`).
    pub(crate) pending_files: Vec<PathBuf>,
    pub(crate) render_state: egui_wgpu::RenderState,

    // Each tool is a floating window toggled from the toolbar (Coordinates
    // from the Molecule window), grouped by task: what the molecule looks
    // like, its surfaces, measuring, APOST-3D results, the look of the
    // scene, exporting.
    pub(crate) show_molecule: bool,
    pub(crate) show_surfaces: bool,
    pub(crate) show_measure: bool,
    pub(crate) show_apost: bool,
    pub(crate) show_appearance: bool,
    pub(crate) show_export: bool,
    pub(crate) show_coordinates: bool,
    pub(crate) show_about: bool,

    pub(crate) coordinate_unit: LengthUnit,
    pub(crate) coordinate_format: CoordinateFormat,
    pub(crate) measurement_style: MeasurementStyle,
    pub(crate) atom_label_mode: AtomLabelMode,
    pub(crate) atom_label_style: AtomLabelStyle,
    pub(crate) render_export: RenderExportState,
    pub(crate) orbital_generation: OrbitalGenerationState,
    /// Updated every viewport repaint from the actual on-screen rect —
    /// the Export window reads this (rather than recomputing it itself,
    /// since it doesn't have direct access to the viewport rect) so the
    /// DPI preset can derive its height from the current on-screen view.
    pub(crate) last_aspect_ratio: f32,
    /// The live view's height in points, updated with `last_aspect_ratio`:
    /// what exports scale their labels against (`scene_labels`).
    pub(crate) last_viewport_height: f32,

    /// Isosurface lighting response — deliberately separate from
    /// `material` (the atom/bond one), shared across every structure the
    /// same way `material` is.
    pub(crate) isosurface_material: IsosurfaceMaterial,
    /// Ambient occlusion — a single shared toggle/settings pair driving
    /// both the live view and export (see `ao_render_material` and
    /// `ViewportResources::run_ao_passes`), so tuning the Appearance-window
    /// sliders shows the result live instead of only after a render.
    pub(crate) ao_enabled: bool,
    pub(crate) ao_settings: AoSettings,
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
    pub(crate) ao_last_camera: Option<OrbitCamera>,
    pub(crate) ao_last_settings: Option<AoSettings>,
    pub(crate) ao_settled: bool,
    /// Depth of field — same shared live+export toggle/settings shape as
    /// AO, but with no settle-tiering of its own: unlike AO's SSAO sample
    /// count, DoF's blur is cheap enough to just rerun at full quality
    /// every frame the viewport actually redraws (it reuses AO's own
    /// settle cadence only for the AO sub-pass it runs internally when
    /// both are on — see `ViewportResources::run_live_dof_pass`).
    pub(crate) dof_enabled: bool,
    pub(crate) dof_settings: DofSettings,
    /// How newly opened or generated surfaces are drawn: the settings of
    /// the surface last edited, so a batch of orbitals comes out like the
    /// one just tuned.
    pub(crate) surface_defaults: SurfaceSettings,

    /// Names of every saved Custom style preset on disk (see
    /// `presets_dir`), cached so the Appearance window doesn't re-read the
    /// directory every frame — refreshed after a save or delete.
    pub(crate) style_presets: Vec<String>,
    /// Text field buffer for naming a new Custom preset before saving.
    pub(crate) new_preset_name: String,

    pub(crate) toasts: Vec<Toast>,
    /// Fixed color ranges chosen by the user, by quantity id, shared by
    /// every structure so figures of different systems compare.
    pub(crate) color_ranges: std::collections::BTreeMap<String, apost3dview_render::ColorScale>,
    /// List, in the Show menu, the values APOST-3D prints only for
    /// comparison (Mulliken's, in a run with another atomic definition)
    /// and the secondary ones (`results_view::is_secondary`).
    pub(crate) list_other_values: bool,
    /// Windows asked for since the last frame, to show expanded
    /// (`open_window`).
    pub(crate) windows_to_expand: Vec<crate::windows::ToolWindow>,
    /// Where the tool windows go (`windows/column.rs`).
    pub(crate) window_column: crate::windows::column::WindowColumn,
    /// The table chosen in the APOST-3D window's Tables section (entry id).
    pub(crate) apost_table: Option<String>,
    /// The APOST-3D window's part shown; `None`: the new input for a
    /// structure without results (loaded or next to it), the results
    /// otherwise.
    pub(crate) apost_tab: Option<crate::input_generator::ApostTab>,
    /// Where an Alt/Option-drag box selection started, while one is drawn.
    pub(crate) box_select_start: Option<egui::Pos2>,
    /// What the results display was last built for (see
    /// `update_results_display`).
    pub(crate) results_signature: String,
    /// The session file last opened or saved: where Save session writes,
    /// and what the window title names.
    pub(crate) session_path: Option<PathBuf>,
    /// The window title last sent, to send it again only when it changes.
    shown_title: String,
    /// "Keep the view when switching structures": the next structure is
    /// seen with the same orientation and zoom, centered on it (see
    /// `view_for`).
    pub(crate) keep_view: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_options(cc, StartupOptions::default())
    }

    pub fn with_options(cc: &eframe::CreationContext<'_>, options: StartupOptions) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("eframe must be running with the wgpu backend");

        let info = render_state.adapter.get_info();
        log::info!("graphics: {} ({:?}, {:?}), format {:?}", info.name, info.backend, info.device_type, render_state.target_format);
        let glyph_atlas = Arc::new(GlyphAtlas::new(&render_state.device, &render_state.queue));
        let resources = ViewportResources::new(&render_state.device, render_state.target_format, &glyph_atlas);
        render_state.renderer.write().callback_resources.insert(resources);
        log::info!("render pipelines ready");

        let logo_texture = load_texture(&cc.egui_ctx, "apost3d_logo", include_bytes!("../assets/logo.png"));
        // Every launch starts with the default window layout (the column,
        // all windows the same size): what an earlier version saved of the
        // windows' sizes and states is dropped, and nothing is saved anymore
        // (`persist_egui_memory`).
        cc.egui_ctx.memory_mut(|memory| memory.data.clear());

        Self {
            // No perspective by default, as in most figures of papers.
            camera: OrbitCamera { orthographic: true, ..OrbitCamera::default() },
            auto_spin: false,
            auto_spin_speed: DEFAULT_AUTO_SPIN_SPEED,
            spin_direction: Vec2::X,
            material: Material::default(),
            structures: Vec::new(),
            active_structure: None,
            logo_texture,
            glyph_atlas,
            start_time: Instant::now(),
            splash: options.splash,
            pending_files: options.files,
            render_state,
            show_molecule: true,
            show_surfaces: false,
            show_measure: false,
            show_apost: false,
            show_appearance: false,
            show_export: false,
            show_coordinates: false,
            show_about: false,
            coordinate_unit: LengthUnit::Angstrom,
            coordinate_format: CoordinateFormat::AtomicNumberTable,
            measurement_style: MeasurementStyle::default(),
            atom_label_mode: AtomLabelMode::None,
            atom_label_style: AtomLabelStyle::default(),
            render_export: RenderExportState::default(),
            orbital_generation: OrbitalGenerationState::default(),
            last_aspect_ratio: 16.0 / 9.0,
            last_viewport_height: 860.0,
            isosurface_material: IsosurfaceMaterial::default(),
            ao_enabled: true,
            ao_settings: AoSettings::default(),
            ao_last_camera: None,
            ao_last_settings: None,
            ao_settled: false,
            dof_enabled: true,
            dof_settings: DofSettings::default(),
            surface_defaults: SurfaceSettings::default(),
            style_presets: list_style_presets(),
            new_preset_name: String::new(),
            toasts: Vec::new(),
            color_ranges: Default::default(),
            list_other_values: false,
            windows_to_expand: Vec::new(),
            window_column: Default::default(),
            apost_table: None,
            apost_tab: None,
            box_select_start: None,
            results_signature: String::new(),
            session_path: None,
            shown_title: String::new(),
            keep_view: false,
        }
    }

    pub(crate) fn rebuild_geometry(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let bond_scales = self.bond_scales(structure);
        self.with_resources(|resources| {
            resources.update_geometry(
                &self.render_state.device,
                &structure.molecule,
                &structure.hidden_atoms,
                &structure.hidden_bonds,
                &structure.bond_styles,
                None,
                bond_scales.as_deref(),
            );
        });
    }

    /// Runs `f` on the viewport's GPU resources, if the renderer has them.
    pub(crate) fn with_resources<R>(&self, f: impl FnOnce(&mut ViewportResources) -> R) -> Option<R> {
        let mut renderer = self.render_state.renderer.write();
        renderer.callback_resources.get_mut::<ViewportResources>().map(f)
    }

    pub(crate) fn rebuild_highlights(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        self.with_resources(|resources| resources.update_highlights(&self.render_state.device, &structure.molecule, &structure.selected_atoms, &structure.selected_bonds));
    }

    pub(crate) fn rebuild_measurements(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let segments: Vec<(usize, usize)> = structure.measurements.iter().flat_map(|m| m.kind.segments()).collect();
        self.with_resources(|resources| resources.update_measurements(&self.render_state.device, &structure.molecule, &segments, self.measurement_style.line_color));
    }

    pub(crate) fn show_warning(&mut self, message: impl Into<String>) {
        self.push_toast(message.into(), Color32::from_rgb(196, 60, 40), WARNING_TOAST_DURATION);
    }

    pub(crate) fn show_status(&mut self, message: impl Into<String>) {
        self.push_toast(message.into(), Color32::from_rgb(45, 130, 80), STATUS_TOAST_DURATION);
    }

    fn push_toast(&mut self, text: String, color: Color32, duration: Duration) {
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast { text, color, shown_at: Instant::now(), duration });
        if self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
    }

    /// Texts of the messages currently up — for tests.
    pub fn toast_texts(&self) -> Vec<String> {
        self.toasts.iter().map(|t| t.text.clone()).collect()
    }

    /// Takes down every message at once, as if their time were up — for
    /// tests (the guide's pictures are taken without them).
    pub fn clear_toasts(&mut self) {
        self.toasts.clear();
    }

    /// Turns the view about the screen's axes (radians), as dragging would
    /// — for tests and the guide's pictures.
    pub fn turn_view(&mut self, yaw: f32, pitch: f32) {
        self.camera.orbit(yaw, pitch);
    }

    /// Turns the view to `orientation` (a quaternion, x y z w: screen
    /// right, up and toward the viewer are its X, Y and Z), keeping what
    /// it looks at and from how far — for the guide's pictures, which set
    /// an orientation computed from the atoms.
    pub fn set_view_orientation(&mut self, orientation: [f32; 4]) {
        self.camera.orientation = glam::Quat::from_array(orientation).normalize();
    }

    /// Moves the camera closer (`factor` below 1) or farther, as zooming
    /// would — for tests and the guide's pictures.
    pub fn zoom_view(&mut self, factor: f32) {
        self.camera.distance *= factor;
    }

    /// Names of the open structures, in list order — for tests.
    pub fn structure_labels(&self) -> Vec<String> {
        self.structures.iter().map(|s| s.label.clone()).collect()
    }

    /// The active structure's surfaces, with whether each is shown — for
    /// tests.
    pub fn surface_labels(&self) -> Vec<(String, bool)> {
        self.active_structure.and_then(|i| self.structures.get(i)).map(|s| s.surfaces.iter().map(|x| (x.label.clone(), x.visible)).collect()).unwrap_or_default()
    }

    /// The active structure's surfaces' opacities (1 = opaque), in list
    /// order — for tests.
    pub fn surface_opacities(&self) -> Vec<f32> {
        self.active_structure.and_then(|i| self.structures.get(i)).map(|s| s.surfaces.iter().map(|x| x.settings.opacity).collect()).unwrap_or_default()
    }

    /// Ticks exactly these orbitals (0-based) of the active structure's
    /// current analysis set — for tests.
    pub fn select_set_orbitals(&mut self, alpha: &[usize], beta: &[usize]) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let Some(set) = structure.active_orbital_set.checked_sub(1).and_then(|i| structure.orbital_sets.get_mut(i)) else { return };
        set.selected_alpha = alpha.iter().copied().collect();
        set.selected_beta = beta.iter().copied().collect();
    }

    /// Whether surface `index` of the active structure is drawn in its
    /// fragment colors (rather than the positive/negative ones) — for tests.
    pub fn surface_colored_by_fragment(&self, index: usize) -> bool {
        self.active_structure
            .and_then(|i| self.structures.get(i))
            .and_then(|s| s.surfaces.get(index))
            .is_some_and(|x| x.settings.by_fragment && x.fragment_tint.is_some())
    }

    /// The active structure's selected atoms (0-based, in click order) —
    /// for tests.
    pub fn selected_atoms(&self) -> Vec<usize> {
        self.active_structure.and_then(|i| self.structures.get(i)).map(|s| s.selected_atoms.clone()).unwrap_or_default()
    }

    /// Selects these atoms (0-based, in this order), as clicks would —
    /// for tests.
    pub fn select_atoms(&mut self, atoms: &[usize]) {
        if let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) {
            structure.selected_atoms = atoms.to_vec();
            structure.selected_bonds.clear();
        }
        self.rebuild_highlights();
    }

    pub(crate) fn clear_selection(&mut self) {
        if let Some(active) = self.active_structure {
            self.structures[active].selected_atoms.clear();
            self.structures[active].selected_bonds.clear();
        }
        self.rebuild_highlights();
    }

    /// Switches the active structure, with the view `view_for` gives it:
    /// unchanged for the same geometry (the same molecule opened from two
    /// files), the same orientation and zoom with "Keep the view", the
    /// view it was left with, or else framed on it (opened files can be
    /// wildly different sizes and positions). The structure left keeps
    /// its view for when it comes back. Surfaces not extracted yet are
    /// extracted on the next frame (see `update_surfaces`).
    pub(crate) fn set_active(&mut self, index: usize) {
        let camera = self.view_for(index);
        if let Some(previous) = self.active_structure.and_then(|p| self.structures.get_mut(p)) {
            previous.view = Some(self.camera);
        }
        self.active_structure = Some(index);
        self.camera = camera;
        self.rebuild_geometry();
        self.rebuild_highlights();
        self.rebuild_measurements();
        self.rebuild_isosurface();
    }

    /// The material to actually render with — identical to the Appearance
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
    /// is what you actually get — `self.material` itself (the Appearance
    /// panel's own live value) is never touched.
    pub(crate) fn ao_render_material(&self) -> Material {
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
    pub(crate) fn ao_render_isosurface_material(&self) -> IsosurfaceMaterial {
        if !self.ao_enabled {
            return self.isosurface_material;
        }
        let m = self.isosurface_material.material;
        IsosurfaceMaterial { material: [(m[0] + 0.15).min(0.75), m[1], m[2] * 0.8, m[3] * 0.8], fresnel: self.isosurface_material.fresnel }
    }

    pub(crate) fn show_splash(&self, ui: &mut egui::Ui) {
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

    /// Closes structure `index`. The next one (or the previous, when it was
    /// the last) becomes active if the closed one was.
    pub(crate) fn close_structure(&mut self, index: usize) {
        if index >= self.structures.len() {
            return;
        }
        self.structures.remove(index);
        self.forget_structure_panels(index);
        match self.active_structure {
            Some(active) if active == index => {
                self.active_structure = None;
                if !self.structures.is_empty() {
                    self.set_active(index.min(self.structures.len() - 1));
                }
            }
            Some(active) if active > index => self.active_structure = Some(active - 1),
            _ => {}
        }
    }

    /// The bar along the bottom: what is open and selected, how to move
    /// around, and auto-spin — always in view, so none of it has to be
    /// looked up in a window.
    fn show_status_bar(&mut self, ui: &mut egui::Ui) {
        let response = egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) {
                    let n = structure.molecule.atomic_numbers.len();
                    ui.label(RichText::new(&structure.label).strong());
                    ui.label(RichText::new(format!("{n} atom{}", if n == 1 { "" } else { "s" })).weak());
                    let selected = structure.selected_atoms.len() + structure.selected_bonds.len();
                    if selected > 0 {
                        ui.separator();
                        let names: Vec<String> =
                            structure.selected_atoms.iter().map(|&i| crate::names::atom_name(&structure.molecule.atomic_numbers, i)).collect();
                        let mut text = format!("Selected: {}", if names.len() > 6 { format!("{} atoms", names.len()) } else { names.join(", ") });
                        if !structure.selected_bonds.is_empty() {
                            text.push_str(&format!(" + {} bond(s)", structure.selected_bonds.len()));
                        }
                        ui.label(text);
                        ui.label(RichText::new("(Esc clears)").weak());
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::DragValue::new(&mut self.auto_spin_speed).range(0.05..=3.0).speed(0.01).max_decimals(2).suffix(" rad/s"))
                        .on_hover_text("Auto-spin speed");
                    ui.toggle_value(&mut self.auto_spin, "Spin (Space)")
                        .on_hover_text("Turn continuously, in the direction of the last arrow key(s) pressed");
                    ui.separator();
                    ui.label(RichText::new("Drag: rotate  \u{b7}  Shift+drag: pan  \u{b7}  Scroll/pinch: zoom  \u{b7}  Click: select").small().weak());
                });
            });
        });
        self.window_column.bottom = response.response.rect.top();
    }

    pub(crate) fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        let mut open_request: Option<Option<FileKind>> = None;
        let mut save_request: Option<bool> = None;
        let response = egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("APOST3Dview").strong());
                ui.separator();
                ui.menu_button("Open", |ui| {
                    if ui.button("All supported files...").on_hover_text("Shortcut: Cmd/Ctrl+O. Files can also be dragged onto the window.").clicked() {
                        open_request = Some(None);
                    }
                    ui.separator();
                    for kind in FileKind::ALL {
                        if ui.button(format!("{}...", kind.label())).clicked() {
                            open_request = Some(Some(kind));
                        }
                    }
                });
                ui.add_enabled_ui(!self.structures.is_empty(), |ui| {
                    ui.menu_button("Save", |ui| {
                        let save = ui.button("Save session").on_hover_text("Shortcut: Cmd/Ctrl+S. Everything open, how it is shown, and the camera, to reopen later");
                        if save.clicked() {
                            save_request = Some(false);
                        }
                        if ui.button("Save session as...").clicked() {
                            save_request = Some(true);
                        }
                    });
                });
                ui.separator();
                // A click closes an open window, and opens a closed one
                // expanded.
                for window in ToolWindow::TOOLBAR {
                    let open = self.window_open(window);
                    if ui.selectable_label(open, window.title()).clicked() {
                        if open {
                            *self.window_flag(window) = false;
                        } else {
                            self.open_window(window);
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(self.show_about, "About").clicked() {
                        self.show_about = !self.show_about;
                    }
                    if self.windows_out_of_column() && ui.button("Arrange windows").on_hover_text("Put the windows you moved back in the column on the right").clicked() {
                        self.arrange_windows();
                    }
                });
            });
        });
        self.window_column.top = response.response.rect.bottom();
        if let Some(kind) = open_request {
            self.open_dialog(kind);
        }
        if let Some(ask_for_name) = save_request {
            self.save_session(ask_for_name);
        }
    }

    /// Draws the messages currently up, stacked over the top of the window,
    /// and drops the expired ones.
    fn show_toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.shown_at.elapsed() < t.duration);
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_max_width((screen.width() - 40.0).min(560.0));
                for toast in &self.toasts {
                    egui::Frame::popup(ui.style()).fill(toast.color).show(ui, |ui| {
                        ui.label(egui::RichText::new(&toast.text).color(Color32::WHITE).strong());
                    });
                    ui.add_space(4.0);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    /// "APOST3Dview - name.a3dview" while a session is open.
    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match self.session_path.as_ref().and_then(|p| p.file_name()) {
            Some(name) => format!("APOST3Dview - {}", name.to_string_lossy()),
            None => "APOST3Dview".to_string(),
        };
        if title != self.shown_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }
    }

    /// Files dragged over the window: a hint while they hover, opened when
    /// dropped.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            self.open_paths(dropped);
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_hint")));
            let rect = ctx.content_rect();
            painter.rect_filled(rect, 0.0, Color32::from_black_alpha(140));
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Drop to open", egui::FontId::proportional(28.0), Color32::WHITE);
        }
    }

    pub(crate) fn show_empty_state(&self, ui: &egui::Ui, rect: egui::Rect) {
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
            "Drag files here, or use Open in the toolbar (.fchk, .molden, .xyz, .cube, .apost.json)",
            egui::FontId::proportional(15.0),
            Color32::from_gray(130),
        );
    }
}
impl eframe::App for App {
    /// Window sizes and states are not kept between launches: see
    /// `with_options`.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        static FIRST_FRAME: std::sync::Once = std::sync::Once::new();
        FIRST_FRAME.call_once(|| log::info!("first frame"));
        // Always the dark theme, whatever the system's setting (egui would
        // follow it): the look every window and color was designed in.
        // Checked every frame, so nothing set later can switch it back.
        if ui.ctx().options(|o| o.theme_preference) != egui::ThemePreference::Dark {
            ui.ctx().set_theme(egui::Theme::Dark);
        }
        if self.splash {
            let dismissed = ui.input(|i| i.pointer.any_click());
            if self.start_time.elapsed() < SPLASH_DURATION && !dismissed {
                self.show_splash(ui);
                return;
            }
            self.splash = false;
        }
        if !self.pending_files.is_empty() {
            let files = std::mem::take(&mut self.pending_files);
            self.open_paths(files);
        }

        let ctx = ui.ctx().clone();
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O))) {
            self.open_dialog(None);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S))) {
            self.save_session(false);
        }
        self.update_title(&ctx);
        self.handle_dropped_files(&ctx);
        self.update_surfaces(&ctx);
        self.update_results_display(&ctx);
        self.update_figure_preview(&ctx);

        self.expand_requested_windows(&ctx);
        self.show_toolbar(ui);
        self.show_status_bar(ui);
        self.plan_window_column(&ctx);
        self.show_molecule_window(&ctx);
        self.show_surfaces_window(&ctx);
        self.show_measure_window(&ctx);
        self.show_apost_window(&ctx);
        self.show_appearance_window(&ctx);
        self.show_export_window(&ctx);
        self.show_figure_preview_window(&ctx);
        self.show_coordinates_window(&ctx);
        self.show_about_window(&ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.show_viewport(ui));
        self.show_toasts(&ctx);
    }
}
