//! Ad hoc visual check (not a kept regression test) for the isosurface
//! material/AO/tone-mapping overhaul against a real molecule + a real
//! generated orbital — the scenario the "can you still see the atom
//! behind the isosurface" complaint was actually about, unlike
//! `test_isosurface.rs`'s synthetic double-sphere scene.

use apost3dview_core::{extract_isosurface, generate_mo_grids, Molecule};
use apost3dview_render::{
    push_isosurface_vertices, AoSettings, ExportSettings, GlyphAtlas, IsosurfaceMaterial, IsosurfaceVertex, Material, OrbitCamera,
    SceneUniforms, ViewportResources,
};
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
    let wfn = apost3dview_core::parse_fchk_wavefunction(&path).expect("failed to parse wavefunction");
    let homo = wfn.alpha.homo_index() - 1;

    // A lower isovalue than the app's own default (0.075) on purpose — a
    // bigger, more enveloping lobe, closer to the real screenshot
    // (2026-08-29) that showed the metal atom almost entirely hidden by a
    // large orbital, rather than the smaller metal-centered HOMO lobe the
    // first version of this check happened to land on.
    let isovalue = std::env::args().nth(2).map(|s| s.parse().expect("isovalue must be a number")).unwrap_or(0.035);
    println!("Generating HOMO grid at isovalue {isovalue}...");
    let grids = generate_mo_grids(&wfn.basis, &[(&wfn.alpha, homo)], 0.28, 4.0).expect("grid generation failed");
    let grid = &grids[0];
    let mesh_pos = extract_isosurface(grid, isovalue);
    let mesh_neg = extract_isosurface(grid, -isovalue);
    println!("positive lobe: {} verts, negative lobe: {} verts", mesh_pos.positions.len(), mesh_neg.positions.len());

    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm;

    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);
    resources.load_molecule(&device, &molecule);

    let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
    push_isosurface_vertices(&mut vertices, &mesh_pos, [60.0 / 255.0, 90.0 / 255.0, 230.0 / 255.0], 1.0);
    push_isosurface_vertices(&mut vertices, &mesh_neg, [220.0 / 255.0, 70.0 / 255.0, 60.0 / 255.0], 1.0);
    resources.update_isosurface(&device, &vertices);
    resources.update_isosurface_material(&queue, &IsosurfaceMaterial::default());

    let (center, radius) = molecule.bounding_sphere();
    let mut camera = OrbitCamera::default();
    camera.frame_bounds(center, radius);
    let material = Material::default();
    let uniforms = SceneUniforms::new(&camera, 1.0, &material);

    let out_dir = std::env::var("ISO_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    let save = |pixels: &[u8], width: u32, height: u32, name: &str| {
        let mut rgba = pixels.to_vec();
        for px in rgba.chunks_mut(4) {
            px.swap(0, 2);
        }
        let out_path = format!("{out_dir}/{name}.png");
        image::save_buffer(&out_path, &rgba, width, height, image::ColorType::Rgba8).expect("failed to save PNG");
        println!("Saved {out_path}");
    };

    // Export-quality render: full AO sample count (`AO_KERNEL_SIZE`,
    // export always uses this — see `render_offscreen`) and 2x
    // supersample, which on its own already smooths away a lot of
    // per-pixel AO grain a 1x live view never gets. This is the
    // "settled" quality tier `App` also uses once the camera stops moving.
    let export_settings = ExportSettings {
        width: 640,
        height: 640,
        supersample: 2,
        background: Some([1.0, 1.0, 1.0, 1.0]),
        ambient_occlusion: Some(AoSettings::default()),
        depth_of_field: None,
        dof_focus_distance: 0.0,
    };
    let export_pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &export_settings)
        .expect("offscreen render should succeed");
    save(&export_pixels, export_settings.width, export_settings.height, "isosurface_quality_export");

    // Live-preview-quality render: `AO_LIVE_SAMPLE_COUNT` (32, the cheap
    // tier used every frame the camera is actually moving) and no
    // supersampling — what a screenshot taken mid-orbit, or right as the
    // camera stops but before the one-time settle recompute lands, would
    // actually show. Mirrors `ViewportCallback::prepare`/`paint` manually
    // (same shape as `test_ao_live.rs`'s `render_live_frame`), since
    // `render_offscreen` is the export-only path and always uses full AO
    // quality when AO is on at all.
    let (width, height) = (640u32, 640u32);
    let view_proj = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
    let camera_eye = glam::Vec3::new(uniforms.camera_eye[0], uniforms.camera_eye[1], uniforms.camera_eye[2]);
    let mut ao_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("live_ao_encoder") });
    resources.run_live_ao_pass(
        &device,
        &mut ao_encoder,
        width,
        height,
        view_proj,
        camera_eye,
        &AoSettings::default(),
        apost3dview_render::AO_LIVE_SAMPLE_COUNT,
        [0.0, 0.0],
    );
    queue.submit(Some(ao_encoder.finish()));

    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let make = |usage| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("live_test"),
                size,
                mip_level_count: 1,
                sample_count: apost3dview_render::MSAA_SAMPLES,
                dimension: wgpu::TextureDimension::D2,
                format: target_format,
                usage,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    let msaa_view = make(wgpu::TextureUsages::RENDER_ATTACHMENT);
    let resolve_color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("live_test_resolve"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let resolve_view = resolve_color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("live_test_depth"),
            size,
            mip_level_count: 1,
            sample_count: apost3dview_render::MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: apost3dview_render::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default());

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
        resources.draw_into_pass(&mut pass, resources.live_ao_bind_group());
    }
    let bytes_per_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("live_test_readback"),
        size: (bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    paint_encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &resolve_color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(height) } },
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
    let mut live_pixels = vec![0u8; (width * 4 * height) as usize];
    for row in 0..height as usize {
        let src = row * bytes_per_row as usize;
        let dst = row * (width * 4) as usize;
        live_pixels[dst..dst + (width * 4) as usize].copy_from_slice(&padded[src..src + (width * 4) as usize]);
    }
    drop(padded);
    readback.unmap();
    save(&live_pixels, width, height, "isosurface_quality_live");
}
