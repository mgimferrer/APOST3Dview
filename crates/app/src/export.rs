//! PNG and animated GIF export.

use apost3dview_render::{render_turntable_gif, ExportSettings, SceneUniforms, TurntableSettings, ViewportResources};

use crate::app::App;
use crate::session::SESSION_EXTENSION;

/// Two ways to size a render: `Dpi` (the default) asks for a figure width
/// in inches and a DPI instead of a bare pixel count, matching how
/// journals actually specify figure requirements, then derives pixel
/// dimensions and embeds the physical size in the PNG itself (see
/// `App::write_png`) so image editors and submission systems read the
/// correct size automatically. `Custom` is the escape hatch for exact
/// pixel dimensions where DPI doesn't apply (slides, web, an exact spec
/// from somewhere that isn't inches-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderPreset {
    Dpi,
    Custom,
}

/// 300 DPI is the common journal minimum, but Martí found renders at 1000
/// DPI (2026-08-28, once AO/depth-of-field gave the image real depth to
/// resolve) look noticeably better, so that's the actual default now — the
/// figure-width quick-set buttons below are typical single/double-column
/// widths, still freely editable, and the DPI field itself stays
/// user-adjustable down to 300 or below for anyone who wants a smaller file.
pub(crate) const DEFAULT_PUBLICATION_DPI: u32 = 1000;
pub(crate) const FIGURE_WIDTH_SINGLE_COLUMN_IN: f64 = 3.25;
pub(crate) const FIGURE_WIDTH_DOUBLE_COLUMN_IN: f64 = 6.75;
/// Renders internally at this multiple of the final pixel size and box-
/// downsamples back — smooths edges beyond the live view's real-time MSAA,
/// independent of whatever DPI/size is chosen.
pub(crate) const PUBLICATION_SUPERSAMPLE: u32 = 2;
/// gifski's own recommended default (1-100). 100 is visibly no better on
/// molecule renders but produces noticeably larger files.
pub(crate) const GIF_QUALITY: u8 = 90;

pub(crate) struct RenderExportState {
    pub(crate) preset: RenderPreset,
    pub(crate) custom_width: u32,
    pub(crate) custom_height: u32,
    pub(crate) custom_supersample: u32,
    pub(crate) transparent_background: bool,
    /// Write the session next to every exported image (on by default).
    pub(crate) save_session_with_image: bool,
    pub(crate) dpi: u32,
    pub(crate) figure_width_in: f64,
    /// Animated GIF export: output width in pixels (height follows the
    /// live view's aspect ratio, like the DPI preset), loop duration in
    /// seconds (one full turn), and frame rate.
    pub(crate) gif_width: u32,
    pub(crate) gif_seconds: f32,
    pub(crate) gif_fps: u32,
    /// Panels, and the caption of a single image (`panels.rs`).
    pub(crate) figure: crate::panels::PanelExportState,
}

impl Default for RenderExportState {
    fn default() -> Self {
        Self {
            preset: RenderPreset::Dpi,
            custom_width: 1920,
            custom_height: 1080,
            custom_supersample: 2,
            transparent_background: false,
            save_session_with_image: true,
            dpi: DEFAULT_PUBLICATION_DPI,
            figure_width_in: FIGURE_WIDTH_SINGLE_COLUMN_IN,
            gif_width: 800,
            gif_seconds: 6.0,
            gif_fps: 20,
            figure: Default::default(),
        }
    }
}

impl App {
    /// Turns the current preset (plus the live view's aspect ratio, for
    /// `Dpi`) into concrete pixel dimensions and background.
    pub(crate) fn resolve_export_settings(&self) -> ExportSettings {
        let background = if self.render_export.transparent_background {
            None
        } else {
            let [r, g, b] = self.material.background;
            Some([r, g, b, 1.0])
        };
        match self.render_export.preset {
            RenderPreset::Dpi => self.publication_export_settings(background),
            RenderPreset::Custom => self.export_settings(
                self.render_export.custom_width.max(1),
                self.render_export.custom_height.max(1),
                self.render_export.custom_supersample.max(1),
                background,
            ),
        }
    }

