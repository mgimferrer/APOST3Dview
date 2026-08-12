//! CPU-side text layout for 3D labels: walks a string's glyphs, accumulates
//! advance widths, and centers the result — producing per-glyph billboard
//! quad instances anchored to a single world-space point.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::glyphs::GlyphAtlas;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphInstance {
    pub label_anchor: [f32; 3],
    pub _padding0: f32,
    /// This glyph's center, in world units, relative to `label_anchor`
    /// along the camera's (right, up) basis.
    pub local_offset: [f32; 2],
    pub half_size: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 3],
    pub _padding1: f32,
}

/// Lays out `text` centered on `anchor`. `world_size_per_raster_px`
/// converts the atlas's fixed rasterization pixels into world units —
/// callers fold both the desired on-screen font size and the current
/// per-pixel world scale (which depends on distance from the camera, for
/// perspective-correct constant apparent size) into this one factor.
pub fn layout_label(atlas: &GlyphAtlas, text: &str, anchor: Vec3, world_size_per_raster_px: f32, color: [f32; 3]) -> Vec<GlyphInstance> {
    let chars: Vec<char> = text.chars().collect();
    let total_advance: f32 = chars.iter().map(|&ch| atlas.get(ch).advance).sum::<f32>() * world_size_per_raster_px;

    let mut cursor_x = -total_advance * 0.5;
    let mut instances = Vec::with_capacity(chars.len());
    for ch in chars {
        let info = atlas.get(ch);
        let glyph_w = info.width * world_size_per_raster_px;
        let glyph_h = info.height * world_size_per_raster_px;
        let offset_x = cursor_x + info.bearing_x * world_size_per_raster_px + glyph_w * 0.5;
        let offset_y = info.bearing_y * world_size_per_raster_px + glyph_h * 0.5;

        if glyph_w > 0.0 && glyph_h > 0.0 {
            instances.push(GlyphInstance {
                label_anchor: anchor.to_array(),
                _padding0: 0.0,
                local_offset: [offset_x, offset_y],
                half_size: [glyph_w * 0.5, glyph_h * 0.5],
                uv_min: info.uv_min,
                uv_max: info.uv_max,
                color,
                _padding1: 0.0,
            });
        }

        cursor_x += info.advance * world_size_per_raster_px;
    }
    instances
}
