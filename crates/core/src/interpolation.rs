//! Tricubic (Catmull-Rom) interpolation for refining a sparse `ScalarGrid`
//! into a denser one before isosurface extraction. Marching cubes already
//! linearly interpolates *within* whatever grid it's given to place the
//! surface, but a coarse source grid still caps how smoothly the result
//! can bend — this fills in genuinely new points *between* the real
//! samples using a smooth cubic curve (not just straight segments), which
//! is what actually fixes a sparse cube's faceted look.
//!
//! Implemented as three separable 1D passes (X, then Y, then Z) rather
//! than a full 3D tricubic convolution — mathematically equivalent for a
//! separable kernel like Catmull-Rom, and much cheaper (a 4-tap 1D pass
//! three times over, instead of a 64-tap 3D one per output point).

use crate::cube::ScalarGrid;

/// Standard Catmull-Rom cubic through samples `p0..p3` (at relative
/// positions -1, 0, 1, 2), evaluated at `t` in `[0, 1]` between `p1` and
/// `p2`. `pub(crate)`: also used by `ScalarGrid::sample_tricubic` for
/// single-point smooth sampling (gradient/normal estimation), not just
/// this module's bulk grid refinement.
pub(crate) fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Number of points along an axis after refining `n` original points to
/// `factor`x density — refined points land exactly on the original ones
/// at every `factor`-th index, with `factor - 1` new points inserted
/// between each original pair.
fn refined_len(n: usize, factor: usize) -> usize {
    if n <= 1 {
        n
    } else {
        (n - 1) * factor + 1
    }
}

/// Refines one 1D sequence of `n` samples (read via `get`) to
/// `refined_len(n, factor)` samples (written via `set`). Out-of-range
/// neighbors needed near the ends of the sequence are clamped to the
/// nearest real sample rather than extrapolated.
fn interpolate_axis(n: usize, factor: usize, get: impl Fn(usize) -> f32, mut set: impl FnMut(usize, f32)) {
    let out_len = refined_len(n, factor);
    if n <= 1 {
        let value = get(0);
        for oi in 0..out_len {
            set(oi, value);
        }
        return;
    }
    let clamp_index = |i: isize| -> usize { i.clamp(0, n as isize - 1) as usize };
    for oi in 0..out_len {
        let pos = oi as f32 / factor as f32;
        let seg = (pos.floor() as usize).min(n - 2);
        let t = pos - seg as f32;
        let p0 = get(clamp_index(seg as isize - 1));
        let p1 = get(seg);
        let p2 = get(seg + 1);
        let p3 = get(clamp_index(seg as isize + 2));
        set(oi, catmull_rom(p0, p1, p2, p3, t));
    }
}

/// Refines `grid` to `factor`x the point density along every axis via
/// separable tricubic interpolation. `factor <= 1` returns an unchanged
/// copy. The step vectors shrink by `factor` (more, smaller voxels
/// covering the same physical volume) and the origin is unchanged, so
/// refined grid point `(i*factor, j*factor, k*factor)` always lands
/// exactly on original grid point `(i, j, k)`.
pub fn refine_grid(grid: &ScalarGrid, factor: usize) -> ScalarGrid {
    if factor <= 1 {
        return grid.clone();
    }
    let [nx, ny, nz] = grid.dims;
    let rx = refined_len(nx, factor);
    let ry = refined_len(ny, factor);
    let rz = refined_len(nz, factor);

    // Pass 1: refine along X — rx * ny * nz intermediate values.
    let mut pass1 = vec![0.0f32; rx * ny * nz];
    for j in 0..ny {
        for k in 0..nz {
            interpolate_axis(nx, factor, |i| grid.value_at(i, j, k), |oi, v| pass1[(oi * ny + j) * nz + k] = v);
        }
    }

    // Pass 2: refine along Y — rx * ry * nz intermediate values.
    let mut pass2 = vec![0.0f32; rx * ry * nz];
    for i in 0..rx {
        for k in 0..nz {
            interpolate_axis(ny, factor, |j| pass1[(i * ny + j) * nz + k], |oj, v| pass2[(i * ry + oj) * nz + k] = v);
        }
    }

    // Pass 3: refine along Z — rx * ry * rz final values.
    let mut values = vec![0.0f32; rx * ry * rz];
    for i in 0..rx {
        for j in 0..ry {
            interpolate_axis(nz, factor, |k| pass2[(i * ry + j) * nz + k], |ok, v| values[(i * ry + j) * rz + ok] = v);
        }
    }

    let steps = [grid.steps[0] / factor as f32, grid.steps[1] / factor as f32, grid.steps[2] / factor as f32];
    ScalarGrid { origin: grid.origin, dims: [rx, ry, rz], steps, values }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn axis_aligned_grid(dims: [usize; 3], step: f32, values: Vec<f32>) -> ScalarGrid {
        ScalarGrid {
            origin: Vec3::ZERO,
            dims,
            steps: [Vec3::new(step, 0.0, 0.0), Vec3::new(0.0, step, 0.0), Vec3::new(0.0, 0.0, step)],
            values,
        }
    }

    #[test]
    fn factor_one_is_unchanged() {
        let grid = axis_aligned_grid([2, 2, 2], 1.0, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let refined = refine_grid(&grid, 1);
        assert_eq!(refined.dims, grid.dims);
        assert_eq!(refined.values, grid.values);
    }

    #[test]
    fn refined_grid_reproduces_original_samples() {
        // A linear ramp along X: refining should still hit the exact
        // original values at the indices that land on original points.
        let n = 5;
        let values: Vec<f32> = (0..n * n * n).map(|i| (i % n) as f32).collect();
        let grid = axis_aligned_grid([n, n, n], 1.0, values);
        let refined = refine_grid(&grid, 3);
        assert_eq!(refined.dims, [13, 13, 13]);
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let expected = grid.value_at(i, j, k);
                    let actual = refined.value_at(i * 3, j * 3, k * 3);
                    assert!((actual - expected).abs() < 1e-4, "mismatch at ({i},{j},{k}): {actual} vs {expected}");
                }
            }
        }
    }

    #[test]
    fn refined_grid_is_smooth_on_a_linear_ramp() {
        // Catmull-Rom reproduces a linear function exactly (it's cubic,
        // linear is a special case) — a good sanity check that the
        // interpolation math itself isn't introducing overshoot/bias.
        let n = 6;
        let mut values = vec![0.0f32; n * n * n];
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    values[(i * n + j) * n + k] = i as f32 * 2.0; // linear in X only
                }
            }
        }
        let grid = axis_aligned_grid([n, n, n], 1.0, values);
        let refined = refine_grid(&grid, 4);
        // Midpoint between original X indices 2 and 3 should be exactly
        // the linear average, for any Y/Z.
        let mid_x = 2 * 4 + 2; // halfway between refined indices for x=2 and x=3
        let value = refined.value_at(mid_x, 5, 5);
        assert!((value - 5.0).abs() < 1e-3, "expected ~5.0 (linear midpoint), got {value}");
    }
}