    /// Turns the requested figure width (inches) and DPI into pixel
    /// dimensions. The physical width always maps to the horizontal
    /// dimension specifically — that's what a journal's column-width
    /// requirement actually constrains, regardless of the molecule's own
    /// aspect ratio. Height is then derived from the live view's aspect
    /// ratio, so the export still frames exactly what's on screen.
    pub(crate) fn publication_export_settings(&self, background: Option<[f32; 4]>) -> ExportSettings {
        let width = ((self.render_export.figure_width_in * self.render_export.dpi as f64).round() as u32).max(1);
        let aspect_ratio = if self.last_aspect_ratio > 0.0 { self.last_aspect_ratio } else { 1.0 };
        let height = ((width as f32 / aspect_ratio).round() as u32).max(1);
        self.export_settings(width, height, PUBLICATION_SUPERSAMPLE, background)
    }

    /// Export settings of this size with the live view's look: ambient
    /// occlusion and depth of field as set, focused where the camera is.
    pub(crate) fn export_settings(&self, width: u32, height: u32, supersample: u32, background: Option<[f32; 4]>) -> ExportSettings {
        ExportSettings {
            width,
            height,
            supersample,
            background,
            ambient_occlusion: self.ao_enabled.then_some(self.ao_settings),
            depth_of_field: self.dof_enabled.then_some(self.dof_settings),
            dof_focus_distance: self.camera.eye_distance(),
        }
    }

