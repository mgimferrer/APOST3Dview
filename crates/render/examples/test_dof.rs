//! Headless visual check for the export depth-of-field post-process (see
//! `dof.rs`) against a real, spatially large molecule — DoF needs real
//! depth variance to show anything at all, unlike AO's own test (BiCl3,
//! 4 atoms, works fine for AO but is too flat/small to usefully judge
//! focus falloff). Saves DoF-off/on PNGs for visual inspection, plus a
//! cheap sanity check: pixels far from the focal plane should change
//! noticeably (blurred), the silhouette's overall footprint should stay
//! roughly where it was (no accidental shift/scale).

use apost3dview_core::Molecule;
use apost3dview_render::{DofSettings, ExportSettings, GlyphAtlas, Material, OrbitCamera, SceneUniforms, ViewportResources};
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
    println!("Parsed {}: {} atoms, {} bonds", path.display(), molecule.atomic_numbers.len(), molecule.bonds.len());

    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm;

    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);
    resources.load_molecule(&device, &molecule);

    let (center, radius) = molecule.bounding_sphere();
    println!("bounding sphere radius: {radius:.2} Angstrom");
    let mut camera = OrbitCamera::default();
    camera.frame_bounds(center, radius);
    println!("camera distance: {:.2} Angstrom", camera.distance);
    let material = Material::default();
    let uniforms = SceneUniforms::new(&camera, 1.0, &material);

    let base_settings = ExportSettings {
        width: 512,
        height: 512,
        supersample: 2,
        background: Some([1.0, 1.0, 1.0, 1.0]),
        ambient_occlusion: None,
        depth_of_field: None,
        dof_focus_distance: camera.distance,
    };

    let off_pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &base_settings)
        .expect("DoF-off render should succeed");
    let on_settings = ExportSettings { depth_of_field: Some(DofSettings::default()), ..base_settings };
    let on_pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &on_settings)
        .expect("DoF-on render should succeed");

    let out_dir = std::env::var("DOF_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    image::save_buffer(format!("{out_dir}/dof_off.png"), &off_pixels, base_settings.width, base_settings.height, image::ColorType::Rgba8)
        .expect("failed to save dof_off.png");
    image::save_buffer(format!("{out_dir}/dof_on.png"), &on_pixels, base_settings.width, base_settings.height, image::ColorType::Rgba8)
        .expect("failed to save dof_on.png");
    println!("Saved {out_dir}/dof_off.png and {out_dir}/dof_on.png");

    assert_eq!(off_pixels.len(), on_pixels.len());
    let mut changed = 0usize;
    let mut off_nonbackground = 0usize;
    let mut on_nonbackground = 0usize;
    for px in off_pixels.chunks_exact(4).zip(on_pixels.chunks_exact(4)) {
        let (a, b) = px;
        if a != b {
            changed += 1;
        }
        if a != [255, 255, 255, 255] {
            off_nonbackground += 1;
        }
        if b != [255, 255, 255, 255] {
            on_nonbackground += 1;
        }
    }
    println!("{changed} pixels changed by DoF, {off_nonbackground} non-background off / {on_nonbackground} non-background on");
    assert!(changed > 1000, "DoF should visibly change a meaningful number of pixels on a molecule with real depth variance");
    // Silhouette footprint shouldn't collapse or balloon — DoF blurs, it
    // doesn't erase or expand geometry.
    let ratio = on_nonbackground as f64 / off_nonbackground as f64;
    assert!(ratio > 0.8 && ratio < 1.2, "non-background pixel count changed too much: ratio {ratio:.2}");

    println!("\nALL CHECKS PASSED");
}
