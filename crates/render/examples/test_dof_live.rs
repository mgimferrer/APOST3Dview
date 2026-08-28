//! Headless check for the *live-view* depth-of-field path
//! (`run_live_dof_pass` then the blit `paint` draws) — as opposed to
//! `test_dof.rs`'s export path (which reads the composite back to a PNG
//! directly instead of blitting it into a shared window pass).
//!
//! Ambient occlusion's own live path needed an explicit pixel-offset
//! correction because it samples `@builtin(position)` (window-relative)
//! from inside the same draw call that rasterizes real 3D geometry — see
//! `test_ao_live.rs`. Depth of field's blit instead derives its UV purely
//! from `vertex_index` in `vs_blit`, never touching `frag_coord` (see
//! `dof.wgsl`'s module doc for why that sidesteps the issue entirely). This
//! test is the actual proof of that claim: it renders the same view once
//! flush with the window origin and once offset within a larger window
//! (mimicking a toolbar + side panel), exactly the way `test_ao_live.rs`
//! caught the real live-AO misalignment bug, and checks the two agree
//! after accounting for the offset.

use apost3dview_core::Molecule;
use apost3dview_render::{AoSettings, DofSettings, GlyphAtlas, Material, OrbitCamera, SceneUniforms, ViewportResources};
use std::path::PathBuf;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("TESTS-VISUALIZER/Bi-dianion-OSD.fchk"));
    let molecule = Molecule::from_fchk(&path).expect("failed to parse real fchk geometry");

    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm;

    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);
    resources.load_molecule(&device, &molecule);

    let (center, radius) = molecule.bounding_sphere();
    let mut camera = OrbitCamera::default();
    camera.frame_bounds(center, radius);
    let material = Material::default();
    let dof_settings = DofSettings::default();
    let ao_settings = AoSettings::default();

    let (viewport_w, viewport_h) = (320u32, 260u32);

    let flush = render_live_frame(
        &mut resources, &device, &queue, &camera, &material, &ao_settings, &dof_settings, target_format, viewport_w, viewport_h, [0.0, 0.0],
        (viewport_w, viewport_h), "flush",
    );

    let offset = [40.0, 70.0];
    let window_size = (viewport_w + 80, viewport_h + 70);
    let offset_frame = render_live_frame(
        &mut resources, &device, &queue, &camera, &material, &ao_settings, &dof_settings, target_format, viewport_w, viewport_h, offset,
        window_size, "offset_window",
    );
    let cropped = crop(&offset_frame, window_size.0, offset[0] as u32, offset[1] as u32, viewport_w, viewport_h);

    let mut max_diff = 0i32;
    let mut differing = 0usize;
    for (a, b) in flush.chunks(4).zip(cropped.chunks(4)) {
        for c in 0..4 {
            let d = (a[c] as i32 - b[c] as i32).abs();
            max_diff = max_diff.max(d);
        }
        if a != b {
            differing += 1;
        }
    }
    let total = (viewport_w * viewport_h) as usize;
    println!("offset-vs-flush: {differing}/{total} pixels differ, max channel diff {max_diff}");
    assert!(differing < total / 20, "offset viewport's DoF blit should match the flush case once cropped, not read misaligned data");

    println!("ALL CHECKS PASSED");
}

fn crop(pixels: &[u8], src_width: u32, x: u32, y: u32, width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    for row in 0..height {
        let src_start = (((y + row) * src_width + x) * 4) as usize;
        let dst_start = (row * width * 4) as usize;
        out[dst_start..dst_start + (width * 4) as usize].copy_from_slice(&pixels[src_start..src_start + (width * 4) as usize]);
    }
    out
}

