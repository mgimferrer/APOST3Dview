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

/// Glyphs are rasterized at `GLYPH_RASTER_PX * SDF_SUPERSAMPLE` and the
/// resulting distance field is box-downsampled back down to the
/// `GLYPH_RASTER_PX` basis before being stored. The atlas stays the same
/// size either way; supersampling only makes the *values* it stores more
/// accurate, the same reason supersampled antialiasing looks better than
/// rendering at the target resolution directly.
const SDF_SUPERSAMPLE: u32 = 4;

/// Distance, in source raster pixels, either side of a glyph's edge that
/// the stored signed distance field spans before saturating to fully
/// inside/outside. Encoded into the R8Unorm texture as byte 128 = exactly
/// on the edge, 0/255 = `SDF_SPREAD_PX` or more outside/inside.
const SDF_SPREAD_PX: f32 = 6.0;

/// A pixel's offset (in source-bitmap texels) to the nearest "seed" pixel
/// found so far during the distance transform below — `dist_sq()` is what
/// the propagation actually compares. `9999` stands in for "no seed found
/// yet"; large enough that any real in-atlas offset never approaches it.
#[derive(Clone, Copy)]
struct EdtPoint {
    dx: i32,
    dy: i32,
}

impl EdtPoint {
    const SEED: EdtPoint = EdtPoint { dx: 0, dy: 0 };
    const FAR: EdtPoint = EdtPoint { dx: 9999, dy: 9999 };

    fn dist_sq(self) -> i64 {
        (self.dx as i64) * (self.dx as i64) + (self.dy as i64) * (self.dy as i64)
    }
}

fn edt_get(grid: &[EdtPoint], width: i32, height: i32, x: i32, y: i32) -> EdtPoint {
    if x < 0 || y < 0 || x >= width || y >= height {
        EdtPoint::FAR
    } else {
        grid[(y * width + x) as usize]
    }
}

fn edt_compare(grid: &[EdtPoint], width: i32, height: i32, best: &mut EdtPoint, x: i32, y: i32, ox: i32, oy: i32) {
    let neighbor = edt_get(grid, width, height, x + ox, y + oy);
    let candidate = EdtPoint { dx: neighbor.dx + ox, dy: neighbor.dy + oy };
    if candidate.dist_sq() < best.dist_sq() {
        *best = candidate;
    }
}

/// Eight-points sequential Euclidean distance transform: in two raster
/// passes (forward then backward over the image, each checking the 4
/// already-visited neighbors) propagates the nearest zero-distance "seed"
/// pixel to every other pixel — the standard compact approximation to a
/// full per-pixel Euclidean distance transform, accurate enough for font
/// SDFs and cheap enough to run per-glyph at atlas build time.
fn edt_propagate(grid: &mut [EdtPoint], width: i32, height: i32) {
    for y in 0..height {
        for x in 0..width {
            let mut best = edt_get(grid, width, height, x, y);
            edt_compare(grid, width, height, &mut best, x, y, -1, 0);
            edt_compare(grid, width, height, &mut best, x, y, 0, -1);
            edt_compare(grid, width, height, &mut best, x, y, -1, -1);
            edt_compare(grid, width, height, &mut best, x, y, 1, -1);
            grid[(y * width + x) as usize] = best;
        }
        for x in (0..width).rev() {
            let mut best = edt_get(grid, width, height, x, y);
            edt_compare(grid, width, height, &mut best, x, y, 1, 0);
            grid[(y * width + x) as usize] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let mut best = edt_get(grid, width, height, x, y);
            edt_compare(grid, width, height, &mut best, x, y, 1, 0);
            edt_compare(grid, width, height, &mut best, x, y, 0, 1);
            edt_compare(grid, width, height, &mut best, x, y, -1, 1);
            edt_compare(grid, width, height, &mut best, x, y, 1, 1);
            grid[(y * width + x) as usize] = best;
        }
        for x in 0..width {
            let mut best = edt_get(grid, width, height, x, y);
            edt_compare(grid, width, height, &mut best, x, y, -1, 0);
            grid[(y * width + x) as usize] = best;
        }
    }
}

/// Converts a grayscale coverage bitmap (as `fontdue` rasterizes it) into a
/// raw signed distance field, one `f32` per pixel in source-bitmap pixel
/// units (positive inside the glyph, negative outside, magnitude = distance
/// to the nearest edge). Kept as unclamped floats rather than encoded bytes
/// so the caller can supersample-then-downsample before any quantization
/// happens — averaging already-encoded bytes would bake in the low-res
/// source's staircase pattern instead of smoothing it away.
fn coverage_to_signed_distance(bitmap: &[u8], width: usize, height: usize) -> Vec<f32> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let w = width as i32;
    let h = height as i32;
    let n = bitmap.len();
    let mut grid_outside = vec![EdtPoint::FAR; n];
    let mut grid_inside = vec![EdtPoint::FAR; n];
    for (i, &value) in bitmap.iter().enumerate() {
        if value >= 128 {
            grid_inside[i] = EdtPoint::SEED;
        } else {
            grid_outside[i] = EdtPoint::SEED;
        }
    }
    edt_propagate(&mut grid_outside, w, h);
    edt_propagate(&mut grid_inside, w, h);

    (0..n)
        .map(|i| {
            let dist_to_outside = (grid_outside[i].dist_sq() as f32).sqrt();
            let dist_to_inside = (grid_inside[i].dist_sq() as f32).sqrt();
            dist_to_outside - dist_to_inside
        })
        .collect()
}

