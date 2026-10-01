//! Headless check for animated GIF export (`render_turntable_gif`) against
//! a real molecule and a real generated orbital: writes the GIF, decodes it
//! back, and checks frame count/size, that frame 0 matches a direct still
//! render of the starting view, that the scene actually turns, and that
//! the loop closes without a jump (the step from the last frame back to the
//! first is no bigger than an ordinary frame-to-frame step).

use apost3dview_core::{extract_isosurface, generate_mo_grids, parse_fchk_wavefunction, Molecule};
use apost3dview_render::{
    push_isosurface_vertices, render_turntable_gif, AoSettings, ExportSettings, GlyphAtlas, IsosurfaceVertex, Material, OrbitCamera, SceneUniforms,
    TurntableSettings, ViewportResources,
};
use glam::Vec2;
use std::path::PathBuf;

fn main() {
    pollster::block_on(run());
}

/// Decodes every frame of a GIF into full RGBA canvases, applying each
/// frame's position and disposal (gifski writes partial, transparent-
/// keyed frames that only update what changed).
fn decode_gif(path: &std::path::Path) -> (usize, usize, Vec<Vec<u8>>) {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(std::fs::File::open(path).unwrap()).unwrap();
    let (w, h) = (decoder.width() as usize, decoder.height() as usize);
    let mut canvas = vec![0u8; w * h * 4];
    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().unwrap() {
        let before = canvas.clone();
        let (fl, ft, fw, fh) = (frame.left as usize, frame.top as usize, frame.width as usize, frame.height as usize);
        for y in 0..fh {
            for x in 0..fw {
                let src = (y * fw + x) * 4;
                if frame.buffer[src + 3] > 0 {
                    let dst = ((ft + y) * w + (fl + x)) * 4;
                    canvas[dst..dst + 4].copy_from_slice(&frame.buffer[src..src + 4]);
                }
            }
        }
        frames.push(canvas.clone());
        match frame.dispose {
            gif::DisposalMethod::Background => {
                for y in 0..fh {
                    for x in 0..fw {
                        let dst = ((ft + y) * w + (fl + x)) * 4;
                        canvas[dst..dst + 4].fill(0);
                    }
                }
            }
            gif::DisposalMethod::Previous => canvas = before,
            _ => {}
        }
    }
    (w, h, frames)
}

/// Root-mean-square difference over RGB, 0-255 scale.
fn rms(a: &[u8], b: &[u8]) -> f64 {
    let (sum, n) = a.chunks(4).zip(b.chunks(4)).fold((0.0, 0usize), |(sum, n), (p, q)| {
        (sum + (0..3).map(|c| (p[c] as f64 - q[c] as f64).powi(2)).sum::<f64>(), n + 3)
    });
    (sum / n as f64).sqrt()
}

async fn run() {
    let path = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("TESTS-VISUALIZER/H2O-ccpVTZ.fchk"));
    let molecule = Molecule::from_fchk(&path).expect("failed to parse fchk geometry");
    let wfn = parse_fchk_wavefunction(&path).expect("failed to parse wavefunction");
    let mo = wfn.alpha.homo_index() - 2;
    let grid = &generate_mo_grids(&wfn.basis, &[(&wfn.alpha, mo)], 0.25, 6.0).expect("grid generation failed")[0];
    let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
    push_isosurface_vertices(&mut vertices, &extract_isosurface(grid, 0.09), [0.24, 0.35, 0.9], 1.0);
    push_isosurface_vertices(&mut vertices, &extract_isosurface(grid, -0.09), [0.86, 0.27, 0.24], 1.0);

    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm;
    let atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &atlas);
    resources.load_molecule(&device, &molecule);
    resources.update_isosurface(&device, &vertices);

    let (center, radius) = molecule.bounding_sphere();
    let mut camera = OrbitCamera::default();
    camera.frame_bounds(center, radius.max(1.9));
    let material = Material::default();
    let settings = ExportSettings {
        width: 400,
        height: 300,
        supersample: 2,
        background: Some([1.0, 1.0, 1.0, 1.0]),
        ambient_occlusion: Some(AoSettings::default()),
        depth_of_field: None,
        dof_focus_distance: camera.distance,
    };
    let frames = 48;
    // A diagonal direction exercises the exact-axis rotation the seamless
    // loop depends on.
    let turntable = TurntableSettings { frames, fps: 20.0, direction: Vec2::new(1.0, 0.6), quality: 90 };

    let out_dir = std::env::var("TURNTABLE_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    let gif_path = PathBuf::from(&out_dir).join("turntable.gif");
    let file = std::io::BufWriter::new(std::fs::File::create(&gif_path).unwrap());
    let started = std::time::Instant::now();
    render_turntable_gif(&mut resources, &device, &queue, target_format, &camera, &material, &settings, turntable, |_| Vec::new(), file)
        .expect("turntable export failed");
    let size_kb = std::fs::metadata(&gif_path).unwrap().len() / 1024;
    println!("wrote {} ({size_kb} KB, {frames} frames) in {:.1}s", gif_path.display(), started.elapsed().as_secs_f64());

    let (w, h, decoded) = decode_gif(&gif_path);
    println!("decoded {} frames, {w}x{h}", decoded.len());
    assert_eq!((w, h), (400, 300), "GIF should have the requested size");
    assert_eq!(decoded.len(), frames, "GIF should have one frame per requested frame");

    // Frame 0 should match a direct still render of the starting view
    // (differences are only GIF's palette quantization).
    let mut uniforms = SceneUniforms::new(&camera, 400.0 / 300.0, &material);
    uniforms.set_srgb_target(target_format.is_srgb());
    let mut still = resources.render_offscreen(&device, &queue, target_format, &uniforms, &[], &settings).unwrap();
    for px in still.chunks_mut(4) {
        px.swap(0, 2);
    }
    let first_vs_still = rms(&decoded[0], &still);
    println!("frame 0 vs still render: RMS {first_vs_still:.2}");
    assert!(first_vs_still < 3.0, "first frame should match the starting view");

    let step = rms(&decoded[0], &decoded[1]);
    let wrap = rms(&decoded[frames - 1], &decoded[0]);
    let half_turn = rms(&decoded[0], &decoded[frames / 2]);
    println!("consecutive step RMS {step:.2}, last->first RMS {wrap:.2}, half-turn RMS {half_turn:.2}");
    assert!(half_turn > step * 2.0, "the scene should actually turn over the animation");
    assert!(wrap < step * 1.5, "the loop should close without a jump");

    println!("ALL CHECKS PASSED");
}
