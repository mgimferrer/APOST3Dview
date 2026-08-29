//! Exercises the isosurface rendering pipeline (WGSL shader validation,
//! vertex layout, translucent blending) against a headless wgpu device —
//! `cargo check` doesn't validate WGSL, so this is the only thing that
//! actually proves the shader compiles and draws something.

use apost3dview_core::{extract_isosurface, ScalarGrid};
use apost3dview_render::{
    push_isosurface_vertices, AoSettings, ExportSettings, GlyphAtlas, IsosurfaceMaterial, IsosurfaceVertex, Material, OrbitCamera,
    SceneUniforms, ViewportResources,
};
use glam::Vec3;

fn main() {
    pollster::block_on(run());
}

fn sphere_grid(n: usize, extent: f32) -> ScalarGrid {
    let step = 2.0 * extent / (n - 1) as f32;
    let origin = Vec3::splat(-extent);
    let mut values = vec![0.0f32; n * n * n];
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let p = origin + Vec3::new(i as f32, j as f32, k as f32) * step;
                values[(i * n + j) * n + k] = -p.length();
            }
        }
    }
    ScalarGrid {
        origin,
        dims: [n, n, n],
        steps: [Vec3::new(step, 0.0, 0.0), Vec3::new(0.0, step, 0.0), Vec3::new(0.0, 0.0, step)],
        values,
    }
}

async fn run() {
    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no GPU adapter available");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("failed to create device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm; // matches this app's actual runtime swapchain format

    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);

    // Two spheres of different radii from the same grid, standing in for
    // "two differently-colored isosurfaces drawn together" (positive lobe
    // + negative lobe, or a kept surface + a live one) — the render
    // pipeline doesn't care where the meshes came from, only that
    // multiple colored/opacity'd pushes into one vertex buffer work.
    let grid = sphere_grid(21, 3.0);
    let outer = extract_isosurface(&grid, -1.8);
    let inner = extract_isosurface(&grid, -0.9);
    assert!(!outer.is_empty() && !inner.is_empty(), "test grid should produce non-empty meshes");

    let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
    push_isosurface_vertices(&mut vertices, &outer, [0.2, 0.4, 0.9], 0.6);
    push_isosurface_vertices(&mut vertices, &inner, [0.9, 0.3, 0.2], 0.6);
    println!("Built {} isosurface vertices ({} triangles)", vertices.len(), vertices.len() / 3);

    resources.update_isosurface(&device, &vertices);
    resources.update_isosurface_material(&queue, &IsosurfaceMaterial::default());

    let camera = OrbitCamera::default();
    let material = Material::default();
    let uniforms = SceneUniforms::new(&camera, 1.0, &material);
    let settings = ExportSettings { width: 128, height: 128, supersample: 1, background: Some([1.0, 1.0, 1.0, 1.0]), ambient_occlusion: None, depth_of_field: None, dof_focus_distance: 0.0 };
    let pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &settings)
        .expect("offscreen render with isosurface should succeed");

    let is_white = |px: &[u8]| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255;
    let non_white_count = pixels.chunks(4).filter(|px| !is_white(px)).count();
    println!("{non_white_count} non-background pixels out of {}", pixels.len() / 4);
    assert!(non_white_count > 100, "expected the translucent isosurface to visibly cover a meaningful area, got {non_white_count} pixels");

    // Isosurface + AO together: exercises `isosurface_pipeline_ao` and
    // `isosurface_gbuffer_pipeline` (see `viewport.rs`'s `draw_gbuffer_pass`/
    // `draw_into_pass`) — the whole point being to catch exactly the kind
    // of bind-group/pipeline-layout mismatch that only shows up at draw
    // time, not at `cargo build`.
    let ao_settings = ExportSettings { ambient_occlusion: Some(AoSettings::default()), ..settings };
    let ao_pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &ao_settings)
        .expect("offscreen render with isosurface + AO should succeed");
    let ao_non_white_count = ao_pixels.chunks(4).filter(|px| !is_white(px)).count();
    println!("{ao_non_white_count} non-background pixels out of {} (AO on)", ao_pixels.len() / 4);
    assert!(ao_non_white_count > 100, "expected the isosurface to still visibly cover a meaningful area with AO on");

    println!("ALL CHECKS PASSED");
}
