use apost3dview_core::{extract_isosurface, parse_cube, refine_grid};
use apost3dview_render::{
    push_isosurface_vertices, ExportSettings, GlyphAtlas, IsosurfaceMaterial, IsosurfaceVertex, Material, OrbitCamera, SceneUniforms,
    ViewportResources,
};
use std::path::PathBuf;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let path = PathBuf::from(std::env::args().nth(1).expect("usage: test_real_cube_render <path.cube>"));
    let cube = parse_cube(&path).expect("failed to parse real cube file");
    println!("Parsed real file: {} atoms, grid {:?}", cube.molecule.atomic_numbers.len(), cube.grid.dims);

    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm;

    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);
    resources.load_molecule(&device, &cube.molecule);

    let isovalue = cube.grid.max_abs_value() * 0.25;
    let refined = refine_grid(&cube.grid, 1); // 1x = passthrough, fast path for this check
    let positive = extract_isosurface(&refined, isovalue);
    let negative = extract_isosurface(&refined.negated(), isovalue);
    println!("Extracted: {} positive triangles, {} negative triangles", positive.positions.len() / 3, negative.positions.len() / 3);

    let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
    push_isosurface_vertices(&mut vertices, &positive, [0.24, 0.35, 0.9], 0.55);
    push_isosurface_vertices(&mut vertices, &negative, [0.86, 0.27, 0.24], 0.55);
    resources.update_isosurface(&device, &vertices);
    resources.update_isosurface_material(&queue, &IsosurfaceMaterial::default());

    let (center, radius) = cube.molecule.bounding_sphere();
    let mut camera = OrbitCamera::default();
    camera.frame_bounds(center, radius);
    let material = Material::default();
    let uniforms = SceneUniforms::new(&camera, 1.0, &material);

    let settings = ExportSettings { width: 256, height: 256, supersample: 1, background: Some([1.0, 1.0, 1.0, 1.0]), ambient_occlusion: None, depth_of_field: None, dof_focus_distance: 0.0 };
    let pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &settings)
        .expect("real-data offscreen render should succeed");

    let is_white = |px: &[u8]| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255;
    let non_white = pixels.chunks(4).filter(|px| !is_white(px)).count();
    println!("{non_white} non-background pixels out of {} — molecule + isosurface both visibly rendered", pixels.len() / 4);
    assert!(non_white > 1000, "expected substantial real-molecule + isosurface coverage");
    println!("ALL CHECKS PASSED");
}