/// Renders one "frame" the way `ViewportCallback::prepare`/`paint` would
/// with depth of field on: `run_live_dof_pass` into the persistent DoF
/// textures, then a single blit draw into a `window_size` target with the
/// viewport positioned at `offset` via `set_viewport`/`set_scissor_rect` —
/// exactly what egui_wgpu does internally for a paint callback whose rect
/// isn't the whole window. Returns the full window's RGBA8 pixels.
#[allow(clippy::too_many_arguments)]
fn render_live_frame(
    resources: &mut ViewportResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    camera: &OrbitCamera,
    material: &Material,
    ao_settings: &AoSettings,
    dof_settings: &DofSettings,
    target_format: wgpu::TextureFormat,
    viewport_w: u32,
    viewport_h: u32,
    offset: [f32; 2],
    window_size: (u32, u32),
    label: &str,
) -> Vec<u8> {
    let aspect = viewport_w as f32 / viewport_h as f32;
    let uniforms = SceneUniforms::new(camera, aspect, material);
    resources.write_uniforms(queue, &uniforms);

    // --- "prepare()": the whole DoF pipeline into its own private, viewport-sized textures ---
    let view_proj = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
    let camera_eye = glam::Vec3::new(uniforms.camera_eye[0], uniforms.camera_eye[1], uniforms.camera_eye[2]);
    let mut dof_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("live_dof_encoder") });
    resources.run_live_dof_pass(
        device,
        &mut dof_encoder,
        viewport_w,
        viewport_h,
        view_proj,
        camera_eye,
        material.background,
        Some(ao_settings),
        apost3dview_render::AO_KERNEL_SIZE as u32,
        dof_settings,
        camera.distance,
    );
    queue.submit(Some(dof_encoder.finish()));

    // --- "paint()": a single blit draw into a window-sized target, viewport positioned at `offset` ---
    let (window_w, window_h) = window_size;
    let size = wgpu::Extent3d { width: window_w, height: window_h, depth_or_array_layers: 1 };
    let msaa_color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("live_test_msaa_color"),
        size,
        mip_level_count: 1,
        sample_count: apost3dview_render::MSAA_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let msaa_view = msaa_color.create_view(&wgpu::TextureViewDescriptor::default());
    let resolve_color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("live_test_resolve_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let resolve_view = resolve_color.create_view(&wgpu::TextureViewDescriptor::default());

    // The real egui-shared pass carries a depth attachment (the
    // direct-draw path depth-tests atoms/bonds/labels against each other
    // in it) — a color-only pass here would miss exactly the kind of
    // pass/pipeline-compatibility bug a real run hit (`dof_blit_pipeline`
    // originally declared `depth_stencil: None`, which wgpu silently
    // accepted at pipeline-creation time but rejected at draw time
    // against a pass that actually has one).
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("live_test_depth"),
        size,
        mip_level_count: 1,
        sample_count: apost3dview_render::MSAA_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: apost3dview_render::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mut paint_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("live_paint_encoder") });
    {
        let mut pass = paint_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("live_test_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                resolve_target: Some(&resolve_view),
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(offset[0], offset[1], viewport_w as f32, viewport_h as f32, 0.0, 1.0);
        pass.set_scissor_rect(offset[0] as u32, offset[1] as u32, viewport_w, viewport_h);
        resources.blit_dof_output(&mut pass, resources.live_dof_bind_group().expect("DoF should have run"));
    }

    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = window_w * bytes_per_pixel;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("live_test_readback"),
        size: (padded_bytes_per_row * window_h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    paint_encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &resolve_color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_bytes_per_row), rows_per_image: Some(window_h) } },
        size,
    );
    queue.submit(Some(paint_encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).expect("poll failed");
    rx.recv().expect("readback channel closed").expect("failed to map readback buffer");

    let padded = slice.get_mapped_range().expect("failed to get mapped range");
    let mut pixels = vec![0u8; (unpadded_bytes_per_row * window_h) as usize];
    for row in 0..window_h as usize {
        let src = row * padded_bytes_per_row as usize;
        let dst = row * unpadded_bytes_per_row as usize;
        pixels[dst..dst + unpadded_bytes_per_row as usize].copy_from_slice(&padded[src..src + unpadded_bytes_per_row as usize]);
    }
    drop(padded);
    readback.unmap();

    for px in pixels.chunks_mut(4) {
        px.swap(0, 2);
    }
    let out_dir = std::env::var("DOF_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    let out_path = format!("{out_dir}/dof_live_{label}.png");
    image::save_buffer(&out_path, &pixels, window_w, window_h, image::ColorType::Rgba8).expect("failed to write PNG");

    let is_white = |px: &[u8]| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255;
    let non_white = pixels.chunks(4).filter(|px| !is_white(px)).count();
    println!("{label}: {window_w}x{window_h} window, viewport {viewport_w}x{viewport_h} at {offset:?}, saved {out_path}, {non_white} non-background pixels");
    assert!(non_white > 100, "{label}: expected the molecule to actually draw something");

    pixels
}
