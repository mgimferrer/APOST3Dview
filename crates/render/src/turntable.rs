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
    mut finish_frame: impl FnMut(&mut [u8], u32, u32),
    out: W,
) -> Result<(), String> {
    let frames = turntable.frames.max(2);
    let fps = if turntable.fps > 0.0 { turntable.fps } else { 20.0 };
    let direction = if turntable.direction == Vec2::ZERO { Vec2::X } else { turntable.direction };
    let (width, height) = (settings.width as usize, settings.height as usize);
    let aspect = settings.width as f32 / settings.height.max(1) as f32;

    let mut gif = GifWriter::new(turntable.quality, out)?;
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
        crate::export::readback_to_rgba(&mut pixels, target_format);
        // Anything drawn onto the finished image (a legend).
        finish_frame(&mut pixels, settings.width, settings.height);
        if let Err(err) = gif.add_frame(pixels, width, height, index as f64 / fps) {
            failure = Some(err);
            break;
        }
    }
    let encoded = gif.finish();
    failure.map_or(encoded, Err)
}

/// A looping GIF written with gifski as frames come in: gifski encodes on
/// its own thread, so frames stream in rather than all being held in
/// memory first. For animations whose frames the caller renders itself
/// (a reaction path, where the atoms move between frames).
pub struct GifWriter {
    collector: Option<gifski::Collector>,
    encoder: Option<std::thread::JoinHandle<gifski::GifResult<()>>>,
    next_index: usize,
}

impl GifWriter {
    /// Starts the encoder writing to `out`; `quality` is gifski's, 1-100.
    pub fn new<W: Write + Send + 'static>(quality: u8, out: W) -> Result<Self, String> {
        let (collector, writer) = gifski::new(gifski::Settings {
            width: None,
            height: None,
            quality: quality.clamp(1, 100),
            fast: false,
            repeat: gifski::Repeat::Infinite,
        })
        .map_err(|err| format!("Could not start GIF encoder: {err}"))?;
        let encoder = std::thread::spawn(move || writer.write(out, &mut gifski::progress::NoProgress {}));
        Ok(Self { collector: Some(collector), encoder: Some(encoder), next_index: 0 })
    }

    /// Adds the next frame (`width` x `height` RGBA pixels), shown from
    /// `seconds` after the start until the next frame's time (a longer gap
    /// holds this frame longer).
    pub fn add_frame(&mut self, rgba: Vec<u8>, width: usize, height: usize, seconds: f64) -> Result<(), String> {
        let collector = self.collector.as_ref().ok_or("GIF encoder already finished")?;
        let frame = imgref::ImgVec::new(rgb::FromSlice::as_rgba(rgba.as_slice()).to_vec(), width, height);
        collector.add_frame_rgba(self.next_index, frame, seconds).map_err(|err| format!("GIF encoding failed: {err}"))?;
        self.next_index += 1;
        Ok(())
    }

    /// Tells the encoder no more frames are coming and waits for the file.
    pub fn finish(mut self) -> Result<(), String> {
        self.close()
    }

    fn close(&mut self) -> Result<(), String> {
        // Dropping the collector tells the encoder no more frames are coming.
        drop(self.collector.take());
        match self.encoder.take().map(|e| e.join()) {
            None | Some(Ok(Ok(()))) => Ok(()),
            Some(Ok(Err(err))) => Err(format!("GIF encoding failed: {err}")),
            Some(Err(_)) => Err("GIF encoder stopped unexpectedly".to_string()),
        }
    }
}

impl Drop for GifWriter {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
