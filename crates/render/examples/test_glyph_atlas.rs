//! Exercises the actual glyph atlas + layout pipeline against a headless
//! wgpu device (no window/surface needed) — the part test_label_layout
//! can't reach, since it needs a real GPU device for the atlas texture.

use apost3dview_render::{glyph_scale_for_font_size, layout_label, GlyphAtlas};
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
    let atlas = GlyphAtlas::new(&device, &queue);

    // Known glyph metrics sanity: 'M' should be wider than 'i'.
    let m = atlas.get('M');
    let i = atlas.get('i');
    println!("'M' advance={}, 'i' advance={}", m.advance, i.advance);
    assert!(m.advance > i.advance, "'M' should be wider than 'i'");

    // Layout "C12" and check: 3 glyph instances (no spaces to skip), and
    // x-offsets strictly increasing left to right.
    let scale = glyph_scale_for_font_size(16.0, 0.01);
    let instances = layout_label(&atlas, "C12", Vec3::ZERO, scale, [0.0, 0.0, 0.0], 0.5);
    println!("'C12' produced {} glyph instances", instances.len());
    assert_eq!(instances.len(), 3, "expected one instance per character");
    for pair in instances.windows(2) {
        assert!(pair[1].local_offset[0] > pair[0].local_offset[0], "glyphs should lay out left to right");
    }

    // Every instance should share the same anchor (all part of one label).
    for inst in &instances {
        assert_eq!(inst.label_anchor, [0.0, 0.0, 0.0]);
    }

    // A space should be skipped (zero-size glyph, not emitted as an
    // instance) rather than producing a degenerate quad.
    let with_space = layout_label(&atlas, "a b", Vec3::ZERO, scale, [0.0, 0.0, 0.0], 0.5);
    println!("'a b' produced {} glyph instances (space skipped)", with_space.len());
    assert_eq!(with_space.len(), 2);

    println!("ALL CHECKS PASSED");
}
