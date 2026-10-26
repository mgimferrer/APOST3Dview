//! Renders a session to a PNG without opening a window:
//! `apost3dview --render figure.a3dview figure.png [--size WIDTHxHEIGHT]`.
//! The same App and offscreen renderer as the Export window, on a wgpu
//! device of its own, so anyone can check that a session gives back its
//! figure (and scripts can remake figures).

use std::path::Path;

use eframe::App as _;

use crate::app::App;
use crate::panels::{FigureSizing, ImageMode};
use crate::StartupOptions;

/// The size of a single image when none is given (a panel figure has its
/// own, in the session).
pub const DEFAULT_SIZE: [u32; 2] = [1920, 1200];

/// Opens `session`, renders what it shows and writes `out`; returns what was
/// written, or why nothing was. A session whose figure is a panel figure
/// (or a single image with a caption) gives that figure; otherwise the
/// view, `size` pixels.
pub fn render_session(session: &Path, out: &Path, size: [u32; 2]) -> Result<String, String> {
    let setup = egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew::without_display_handle());
    let instance = pollster::block_on(setup.new_instance());
    let options = egui_wgpu::RendererOptions {
        msaa_samples: apost3dview_render::MSAA_SAMPLES,
        depth_stencil_format: Some(apost3dview_render::DEPTH_FORMAT),
        ..Default::default()
    };
    let render_state = pollster::block_on(egui_wgpu::RenderState::create(&egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() }, &instance, None, options))
        .map_err(|e| format!("no graphics device: {e}"))?;

    let ctx = egui::Context::default();
    let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
    cc.wgpu_render_state = Some(render_state.clone());
    let mut frame = eframe::Frame::_new_kittest();
    frame.wgpu_render_state = Some(render_state);
    let mut app = App::with_options(&cc, StartupOptions { files: vec![session.to_path_buf()], splash: false });
    // A few frames, as in the window: the session opens, surfaces are
    // extracted, results drawn.
    for _ in 0..8 {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            app.logic(ui, &mut frame);
            app.ui(ui, &mut frame);
        });
        // Nothing is painted: the windows' textures aren't needed.
        output.textures_delta.clear();
    }
    if app.active_structure.is_none() {
        let problems = app.toast_texts().join("; ");
        return Err(format!("{} could not be opened: {problems}", session.display()));
    }
    let settings = &app.render_export.figure.settings;
    let panels = settings.mode == ImageMode::Panels;
    let as_figure = panels || settings.single_caption;
    let dpi = (as_figure && settings.sizing == FigureSizing::Dpi).then_some(settings.dpi);
    let (width, height, pixels, dpi) = if as_figure {
        let figure = app.render_figure()?;
        (figure.width, figure.height, figure.pixels, dpi)
    } else {
        let [w, h] = size;
        (w, h, app.render_image_supersampled(w, h, 2)?, None)
    };
    App::write_png(out, &pixels, width, height, dpi).map_err(|e| format!("could not write {}: {e}", out.display()))?;
    let what = if panels { "panel figure" } else { "image" };
    Ok(format!("{} written: {what}, {width}x{height} pixels", out.display()))
}