    /// The DPI to embed in the exported PNG's physical-size metadata
    /// (`pHYs` chunk) — only meaningful for `Dpi`, since `Custom` has no
    /// attached physical size to be consistent with.
    pub(crate) fn resolve_export_dpi(&self) -> Option<u32> {
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
    pub(crate) fn write_png(path: &std::path::Path, pixels: &[u8], width: u32, height: u32, dpi: Option<u32>) -> Result<(), String> {
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

    /// Renders the active structure offscreen at `settings` and saves it
    /// as a PNG, letting the user pick the name/location. Blocks the UI
    /// thread briefly (see `ViewportResources::render_offscreen`) — an
    /// acceptable one-shot cost for an explicit, infrequent action.
    pub(crate) fn export_render_png(&mut self, settings: ExportSettings) {
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

        match self.render_rgba(active, &settings) {
            Ok(pixels) => {
                let dpi = self.resolve_export_dpi();
                match Self::write_png(&path, &pixels, settings.width, settings.height, dpi) {
                    Ok(()) => self.report_saved_image(&path),
                    Err(err) => self.show_warning(format!("Could not write {}: {err}", path.display())),
                }
            }
            Err(err) => self.show_warning(format!("Render failed: {err}")),
        }
    }

    /// Renders structure `active` offscreen at `settings`, as RGBA8 rows.
    /// Blocks until the GPU is done (see `ViewportResources::
    /// render_offscreen`) — fine for an explicit, one-shot action.
    fn render_rgba(&self, active: usize, settings: &ExportSettings) -> Result<Vec<u8>, String> {
        self.render_rgba_with(active, settings, true)
    }

    /// The same, with or without the results legend drawn into it (a
    /// figure of panels can draw it once below them instead).
    pub(crate) fn render_rgba_with(&self, active: usize, settings: &ExportSettings, legend: bool) -> Result<Vec<u8>, String> {
        let export_aspect = settings.width as f32 / settings.height.max(1) as f32;
        let mut uniforms = SceneUniforms::new(&self.camera, export_aspect, &self.ao_render_material());
        let target_format = self.render_state.target_format;
        uniforms.set_srgb_target(target_format.is_srgb());
        let label_instances = self.build_export_label_instances(active, &self.camera, settings.width as f32, settings.height as f32);

        let mut renderer = self.render_state.renderer.write();
        let resources = renderer.callback_resources.get_mut::<ViewportResources>().ok_or("Renderer not ready.")?;
        let mut pixels = resources.render_offscreen(&self.render_state.device, &self.render_state.queue, target_format, &uniforms, &label_instances, settings)?;
        apost3dview_render::readback_to_rgba(&mut pixels, target_format);
        drop(renderer);
        if legend {
            self.draw_legend_into(&mut pixels, settings.width, settings.height);
        }
        Ok(pixels)
    }

    /// The current view, as an export would render it (labels, ambient
    /// occlusion, depth of field), at `width` x `height`, as RGBA8 rows.
    /// Without a file — for tests, and later for rendering from the
    /// command line.
    pub fn render_image(&mut self, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.render_image_supersampled(width, height, 1)
    }

    /// The same, rendered at `supersample` times the size and scaled down.
    pub fn render_image_supersampled(&mut self, width: u32, height: u32, supersample: u32) -> Result<Vec<u8>, String> {
        let active = self.active_structure.ok_or("nothing open")?;
        let [r, g, b] = self.material.background;
        let settings = self.export_settings(width, height, supersample, Some([r, g, b, 1.0]));
        self.render_rgba(active, &settings)
    }

    /// After an image is written: also writes the session next to it
    /// (same name, `.a3dview`) when "Also save the session" is ticked, so
    /// every figure keeps the exact setup that produced it.
    pub(crate) fn report_saved_image(&mut self, image: &std::path::Path) {
        let image_name = crate::util::file_display_name(image);
        if !self.render_export.save_session_with_image {
            self.show_status(format!("Saved {image_name}"));
            return;
        }
        let session = image.with_extension(SESSION_EXTENSION);
        let session_name = crate::util::file_display_name(&session);
        match self.save_session_to(&session) {
            Ok(()) => self.show_status(format!("Saved {image_name} and its session {session_name}")),
            Err(err) => self.show_warning(format!("Saved {image_name}, but not its session: {err}")),
        }
    }

    /// Size/quality for animated GIF export: always an opaque background
    /// (GIF only has 1-bit transparency, which would leave hard, haloed
    /// edges around antialiased atoms), height from the live view's
    /// aspect ratio so the animation frames exactly what's on screen.
    pub(crate) fn gif_export_settings(&self) -> ExportSettings {
        let width = self.render_export.gif_width.max(16);
        let aspect_ratio = if self.last_aspect_ratio > 0.0 { self.last_aspect_ratio } else { 1.0 };
        let height = ((width as f32 / aspect_ratio).round() as u32).max(16);
        let [r, g, b] = self.material.background;
        self.export_settings(width, height, PUBLICATION_SUPERSAMPLE, Some([r, g, b, 1.0]))
    }

    pub(crate) fn gif_frame_count(&self) -> usize {
        ((self.render_export.gif_seconds * self.render_export.gif_fps as f32).round() as usize).max(2)
    }

    /// Renders one full turn of the active structure, starting from the
    /// current view and turning in the auto-spin direction (the last arrow
    /// key(s) pressed), and saves it as a looping animated GIF (see
    /// `apost3dview_render::render_turntable_gif`). Blocks the UI until
    /// done, like PNG export.
    pub(crate) fn export_render_gif(&mut self) {
        let Some(active) = self.active_structure else {
            self.show_warning("Open a structure first.");
            return;
        };
        let default_name = self
            .structures[active]
            .source_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| format!("{}.gif", s.to_string_lossy()))
            .unwrap_or_else(|| "animation.gif".to_string());
        let Some(path) = rfd::FileDialog::new().add_filter("GIF animation", &["gif"]).set_file_name(default_name).save_file() else {
            return;
        };
        let file = match std::fs::File::create(&path) {
            Ok(file) => std::io::BufWriter::new(file),
            Err(err) => {
                self.show_warning(format!("Could not create {}: {err}", path.display()));
                return;
            }
        };

        let settings = self.gif_export_settings();
        let turntable = TurntableSettings {
            frames: self.gif_frame_count(),
            fps: self.render_export.gif_fps as f64,
            direction: self.spin_direction,
            quality: GIF_QUALITY,
        };
        let material = self.ao_render_material();
        let result = {
            let mut renderer = self.render_state.renderer.write();
            match renderer.callback_resources.get_mut::<ViewportResources>() {
                None => Err("Renderer not ready.".to_string()),
                Some(resources) => render_turntable_gif(
                    resources,
                    &self.render_state.device,
                    &self.render_state.queue,
                    self.render_state.target_format,
                    &self.camera,
                    &material,
                    &settings,
                    turntable,
                    |camera| self.build_export_label_instances(active, camera, settings.width as f32, settings.height as f32),
                    |pixels, width, height| self.draw_legend_into(pixels, width, height),
                    file,
                ),
            }
        };
        match result {
            Ok(()) => self.report_saved_image(&path),
            Err(message) => {
                let _ = std::fs::remove_file(&path);
                self.show_warning(message);
            }
        }
    }
}
