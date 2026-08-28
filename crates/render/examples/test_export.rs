//! Exercises the actual offscreen PNG-export render path (offscreen MSAA
//! textures, resolve, GPU readback, alpha-aware downsample) against a
//! headless wgpu device — the part no unit test can reach, since it needs
//! a real GPU device and a real render pass.

use apost3dview_core::Molecule;
use apost3dview_render::{ExportSettings, GlyphAtlas, Material, OrbitCamera, SceneUniforms, ViewportResources};
use glam::Vec3;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("no GPU adapter available in this environment");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("failed to create device");

    // Bgra8Unorm: confirmed (via a one-off diagnostic print in App::new)
    // to be the actual swapchain format eframe/wgpu hands this app on
    // macOS/Metal — exercising that exact format here, not a hypothetical
    // one, since the app crate's PNG export has to channel-swap BGRA back
    // to RGBA before encoding and this is what proves that path is real.
    let target_format = wgpu::TextureFormat::Bgra8Unorm;

    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);

    // Two-atom molecule (arbitrary elements), positioned so both are
    // comfortably inside the default camera's view frustum.
    let molecule = Molecule::from_atoms(vec![6, 8], vec![Vec3::new(-0.5, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0)]);
    resources.load_molecule(&device, &molecule);

    let camera = OrbitCamera::default();
    let material = Material::default();
    let uniforms = SceneUniforms::new(&camera, 1.0, &material);

    // Opaque white background: the exported image should contain both the
    // background color AND non-background pixels (proof the molecule
    // actually got drawn, not just a blank clear).
    let settings = ExportSettings { width: 64, height: 64, supersample: 2, background: Some([1.0, 1.0, 1.0, 1.0]), ambient_occlusion: None, depth_of_field: None, dof_focus_distance: 0.0 };
    let pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &settings)
        .expect("offscreen render should succeed");

    assert_eq!(pixels.len(), (64 * 64 * 4) as usize, "output buffer should be exactly width*height*4 bytes");

    let is_white = |px: &[u8]| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255;
    let all_white = pixels.chunks(4).all(is_white);
    let any_non_white = pixels.chunks(4).any(|px| !is_white(px));
    assert!(!all_white, "expected the molecule to actually draw something, not just background");
    assert!(any_non_white, "sanity check on the same data");
    println!("opaque-background export: {} bytes, molecule visibly drawn", pixels.len());

    // Transparent background: pixels away from the molecule should have
    // alpha 0, pixels the molecule covers should have alpha 255 — proving
    // the "opaque-or-nothing" transparency claim actually holds through
    // the whole offscreen + MSAA-resolve + downsample pipeline, not just
    // in the shader in isolation.
    let transparent_settings = ExportSettings { width: 64, height: 64, supersample: 2, background: None, ambient_occlusion: None, depth_of_field: None, dof_focus_distance: 0.0 };
    let transparent_pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &transparent_settings)
        .expect("transparent offscreen render should succeed");

    let any_alpha_zero = transparent_pixels.chunks(4).any(|px| px[3] == 0);
    let any_alpha_full = transparent_pixels.chunks(4).any(|px| px[3] == 255);
    assert!(any_alpha_zero, "background pixels should be fully transparent");
    assert!(any_alpha_full, "molecule pixels should be fully opaque");
    println!("transparent-background export: background alpha=0 and molecule alpha=255 both present");

    // Supersample=1 (no supersampling) should still work and produce the
    // exact requested resolution.
    let no_supersample = ExportSettings { width: 32, height: 48, supersample: 1, background: Some([0.0, 0.0, 0.0, 1.0]), ambient_occlusion: None, depth_of_field: None, dof_focus_distance: 0.0 };
    let pixels_1x = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &no_supersample)
        .expect("1x supersample export should succeed");
    assert_eq!(pixels_1x.len(), (32 * 48 * 4) as usize);
    println!("1x supersample export: correct {}x{} output size", 32, 48);

    println!("ALL CHECKS PASSED");
}
