//! Pure (no-GPU) helpers for PNG export: the settings struct describing
//! what to render, and a correct (alpha-aware) box downsample from a
//! supersampled render back down to the final output resolution. The
//! actual GPU orchestration (offscreen textures, the render pass, readback)
//! lives on `ViewportResources` in `viewport.rs`, since it needs access to
//! that struct's private pipelines/buffers — this module is just the math,
//! kept separate so it's testable without a GPU device.

/// What to render for a PNG export. `supersample` renders internally at
/// `width * supersample` x `height * supersample` and box-downsamples back
/// down — the simplest robust way to further smooth edges beyond the live
/// view's real-time MSAA, since it reuses the exact same rendering code
/// untouched and just aims it at a bigger offscreen texture first.
#[derive(Debug, Clone, Copy)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    /// `None` clears to a fully transparent background — safe to do
    /// cheaply because atoms/bonds already render fully opaque-or-nothing
    /// (the analytic ray-sphere/cylinder shaders `discard` everywhere they
    /// don't hit, no soft/semi-transparent edges besides text's own
    /// coverage), so "nothing drawn here" already means "transparent" with
    /// no further shader changes needed.
    pub background: Option<[f32; 4]>,
    /// `Some` runs the ambient-occlusion pre-passes (see `ao.rs`) and the
    /// AO-sampling shader variants before readback, at the given settings
    /// — `None` skips all of it, a real extra GPU cost otherwise. The same
    /// `AoSettings` also drive the live view, so what you tune there is
    /// what an export with AO on actually uses.
    pub ambient_occlusion: Option<crate::ao::AoSettings>,
    /// `Some` runs the depth-of-field post-process (see `dof.rs`) before
    /// readback — `None` skips it, a real extra GPU cost otherwise. The
    /// same `DofSettings` also drive the live view. `dof_focus_distance`
    /// (the camera's own orbit distance — see `dof.rs`'s module doc for
    /// why there's no separate focus-point control) is only meaningful
    /// alongside this.
    pub depth_of_field: Option<crate::dof::DofSettings>,
    pub dof_focus_distance: f32,
}

/// Box-downsamples an RGBA8 image (top-to-bottom row order) by an integer
/// `factor`, e.g. turning a supersampled render back into the requested
/// output resolution. Averages in premultiplied-alpha space and
/// un-premultiplies the result — naively averaging straight
/// (non-premultiplied) RGBA darkens color fringes near transparent edges,
/// since a transparent pixel's RGB (usually 0,0,0) would otherwise still
/// pull the average down even though it shouldn't contribute any color,
/// only reduce coverage.
pub fn downsample_rgba(pixels: &[u8], width: u32, height: u32, factor: u32) -> (Vec<u8>, u32, u32) {
    if factor <= 1 {
        return (pixels.to_vec(), width, height);
    }
    let down_w = width / factor;
    let down_h = height / factor;
    let mut out = vec![0u8; (down_w * down_h * 4) as usize];

    for oy in 0..down_h {
        for ox in 0..down_w {
            let mut sum_r = 0.0f32;
            let mut sum_g = 0.0f32;
            let mut sum_b = 0.0f32;
            let mut sum_a = 0.0f32;
            for sy in 0..factor {
                let y = oy * factor + sy;
                for sx in 0..factor {
                    let x = ox * factor + sx;
                    let i = ((y * width + x) * 4) as usize;
                    let a = pixels[i + 3] as f32 / 255.0;
                    sum_r += pixels[i] as f32 * a;
                    sum_g += pixels[i + 1] as f32 * a;
                    sum_b += pixels[i + 2] as f32 * a;
                    sum_a += a;
                }
            }
            let count = (factor * factor) as f32;
            let avg_a = (sum_a / count).clamp(0.0, 1.0);
            // Un-premultiply: dividing the premultiplied sums by the
            // *summed* alpha (not the sample count) recovers the true
            // alpha-weighted average color — the per-sample count cancels
            // out algebraically between the numerator and denominator.
            let (r, g, b) = if sum_a > 0.0001 {
                ((sum_r / sum_a).clamp(0.0, 255.0), (sum_g / sum_a).clamp(0.0, 255.0), (sum_b / sum_a).clamp(0.0, 255.0))
            } else {
                (0.0, 0.0, 0.0)
            };
            let o = ((oy * down_w + ox) * 4) as usize;
            out[o] = r.round() as u8;
            out[o + 1] = g.round() as u8;
            out[o + 2] = b.round() as u8;
            out[o + 3] = (avg_a * 255.0).round() as u8;
        }
    }

    (out, down_w, down_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_one_is_a_passthrough() {
        let pixels = vec![10, 20, 30, 255, 40, 50, 60, 128];
        let (out, w, h) = downsample_rgba(&pixels, 2, 1, 1);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out, pixels);
    }

    #[test]
    fn uniform_block_downsamples_unchanged() {
        // A solid 4x4 opaque green field should downsample to solid green.
        let mut pixels = vec![0u8; 4 * 4 * 4];
        for px in pixels.chunks_mut(4) {
            px.copy_from_slice(&[10, 200, 30, 255]);
        }
        let (out, w, h) = downsample_rgba(&pixels, 4, 4, 2);
        assert_eq!((w, h), (2, 2));
        for px in out.chunks(4) {
            assert_eq!(px, &[10, 200, 30, 255]);
        }
    }

    #[test]
    fn half_transparent_block_keeps_true_color_not_darkened() {
        // A 2x2 block: two fully-opaque red pixels, two fully-transparent
        // (black, alpha 0) pixels. Naively averaging straight RGBA would
        // give a darkened/desaturated red (since transparent black's RGB
        // still pulls the sum down); correct premultiplied-alpha averaging
        // should keep the color pure red at 50% coverage.
        let red_opaque = [255u8, 0, 0, 255];
        let transparent = [0u8, 0, 0, 0];
        let pixels: Vec<u8> = [red_opaque, red_opaque, transparent, transparent].concat();
        let (out, w, h) = downsample_rgba(&pixels, 2, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(&out[0..3], &[255, 0, 0], "color should stay pure red, not darkened");
        assert_eq!(out[3], 128, "alpha should reflect ~50% coverage (255*0.5 rounded)");
    }

    #[test]
    fn fully_transparent_block_stays_transparent() {
        let pixels = vec![0u8; 2 * 2 * 4];
        let (out, w, h) = downsample_rgba(&pixels, 2, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, vec![0, 0, 0, 0]);
    }
}
