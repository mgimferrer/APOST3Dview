//! The application state and frame loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use apost3dview_core::{CoordinateFormat, LengthUnit};
use apost3dview_render::{AoSettings, DofSettings, GlyphAtlas, IsosurfaceMaterial, Material, OrbitCamera, ViewportResources};
use egui::Color32;
use glam::Vec2;

use crate::export::RenderExportState;
use crate::isosurfaces::KeptIsosurface;
use crate::orbitals::OrbitalGenerationState;
use crate::presets::list_style_presets;
use crate::state::{AtomLabelMode, AtomLabelStyle, LoadedStructure, MeasurementStyle};
use crate::util::molecules_share_geometry;
use crate::viewport::DEFAULT_AUTO_SPIN_SPEED;

/// Minimum time the splash screen stays up, regardless of how fast startup
/// actually finishes.
pub(crate) const SPLASH_MIN_DURATION: Duration = Duration::from_secs(4);

pub(crate) const WARNING_DURATION: Duration = Duration::from_secs(3);

pub(crate) fn load_texture(ctx: &egui::Context, name: &str, png_bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(png_bytes).expect("bundled image should be valid").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

pub struct App {
    pub(crate) camera: OrbitCamera,
    /// Continuous rotation, toggled with the Space bar or the
    /// Visualization window. Pauses while the user is rotating by hand.
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
    pub(crate) render_state: egui_wgpu::RenderState,

    // Each tool panel is an independent floating window, toggled from the
    // top toolbar — this is the scalable structure: adding a new panel
    // later is one more bool + one more `show_*_window` function, no
    // restructuring of the others.
    pub(crate) show_style: bool,
    pub(crate) show_xyz: bool,
    pub(crate) show_visualization: bool,
    pub(crate) show_analysis: bool,
    pub(crate) show_structures: bool,
    pub(crate) show_about: bool,
    pub(crate) show_render: bool,

    pub(crate) coordinate_unit: LengthUnit,
    pub(crate) coordinate_format: CoordinateFormat,
    pub(crate) measurement_style: MeasurementStyle,
    pub(crate) atom_label_mode: AtomLabelMode,
    pub(crate) atom_label_style: AtomLabelStyle,
    pub(crate) render_export: RenderExportState,
    pub(crate) orbital_generation: OrbitalGenerationState,
    /// Updated every viewport repaint from the actual on-screen rect —
    /// the Render window reads this (rather than recomputing it itself,
    /// since it doesn't have direct access to the viewport rect) so the
    /// DPI preset can derive its height from the current on-screen view.
    pub(crate) last_aspect_ratio: f32,

    /// Isosurface lighting response — deliberately separate from
    /// `material` (the atom/bond one), shared across every structure the
    /// same way `material` is.
    pub(crate) isosurface_material: IsosurfaceMaterial,
    /// Ambient occlusion — a single shared toggle/settings pair driving
    /// both the live view and export (see `ao_render_material` and
    /// `ViewportResources::run_ao_passes`), so tuning the Style-window
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
    /// Snapshots taken by the Isosurfaces "Keep surface" button — frozen
    /// geometry *and* color/opacity as they were at that moment, rendered
    /// every frame regardless of which structure is currently active, so
    /// several `.cube` files sharing one geometry (the common case:
    /// several orbitals of one molecule) can be composed into one image.
    /// "Clean" empties this back out.
    pub(crate) kept_isosurfaces: Vec<KeptIsosurface>,

    /// Names of every saved Custom style preset on disk (see
    /// `presets_dir`), cached so the Style window doesn't re-read the
    /// directory every frame — refreshed after a save or delete.
    pub(crate) style_presets: Vec<String>,
    /// Text field buffer for naming a new Custom preset before saving.
    pub(crate) new_preset_name: String,

    pub(crate) warning: Option<(String, Color32, Instant)>,
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
            auto_spin: false,
            auto_spin_speed: DEFAULT_AUTO_SPIN_SPEED,
            spin_direction: Vec2::X,
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
            style_presets: list_style_presets(),
            new_preset_name: String::new(),
            warning: None,
        }
    }

    pub(crate) fn rebuild_geometry(&self) {
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

    pub(crate) fn rebuild_highlights(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_highlights(&self.render_state.device, &structure.molecule, &structure.selected_atoms, &structure.selected_bonds);
        }
    }

    pub(crate) fn rebuild_measurements(&self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get(active) else { return };
        let segments: Vec<(usize, usize)> = structure.measurements.iter().flat_map(|m| m.kind.segments()).collect();
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_measurements(&self.render_state.device, &structure.molecule, &segments, self.measurement_style.line_color);
        }
    }

    pub(crate) fn show_warning(&mut self, message: impl Into<String>) {
        self.warning = Some((message.into(), Color32::from_rgb(196, 60, 40), Instant::now()));
    }

    pub(crate) fn show_status(&mut self, message: impl Into<String>) {
        self.warning = Some((message.into(), Color32::from_rgb(45, 130, 80), Instant::now()));
    }

    pub(crate) fn clear_selection(&mut self) {
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
    pub(crate) fn set_active(&mut self, index: usize) {
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

    pub(crate) fn show_toolbar(&mut self, ui: &mut egui::Ui) {
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

    pub(crate) fn show_warning_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
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
            .show(ui, |ui| self.show_viewport(ui));
    }
}
