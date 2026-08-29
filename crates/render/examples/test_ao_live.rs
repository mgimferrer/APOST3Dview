//! Headless check for the *live-view* ambient-occlusion path — persistent,
//! resize-aware G-buffer/AO textures (`ensure_ao_textures`) and the
//! `prepare`/`paint` split `ViewportCallback` uses (`run_live_ao_pass` then
//! `draw_into_pass`), as opposed to `test_ao.rs`'s export path (fresh
//! textures every call).
//!
//! Specifically targets the bug an actual hands-on session caught that no
//! earlier version of this test did: the live view's `paint` draws into a
//! render target shared with the rest of the window (toolbar, side
//! panels), not one sized to just the 3D viewport — so `@builtin(position)`
//! in `fs_main_ao` is in *full-window* coordinates, while the AO/depth
//! textures it samples are sized to the viewport alone. Without correcting
//! for the viewport's own on-screen offset, AO sampling reads whatever
//! happens to be at that pixel offset in a texture that's a different
//! shape entirely — exactly the badly-misaligned artifacts reported live.
//! This test renders the same view twice, once as if the viewport were
//! flush with the window origin and once offset within a larger window
//! (mimicking a real toolbar + side panel layout), and checks the two
//! agree after accounting for the offset.

use apost3dview_core::parse_xyz;
use apost3dview_render::{AoSettings, GlyphAtlas, Material, OrbitCamera, SceneUniforms, ViewportResources};
use std::path::PathBuf;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../TESTS-VISUALIZER/BiCl3-def2QZVPP.xyz"));
    let molecule = parse_xyz(&path).expect("failed to parse real xyz file");

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
    let material = Material { ambient: 0.55, reflectance: 0.03, light_intensity: 2.0, ..Material::default() };
    let ao_settings = AoSettings::default();

    let (viewport_w, viewport_h) = (320u32, 260u32);

    // "Flush with the window" — the case every earlier version of this
    // test covered, and the only case that happened to work by accident.
    let flush = render_live_frame(&mut resources, &device, &queue, &camera, &material, &ao_settings, target_format, viewport_w, viewport_h, [0.0, 0.0], (viewport_w, viewport_h), apost3dview_render::AO_LIVE_SAMPLE_COUNT, "flush");

    // A resize, still flush — the thing export never has to handle.
    render_live_frame(&mut resources, &device, &queue, &camera, &material, &ao_settings, target_format, 384, 300, [0.0, 0.0], (384, 300), apost3dview_render::AO_LIVE_SAMPLE_COUNT, "resized");

    // "Phase C" settle tier — the same one-time full-quality recompute
    // `App` triggers once the camera stops moving. Just a smoke test that
    // it runs and looks reasonable at the higher sample count; the actual
    // settle *timing* logic lives in `App`, not this crate, so there's
    // nothing else here to headlessly exercise.
    render_live_frame(&mut resources, &device, &queue, &camera, &material, &ao_settings, target_format, viewport_w, viewport_h, [0.0, 0.0], (viewport_w, viewport_h), apost3dview_render::AO_KERNEL_SIZE as u32, "settled");

    // The actual real-world case: viewport offset within a larger window
    // (a toolbar above it, a side panel to the right — a 40x70 px margin
    // here). Crop the matching sub-rect back out and compare to `flush`.
    let offset = [40.0, 70.0];
    let window_size = (viewport_w + 80, viewport_h + 70);
    let offset_frame = render_live_frame(&mut resources, &device, &queue, &camera, &material, &ao_settings, target_format, viewport_w, viewport_h, offset, window_size, apost3dview_render::AO_LIVE_SAMPLE_COUNT, "offset_window");
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
    // Some tolerance for MSAA/sub-pixel jitter between the two renders
    // (the two use different-sized MSAA targets, so resolve isn't
    // bit-identical) — but nothing close to the gross misalignment the
    // actual bug produced, which differed almost everywhere.
    assert!(differing < total / 20, "offset viewport's AO should match the flush case once cropped, not read misaligned data");

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

/// Renders one "frame" the way `ViewportCallback::prepare`/`paint` would:
/// AO pre-passes into the (viewport-sized) persistent textures, then the
/// main draw into a `window_size` target with the viewport positioned at
/// `offset` within it via `set_viewport`/`set_scissor_rect` — exactly what
/// egui_wgpu does internally for a paint callback whose rect isn't the
/// whole window. Returns the full window's RGBA8 pixels.
#[allow(clippy::too_many_arguments)]
fn render_live_frame(
    resources: &mut ViewportResources,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    camera: &OrbitCamera,
    material: &Material,
    ao_settings: &AoSettings,
    target_format: wgpu::TextureFormat,
    viewport_w: u32,
    viewport_h: u32,
    offset: [f32; 2],
    window_size: (u32, u32),
    sample_count: u32,
    label: &str,
) -> Vec<u8> {
    let aspect = viewport_w as f32 / viewport_h as f32;
    let uniforms = SceneUniforms::new(camera, aspect, material);
    resources.write_uniforms(queue, &uniforms);

    // --- "prepare()": AO pre-passes into the viewport-sized persistent textures ---
    let view_proj = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
    let camera_eye = glam::Vec3::new(uniforms.camera_eye[0], uniforms.camera_eye[1], uniforms.camera_eye[2]);
    let mut ao_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("live_ao_encoder") });
    resources.run_live_ao_pass(device, &mut ao_encoder, viewport_w, viewport_h, view_proj, camera_eye, ao_settings, sample_count, offset);
    queue.submit(Some(ao_encoder.finish()));

    // --- "paint()": draw into a window-sized target, viewport positioned at `offset` ---
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
        // Exactly what egui_wgpu does internally for a paint callback
        // whose rect is a sub-region of the window it shares with every
        // other panel — this is what makes `@builtin(position)` in
        // `fs_main_ao` come out in full-window coordinates.
        pass.set_viewport(offset[0], offset[1], viewport_w as f32, viewport_h as f32, 0.0, 1.0);
        pass.set_scissor_rect(offset[0] as u32, offset[1] as u32, viewport_w, viewport_h);
        resources.draw_into_pass(&mut pass, resources.live_ao_bind_group());
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

    // BGRA -> RGBA for the PNG encoder.
    for px in pixels.chunks_mut(4) {
        px.swap(0, 2);
    }
    let out_dir = std::env::var("AO_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    let out_path = format!("{out_dir}/ao_live_{label}.png");
    image::save_buffer(&out_path, &pixels, window_w, window_h, image::ColorType::Rgba8).expect("failed to write PNG");

    let is_white = |px: &[u8]| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255;
    let non_white = pixels.chunks(4).filter(|px| !is_white(px)).count();
    println!("{label}: {window_w}x{window_h} window, viewport {viewport_w}x{viewport_h} at {offset:?}, saved {out_path}, {non_white} non-background pixels");
    assert!(non_white > 100, "{label}: expected the molecule to actually draw something");

    pixels
}
