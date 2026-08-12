//! Glyph rasterization + atlas packing for true 3D labels (labels rendered
//! as depth-tested billboard geometry, not a 2D UI overlay, so they're
//! correctly occluded — even partially — by whatever's actually in front
//! of them in the scene).
//!
//! Reuses the font bytes egui already bundles (`default_fonts` feature)
//! rather than shipping a separate font asset — no new licensing surface,
//! and label text ends up visually consistent with the rest of the UI.
//! Every glyph needed is rasterized once at a fixed pixel size into one
//! atlas texture at startup; callers scale the resulting quads to
//! whatever final size they want rather than re-rasterizing per size.

use std::collections::HashMap;

use wgpu::util::DeviceExt;

/// Every character atom/measurement labels can actually produce: ASCII
/// printable range, plus the angstrom and degree signs.
fn character_set() -> Vec<char> {
    let mut chars: Vec<char> = (0x20u8..=0x7Eu8).map(|b| b as char).collect();
    chars.push('Å');
    chars.push('°');
    chars
}

/// Fixed rasterization size, in pixels — a middle-resolution glyph that
/// stays reasonably crisp whether a label ends up small (a hydrogen atom
/// tag) or large (a zoomed-in measurement), since it's just a texture
/// sampled onto a quad scaled after the fact.
const GLYPH_RASTER_PX: f32 = 64.0;
const ATLAS_CELL_PX: u32 = 72;

/// Converts a desired on-screen font size (in pixels) plus the current
/// world-units-per-pixel (see `OrbitCamera::world_units_per_pixel`) into
/// the single scale factor `layout_label` needs, folding in the atlas's
/// fixed rasterization resolution.
pub fn glyph_scale_for_font_size(font_size_px: f32, world_units_per_pixel: f32) -> f32 {
    (font_size_px / GLYPH_RASTER_PX) * world_units_per_pixel
}

#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    /// Rasterized bitmap size, in the same pixel units as `GLYPH_RASTER_PX`.
    pub width: f32,
    pub height: f32,
    /// Offset from the layout cursor to the bitmap's left/bottom edge.
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

pub struct GlyphAtlas {
    pub texture_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    metrics: HashMap<char, GlyphMetrics>,
    fallback: GlyphMetrics,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let font_bytes = egui::FontDefinitions::default()
            .font_data
            .get("Ubuntu-Light")
            .expect("egui's default_fonts bundle should include Ubuntu-Light")
            .font
            .clone();
        let font = fontdue::Font::from_bytes(font_bytes.as_ref(), fontdue::FontSettings::default())
            .expect("bundled font bytes should be a valid font");

        let chars = character_set();
        let columns = (chars.len() as f32).sqrt().ceil() as u32;
        let rows = (chars.len() as u32).div_ceil(columns);
        let atlas_width = columns * ATLAS_CELL_PX;
        let atlas_height = rows * ATLAS_CELL_PX;

        let mut pixels = vec![0u8; (atlas_width * atlas_height) as usize];
        let mut metrics = HashMap::with_capacity(chars.len());

        for (index, &ch) in chars.iter().enumerate() {
            let (raster_metrics, bitmap) = font.rasterize(ch, GLYPH_RASTER_PX);
            let col = index as u32 % columns;
            let row = index as u32 / columns;
            let cell_x = col * ATLAS_CELL_PX;
            let cell_y = row * ATLAS_CELL_PX;

            for y in 0..raster_metrics.height {
                for x in 0..raster_metrics.width {
                    let src = y * raster_metrics.width + x;
                    let dst_x = cell_x as usize + x;
                    let dst_y = cell_y as usize + y;
                    pixels[dst_y * atlas_width as usize + dst_x] = bitmap[src];
                }
            }

            metrics.insert(
                ch,
                GlyphMetrics {
                    uv_min: [cell_x as f32 / atlas_width as f32, cell_y as f32 / atlas_height as f32],
                    uv_max: [
                        (cell_x + raster_metrics.width as u32) as f32 / atlas_width as f32,
                        (cell_y + raster_metrics.height as u32) as f32 / atlas_height as f32,
                    ],
                    width: raster_metrics.width as f32,
                    height: raster_metrics.height as f32,
                    bearing_x: raster_metrics.xmin as f32,
                    bearing_y: raster_metrics.ymin as f32,
                    advance: raster_metrics.advance_width,
                },
            );
        }

        let fallback = *metrics.get(&'?').unwrap_or(&GlyphMetrics {
            uv_min: [0.0, 0.0],
            uv_max: [0.0, 0.0],
            width: 0.0,
            height: 0.0,
            bearing_x: 0.0,
            bearing_y: 0.0,
            advance: GLYPH_RASTER_PX * 0.5,
        });

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glyph_atlas_texture"),
                size: wgpu::Extent3d { width: atlas_width, height: atlas_height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &pixels,
        );
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph_atlas_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { texture_view, sampler, metrics, fallback }
    }

    pub fn get(&self, ch: char) -> &GlyphMetrics {
        self.metrics.get(&ch).unwrap_or(&self.fallback)
    }
}
