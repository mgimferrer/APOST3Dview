//! Shared by the app's integration tests: a headless `App` (egui_kittest)
//! that can also be rendered to an image, 3D viewport included.
//!
//! kittest's own wgpu renderer draws into a plain color target, while the
//! viewport's pipelines are built for the real window's surface: 4x MSAA
//! and a depth buffer (`apost3dview_render::{MSAA_SAMPLES, DEPTH_FORMAT}`,
//! also set in `main.rs`). `AppRenderer` renders the same way the window
//! does.

#![allow(dead_code)]

use std::iter;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use apost3dview::{App, StartupOptions};
use apost3dview_render::{DEPTH_FORMAT, MSAA_SAMPLES};
use egui::TexturesDelta;
use egui_kittest::wgpu::{create_render_state, default_wgpu_setup};
use egui_kittest::{Harness, TestRenderer};
use egui_wgpu::{RenderState, ScreenDescriptor};
use image::RgbaImage;

pub fn repo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixtures_dir() -> Option<PathBuf> {
    let dir = repo_dir().join("fixtures/apost3d");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("fixtures/apost3d/ not found; run scripts/sync-apost3d-fixtures.sh to enable this test");
        None
    }
}

/// A fresh scratch directory for one test.
pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("apost3dview-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The app with `files` given on the command line and no splash screen,
/// after a few frames. Renderable with `Harness::render`.
pub fn app_with(files: Vec<PathBuf>) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size([1400.0, 900.0])
        .renderer(AppRenderer::new())
        .build_eframe(move |cc| App::with_options(cc, StartupOptions { files, splash: false }));
    harness.run_steps(4);
    harness
}

pub fn toasts(harness: &Harness<'_, App>) -> Vec<String> {
    harness.state().toast_texts()
}

/// Saves a screenshot when `APOST3DVIEW_SCREENSHOT_DIR` is set (to look at
/// the UI while working on it); otherwise only checks that it rendered.
pub fn screenshot(harness: &mut Harness<'_, App>, name: &str) -> RgbaImage {
    let image = harness.render().expect("render");
    if let Some(dir) = std::env::var_os("APOST3DVIEW_SCREENSHOT_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }
    image
}

pub struct AppRenderer {
    render_state: RenderState,
}

impl AppRenderer {
    pub fn new() -> Self {
        let options = egui_wgpu::RendererOptions {
            msaa_samples: MSAA_SAMPLES,
            depth_stencil_format: Some(DEPTH_FORMAT),
            ..egui_wgpu::RendererOptions::PREDICTABLE
        };
        Self { render_state: create_render_state(default_wgpu_setup(), options) }
    }

    fn target(&self, size: [u32; 2], samples: u32, format: wgpu::TextureFormat, copy_src: bool) -> wgpu::Texture {
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if copy_src {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        self.render_state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test render target"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    fn read_back(&self, texture: &wgpu::Texture) -> RgbaImage {
        let (width, height) = (texture.width(), texture.height());
        let unpadded = width as usize * 4;
        let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let device = &self.render_state.device;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test readback"),
            size: (padded * height as usize) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("test readback") });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: None },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let submission = self.render_state.queue.submit(iter::once(encoder.finish()));
        let (sender, receiver) = channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| drop(sender.send(result)));
        device
            .poll(wgpu::PollType::Wait { submission_index: Some(submission), timeout: Some(Duration::from_secs(20)) })
            .expect("poll");
        receiver.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range().expect("mapped range");
        let mut pixels: Vec<u8> = data.chunks_exact(padded).flat_map(|row| row[..unpadded].iter().copied()).collect();
        if matches!(texture.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
            pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|px| px.swap(0, 2));
        }
        RgbaImage::from_raw(width, height, pixels).expect("image")
    }
}

impl TestRenderer for AppRenderer {
    fn setup_eframe(&self, cc: &mut eframe::CreationContext<'_>, frame: &mut eframe::Frame) {
        cc.wgpu_render_state = Some(self.render_state.clone());
        frame.wgpu_render_state = Some(self.render_state.clone());
    }

    fn handle_delta(&mut self, delta: &mut TexturesDelta) {
        let mut renderer = self.render_state.renderer.write();
        for (id, images) in delta.set.drain() {
            for image in images {
                renderer.update_texture(&self.render_state.device, &self.render_state.queue, id, &image);
            }
        }
        for id in delta.free.drain() {
            renderer.free_texture(&id);
        }
    }

    fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput) -> Result<RgbaImage, String> {
        let size = ctx.content_rect().size() * ctx.pixels_per_point();
        let screen = ScreenDescriptor { pixels_per_point: ctx.pixels_per_point(), size_in_pixels: [size.x.round() as u32, size.y.round() as u32] };
        let format = self.render_state.target_format;
        let multisampled = self.target(screen.size_in_pixels, MSAA_SAMPLES, format, false);
        let resolved = self.target(screen.size_in_pixels, 1, format, true);
        let depth = self.target(screen.size_in_pixels, MSAA_SAMPLES, DEPTH_FORMAT, false);

        let device = &self.render_state.device;
        let mut renderer = self.render_state.renderer.write();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("test frame") });
        let tessellated = ctx.tessellate(output.shapes.clone(), ctx.pixels_per_point());
        let user_buffers = renderer.update_buffers(device, &self.render_state.queue, &mut encoder, &tessellated, &screen);
        {
            let multisampled_view = multisampled.create_view(&Default::default());
            let resolved_view = resolved.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("test frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &multisampled_view,
                        resolve_target: Some(&resolved_view),
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &tessellated, &screen);
        }
        self.render_state.queue.submit(user_buffers.into_iter().chain(iter::once(encoder.finish())));
        drop(renderer);
        Ok(self.read_back(&resolved))
    }
}
