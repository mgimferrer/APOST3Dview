//! Animated "turntable" export: one full turn of the scene, rendered frame
//! by frame through the same offscreen renderer as PNG export, encoded as a
//! looping GIF with gifski.
//!
//! gifski picks a tuned palette per frame (with dithering) rather than one
//! fixed palette, which is what lets smooth sphere and isosurface shading
//! survive GIF's 256-color-per-frame limit; a basic per-frame quantizer
//! visibly bands and drops small details on the same frames. It's pure
//! Rust, so it adds no C toolchain or external tool to the build.

use std::io::Write;

use glam::Vec2;

use crate::camera::OrbitCamera;
use crate::export::ExportSettings;
use crate::label::GlyphInstance;
use crate::material::Material;
use crate::uniforms::SceneUniforms;
use crate::viewport::ViewportResources;

/// What to animate: `frames` frames covering exactly one turn, played at
/// `fps`, turning along the screen-space `direction` (x = horizontal, y =
/// vertical, need not be normalized; zero falls back to horizontal).
#[derive(Debug, Clone, Copy)]
pub struct TurntableSettings {
    pub frames: usize,
    pub fps: f64,
    pub direction: Vec2,
    /// gifski quality, 1-100.
    pub quality: u8,
}

/// Renders the turntable and writes the GIF to `out`. Frame `i` is the
/// start view rotated by `i / frames` of a turn, so the frame after the
/// last would be the first again: the loop is seamless. `labels_for`
/// builds each frame's label geometry for that frame's camera (atom labels
/// face the camera, so they move with it). gifski encodes on its own
/// thread while frames are rendered here, so frames stream in rather than
/// all being held in memory first.
#[allow(clippy::too_many_arguments)]
pub fn render_turntable_gif<W: Write + Send + 'static>(
    resources: &mut ViewportResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target_format: wgpu::TextureFormat,
    start_camera: &OrbitCamera,
    material: &Material,
    settings: &ExportSettings,
    turntable: TurntableSettings,
    mut labels_for: impl FnMut(&OrbitCamera) -> Vec<GlyphInstance>,
    out: W,
) -> Result<(), String> {
    let frames = turntable.frames.max(2);
    let fps = if turntable.fps > 0.0 { turntable.fps } else { 20.0 };
    let direction = if turntable.direction == Vec2::ZERO { Vec2::X } else { turntable.direction };
    let (width, height) = (settings.width as usize, settings.height as usize);
    let aspect = settings.width as f32 / settings.height.max(1) as f32;
    let bgra = matches!(target_format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);

    let (collector, writer) = gifski::new(gifski::Settings {
        width: None,
        height: None,
        quality: turntable.quality.clamp(1, 100),
        fast: false,
        repeat: gifski::Repeat::Infinite,
    })
    .map_err(|err| format!("Could not start GIF encoder: {err}"))?;
    let encoder = std::thread::spawn(move || writer.write(out, &mut gifski::progress::NoProgress {}));

    let step = std::f32::consts::TAU / frames as f32;
    let mut failure = None;
    for index in 0..frames {
        let mut camera = *start_camera;
        camera.orbit_along(direction, step * index as f32);
        let mut uniforms = SceneUniforms::new(&camera, aspect, material);
        uniforms.set_srgb_target(target_format.is_srgb());
        let labels = labels_for(&camera);
        let mut pixels = match resources.render_offscreen(device, queue, target_format, &uniforms, &labels, settings) {
            Ok(pixels) => pixels,
            Err(err) => {
                failure = Some(format!("Render failed: {err}"));
                break;
            }
        };
        // The offscreen readback is in the target format's channel order;
        // gifski wants RGBA.
        if bgra {
            for px in pixels.chunks_mut(4) {
                px.swap(0, 2);
            }
        }
        let frame = imgref::ImgVec::new(rgb::FromSlice::as_rgba(pixels.as_slice()).to_vec(), width, height);
        if let Err(err) = collector.add_frame_rgba(index, frame, index as f64 / fps) {
            failure = Some(format!("GIF encoding failed: {err}"));
            break;
        }
    }
    // Dropping the collector tells the encoder no more frames are coming.
    drop(collector);
    let encoded = match encoder.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(format!("GIF encoding failed: {err}")),
        Err(_) => Err("GIF encoder stopped unexpectedly".to_string()),
    };
    failure.map_or(encoded, Err)
}
