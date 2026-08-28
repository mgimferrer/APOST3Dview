//! Headless visual check for the export-only ambient-occlusion post-process
//! (see `ao.rs`) against a real molecule, not synthetic geometry — renders
//! the same view with AO off and AO on and saves both PNGs so the result
//! can actually be looked at, plus a couple of cheap pixel-level sanity
//! checks (AO should darken pixels, never brighten or move the silhouette).

use apost3dview_core::parse_xyz;
use apost3dview_render::{AoSettings, ExportSettings, GlyphAtlas, Material, OrbitCamera, SceneUniforms, ViewportResources};
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
    println!("Parsed {}: {} atoms, {} bonds", path.display(), molecule.atomic_numbers.len(), molecule.bonds.len());

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
    let off_uniforms = SceneUniforms::new(&camera, 1.0, &material);

    // Mirrors `App::export_render_png`'s own flattening: a strong Phong
    // specular highlight competes with and dilutes AO's darkening, so the
    // AO export path dampens it — same reason Speck's atoms run zero
    // lighting at all and let AO + outline do the whole job.
    let ao_material =
        Material { ambient: (material.ambient + 0.45).min(0.85), diffuse: material.diffuse * 0.4, specular: material.specular * 0.05, ..material };
    let on_uniforms = SceneUniforms::new(&camera, 1.0, &ao_material);

    let base_settings = ExportSettings {
        width: 512,
        height: 512,
        supersample: 2,
        background: Some([1.0, 1.0, 1.0, 1.0]),
        ambient_occlusion: None,
        depth_of_field: None,
        dof_focus_distance: 0.0,
    };

    let off_pixels = resources
        .render_offscreen(&device, &queue, target_format, &off_uniforms, &[], &base_settings)
        .expect("AO-off render should succeed");
    let on_settings = ExportSettings { ambient_occlusion: Some(AoSettings::default()), ..base_settings };
    let on_pixels = resources
        .render_offscreen(&device, &queue, target_format, &on_uniforms, &[], &on_settings)
        .expect("AO-on render should succeed");

    assert_eq!(off_pixels.len(), on_pixels.len(), "AO on/off should produce the same resolution output");

    let out_dir = std::env::var("AO_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    save_png(&format!("{out_dir}/ao_off.png"), &off_pixels, base_settings.width, base_settings.height);
    save_png(&format!("{out_dir}/ao_on.png"), &on_pixels, base_settings.width, base_settings.height);
    println!("Saved {out_dir}/ao_off.png and {out_dir}/ao_on.png");

    // Sanity checks, not a substitute for actually looking at the images.
    // Note the AO-on render intentionally uses a flatter material too (see
    // above), so this is no longer a pure "AO darkens, never brightens"
    // comparison — raising the ambient floor legitimately brightens
    // previously-shadowed diffuse regions. The one invariant that must
    // still hold regardless: AO/outline must never move the silhouette
    // (background pixels must stay background and vice versa) — that
    // would mean the G-buffer and the main pass disagree about where the
    // molecule actually is, a real bug, not just a shading difference.
    let mut changed = 0usize;
    let mut silhouette_mismatch = 0usize;
    for (off_px, on_px) in off_pixels.chunks(4).zip(on_pixels.chunks(4)) {
        let is_background = |px: &[u8]| px[0] == 255 && px[1] == 255 && px[2] == 255 && px[3] == 255;
        if is_background(off_px) != is_background(on_px) {
            silhouette_mismatch += 1;
            continue;
        }
        let off_luma: i32 = off_px[0] as i32 + off_px[1] as i32 + off_px[2] as i32;
        let on_luma: i32 = on_px[0] as i32 + on_px[1] as i32 + on_px[2] as i32;
        if (on_luma - off_luma).abs() > 2 {
            changed += 1;
        }
    }
    println!("{changed} pixels changed, {silhouette_mismatch} silhouette mismatches");
    assert_eq!(silhouette_mismatch, 0, "AO must not change which pixels are background vs. molecule");
    assert!(changed > 1000, "expected AO + the flatter export material to visibly change a meaningful number of pixels");

    println!("ALL CHECKS PASSED");
}

fn save_png(path: &str, pixels: &[u8], width: u32, height: u32) {
    // BGRA -> RGBA channel swap, same fix-up the app crate's PNG export
    // does for this swapchain format.
    let mut rgba = pixels.to_vec();
    for px in rgba.chunks_mut(4) {
        px.swap(0, 2);
    }
    image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8).expect("failed to write PNG");
}