/// Box-downsamples a signed distance field by an integer `factor`, e.g.
/// turning a 256px supersampled field into a 64px one by averaging each
/// `factor`x`factor` block. Averaging in the continuous distance domain
/// (rather than downsampling the rasterized bitmap and running the EDT at
/// the low resolution directly) is what gives the final low-res atlas cell
/// smooth sub-texel gradients instead of a blocky staircase — the actual
/// fix for labels going pixelated once magnified past the atlas's native
/// resolution.
fn downsample_signed_distance(distances: &[f32], width: usize, height: usize, factor: usize) -> (Vec<f32>, usize, usize) {
    if width == 0 || height == 0 {
        return (Vec::new(), 0, 0);
    }
    let down_w = width.div_ceil(factor);
    let down_h = height.div_ceil(factor);
    let mut out = vec![0.0f32; down_w * down_h];
    for oy in 0..down_h {
        for ox in 0..down_w {
            let mut sum = 0.0f32;
            let mut count = 0u32;
            for sy in 0..factor {
                let y = oy * factor + sy;
                if y >= height {
                    continue;
                }
                for sx in 0..factor {
                    let x = ox * factor + sx;
                    if x >= width {
                        continue;
                    }
                    sum += distances[y * width + x];
                    count += 1;
                }
            }
            out[oy * down_w + ox] = if count > 0 { sum / count as f32 } else { 0.0 };
        }
    }
    (out, down_w, down_h)
}

/// Encodes a signed distance field (already in final-atlas pixel units, not
/// supersampled ones) into one byte per pixel — 128 sits exactly on the
/// glyph edge, above is inside, below is outside, spread over
/// `SDF_SPREAD_PX` pixels either side.
fn encode_signed_distance(distances: &[f32]) -> Vec<u8> {
    distances
        .iter()
        .map(|&signed| {
            let normalized = (signed / (2.0 * SDF_SPREAD_PX) + 0.5).clamp(0.0, 1.0);
            (normalized * 255.0).round() as u8
        })
        .collect()
}

/// Converts a desired on-screen font size (in pixels) plus the current
/// world-units-per-pixel (see `OrbitCamera::world_units_per_pixel`) into
/// the single scale factor `layout_label` needs, folding in the atlas's
/// fixed rasterization resolution.
pub fn glyph_scale_for_font_size(font_size_px: f32, world_units_per_pixel: f32) -> f32 {
    (font_size_px / GLYPH_RASTER_PX) * world_units_per_pixel
}

/// Converts a desired glyph height, already in world units, into the scale
/// factor `layout_label` needs. Unlike `glyph_scale_for_font_size`, this
/// has no dependency on camera distance — labels sized this way are real
/// fixed-size 3D geometry that grows and shrinks with zoom exactly like
/// the atom they're attached to, rather than holding a constant apparent
/// screen size the way a 2D overlay would.
pub fn glyph_scale_for_world_size(world_height: f32) -> f32 {
    world_height / GLYPH_RASTER_PX
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
    /// Vertical center (bearing_y + height/2) of a reference digit, in
    /// raster pixels above the baseline. `layout_label` subtracts this
    /// from every glyph's baseline-relative offset so a typical label
    /// (digits, uppercase symbols) ends up vertically centered on its
    /// anchor instead of sitting entirely above it.
    pub cap_height_center: f32,
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

        let supersample = SDF_SUPERSAMPLE as usize;
        let inv_supersample = 1.0 / SDF_SUPERSAMPLE as f32;

        for (index, &ch) in chars.iter().enumerate() {
            // Rasterize well above the atlas's stored resolution, compute
            // the distance field at that supersampled size, then
            // box-downsample it back down — see SDF_SUPERSAMPLE's doc
            // comment for why this (rather than rasterizing directly at
            // GLYPH_RASTER_PX) is what keeps magnified labels smooth
            // instead of staircased.
            let (raster_metrics, bitmap) = font.rasterize(ch, GLYPH_RASTER_PX * SDF_SUPERSAMPLE as f32);
            let signed_distance_ss = coverage_to_signed_distance(&bitmap, raster_metrics.width, raster_metrics.height);
            let (signed_distance, down_w, down_h) =
                downsample_signed_distance(&signed_distance_ss, raster_metrics.width, raster_metrics.height, supersample);
            let signed_distance_final_basis: Vec<f32> = signed_distance.iter().map(|d| d * inv_supersample).collect();
            let sdf = encode_signed_distance(&signed_distance_final_basis);

            let col = index as u32 % columns;
            let row = index as u32 / columns;
            let cell_x = col * ATLAS_CELL_PX;
            let cell_y = row * ATLAS_CELL_PX;

            for y in 0..down_h {
                for x in 0..down_w {
                    let src = y * down_w + x;
                    let dst_x = cell_x as usize + x;
                    let dst_y = cell_y as usize + y;
                    pixels[dst_y * atlas_width as usize + dst_x] = sdf[src];
                }
            }

            metrics.insert(
                ch,
                GlyphMetrics {
                    uv_min: [cell_x as f32 / atlas_width as f32, cell_y as f32 / atlas_height as f32],
                    uv_max: [
                        (cell_x + down_w as u32) as f32 / atlas_width as f32,
                        (cell_y + down_h as u32) as f32 / atlas_height as f32,
                    ],
                    width: down_w as f32,
                    height: down_h as f32,
                    bearing_x: raster_metrics.xmin as f32 * inv_supersample,
                    bearing_y: raster_metrics.ymin as f32 * inv_supersample,
                    advance: raster_metrics.advance_width * inv_supersample,
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

        let cap_height_center = metrics
            .get(&'0')
            .map(|m| m.bearing_y + m.height * 0.5)
            .unwrap_or(0.0);

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

        Self { texture_view, sampler, metrics, fallback, cap_height_center }
    }

    pub fn get(&self, ch: char) -> &GlyphMetrics {
        self.metrics.get(&ch).unwrap_or(&self.fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 9x9 bitmap with a solid 3x3 "inside" square (value 255) centered
    /// in an otherwise fully "outside" (value 0) field.
    fn solid_square_bitmap() -> (Vec<u8>, usize, usize) {
        let (w, h) = (9, 9);
        let mut bitmap = vec![0u8; w * h];
        for y in 3..6 {
            for x in 3..6 {
                bitmap[y * w + x] = 255;
            }
        }
        (bitmap, w, h)
    }

    #[test]
    fn sdf_center_is_further_inside_than_edge() {
        let (bitmap, w, h) = solid_square_bitmap();
        let distances = coverage_to_signed_distance(&bitmap, w, h);
        let sdf = encode_signed_distance(&distances);
        // Center of the inside square should be deeper inside (higher
        // byte value) than a pixel right at its edge.
        let center = sdf[4 * w + 4];
        let inside_edge = sdf[3 * w + 3];
        assert!(center > inside_edge, "center {center} should exceed edge {inside_edge}");
        assert!(center > 128, "center should read as inside (>128), got {center}");
    }

    #[test]
    fn sdf_outside_decreases_with_distance() {
        let (bitmap, w, h) = solid_square_bitmap();
        let distances = coverage_to_signed_distance(&bitmap, w, h);
        let sdf = encode_signed_distance(&distances);
        // Walking straight out from the square's right edge (x=5 is the
        // last inside column), distance-from-edge should strictly
        // decrease (byte value strictly decreases) until it saturates.
        let near = sdf[4 * w + 6];
        let far = sdf[4 * w + 8];
        assert!(near > far, "near-outside {near} should exceed far-outside {far}");
        assert!(near < 128, "just-outside pixel should read as outside (<128), got {near}");
    }

    #[test]
    fn sdf_empty_bitmap_returns_empty() {
        assert!(coverage_to_signed_distance(&[], 0, 0).is_empty());
    }

    #[test]
    fn downsample_averages_blocks_and_scales_correctly() {
        // A 4x4 field of increasing values, downsampled by factor 2, should
        // produce a 2x2 field where each output is the mean of its 2x2
        // source block.
        let distances: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let (down, w, h) = downsample_signed_distance(&distances, 4, 4, 2);
        assert_eq!((w, h), (2, 2));
        // Block (0,0) = source[0,1,4,5] = mean(0,1,4,5) = 2.5
        assert!((down[0] - 2.5).abs() < 1e-6);
        // Block (1,1) = source[10,11,14,15] = mean = 12.5
        assert!((down[3] - 12.5).abs() < 1e-6);
    }

    #[test]
    fn downsample_handles_non_multiple_dimensions() {
        // A 3x3 field downsampled by factor 2 should produce a 2x2 field
        // (ceil(3/2)=2) without panicking on the ragged last row/column.
        let distances: Vec<f32> = vec![1.0; 9];
        let (down, w, h) = downsample_signed_distance(&distances, 3, 3, 2);
        assert_eq!((w, h), (2, 2));
        assert_eq!(down.len(), 4);
        assert!(down.iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }
}
