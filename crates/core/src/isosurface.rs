//! Isosurface extraction from a `ScalarGrid` via marching tetrahedra
//! rather than the more commonly cited marching cubes. Deliberately
//! chosen over marching cubes: cubes need a large (256-case) precomputed
//! lookup table that's easy to get subtly wrong transcribing by hand and
//! hard to verify by inspection, and some of its cube-face configurations
//! are topologically ambiguous (multiple valid triangulations exist).
//! Splitting each cube into 6 tetrahedra first avoids both problems — a
//! tetrahedron only has 16 corner-sign combinations, they collapse to
//! just three shapes (0, 1, or 2 triangles) by corner count, and the
//! isosurface within a single tetrahedron of a piecewise-linear field is
//! always exactly planar, so there's never an ambiguous case to resolve.
//! The tradeoff is more triangles for the same grid than an optimal
//! marching-cubes mesh would produce, which doesn't matter here — real
//! isosurfaces only pass through a small fraction of cells, and orbital/
//! density isosurfaces are smooth blobs, not huge meshes.

use glam::Vec3;

use crate::cube::ScalarGrid;

/// A triangle-soup mesh (no shared-vertex indexing — see module docs on
/// why that tradeoff is fine here): every 3 consecutive entries are one
/// triangle. Normals come from the scalar field's own gradient (central
/// differences), not triangle winding, so lighting is correct regardless
/// of how a given triangle happens to be wound.
pub struct IsosurfaceMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

impl IsosurfaceMesh {
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// The cube's 8 corners as (di, dj, dk) offsets from its "low" corner.
const CORNERS: [(i32, i32, i32); 8] =
    [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0), (0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)];

/// Splits a cube into 6 tetrahedra, all sharing the space diagonal from
/// corner (0,0,0) to corner (1,1,1). The other 6 corners form a hexagonal
/// cycle connected by real cube edges — (1,0,0)-(1,1,0)-(0,1,0)-(0,1,1)-
/// (0,0,1)-(1,0,1)-back to (1,0,0) — and each tetrahedron is the diagonal
/// plus one consecutive pair from that cycle. This exactly partitions the
/// cube's volume with no gaps or overlaps (each tet has volume 1/6 of the
/// cube, verified by construction).
const TETRAHEDRA_EVEN: [[(i32, i32, i32); 4]; 6] = [
    [(0, 0, 0), (1, 1, 1), (1, 0, 0), (1, 1, 0)],
    [(0, 0, 0), (1, 1, 1), (1, 1, 0), (0, 1, 0)],
    [(0, 0, 0), (1, 1, 1), (0, 1, 0), (0, 1, 1)],
    [(0, 0, 0), (1, 1, 1), (0, 1, 1), (0, 0, 1)],
    [(0, 0, 0), (1, 1, 1), (0, 0, 1), (1, 0, 1)],
    [(0, 0, 0), (1, 1, 1), (1, 0, 1), (1, 0, 0)],
];

/// The same construction as `TETRAHEDRA_EVEN`, but along the *other*
/// space diagonal, from corner (1,0,0) to corner (0,1,1) — used on a
/// pseudo-randomly chosen subset of cube cells (see `cell_uses_odd_diagonal`)
/// so the triangulation's "grain" doesn't line up the same way across the
/// whole grid. Splitting every cell along the *same* diagonal produces a
/// visible directional ridging pattern on curved surfaces; alternating on
/// a simple checkerboard (tried first) reduces but doesn't eliminate it,
/// since a period-2 pattern is still perfectly regular — a hashed,
/// non-periodic choice removes the regularity entirely.
const TETRAHEDRA_ODD: [[(i32, i32, i32); 4]; 6] = [
    [(1, 0, 0), (0, 1, 1), (0, 0, 0), (0, 1, 0)],
    [(1, 0, 0), (0, 1, 1), (0, 1, 0), (1, 1, 0)],
    [(1, 0, 0), (0, 1, 1), (1, 1, 0), (1, 1, 1)],
    [(1, 0, 0), (0, 1, 1), (1, 1, 1), (1, 0, 1)],
    [(1, 0, 0), (0, 1, 1), (1, 0, 1), (0, 0, 1)],
    [(1, 0, 0), (0, 1, 1), (0, 0, 1), (0, 0, 0)],
];

/// Deterministic but non-periodic per-cell choice of which space diagonal
/// to split along — a standard integer hash-mixing technique (splitmix64
/// -style finalizer), reduced to one bit. Not cryptographic, just needs
/// to avoid any correlation with the grid's own axes, which a simple
/// `(i+j+k) % 2` checkerboard doesn't fully avoid (it's still a perfectly
/// regular period-2 pattern).
fn cell_uses_odd_diagonal(i: usize, j: usize, k: usize) -> bool {
    let mut h = (i as u64).wrapping_mul(0x9E3779B97F4A7C15);
    h ^= (j as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= (k as u64).wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51AFD7ED558CCD);
    h ^= h >> 33;
    (h & 1) == 1
}

/// Position where the field crosses `isovalue` along the edge from
/// `(pa, va)` to `(pb, vb)` — `va`/`vb` must straddle `isovalue`.
fn edge_crossing(pa: Vec3, va: f32, pb: Vec3, vb: f32, isovalue: f32) -> Vec3 {
    let t = ((isovalue - va) / (vb - va)).clamp(0.0, 1.0);
    pa.lerp(pb, t)
}

/// One tetrahedron corner: its world position and scalar value.
#[derive(Clone, Copy)]
struct TetCorner {
    position: Vec3,
    value: f32,
}

/// Emits this tetrahedron's contribution to the isosurface (0, 1, or 2
/// triangles depending on how many of its 4 corners are "inside",
/// i.e. have value >= isovalue) into `positions`.
fn triangulate_tetrahedron(corners: [TetCorner; 4], isovalue: f32, positions: &mut Vec<Vec3>) {
    let inside: [bool; 4] = std::array::from_fn(|i| corners[i].value >= isovalue);
    let inside_count = inside.iter().filter(|&&b| b).count();

    match inside_count {
        0 | 4 => {}
        1 | 3 => {
            // Exactly one corner is on the minority side (alone inside,
            // for count==1, or alone outside, for count==3) — the
            // isosurface here is the single triangle where the 3 edges
            // from that corner to the other 3 cross the isovalue.
            let solo = inside.iter().position(|&b| b == (inside_count == 1)).unwrap();
            let others: Vec<usize> = (0..4).filter(|&i| i != solo).collect();
            let solo_corner = corners[solo];
            let p0 = edge_crossing(solo_corner.position, solo_corner.value, corners[others[0]].position, corners[others[0]].value, isovalue);
            let p1 = edge_crossing(solo_corner.position, solo_corner.value, corners[others[1]].position, corners[others[1]].value, isovalue);
            let p2 = edge_crossing(solo_corner.position, solo_corner.value, corners[others[2]].position, corners[others[2]].value, isovalue);
            positions.push(p0);
            positions.push(p1);
            positions.push(p2);
        }
        2 => {
            // Two corners inside (p, q), two outside (r, s). The
            // isosurface is the planar quadrilateral where the 4 edges
            // p-r, p-s, q-r, q-s cross the isovalue (a tetrahedron's
            // level set is always exactly planar for this case, so
            // either diagonal split into 2 triangles is correct — no
            // ambiguity to resolve, unlike marching cubes' cube faces).
            let inside_idx: Vec<usize> = (0..4).filter(|&i| inside[i]).collect();
            let outside_idx: Vec<usize> = (0..4).filter(|&i| !inside[i]).collect();
            let (p, q) = (corners[inside_idx[0]], corners[inside_idx[1]]);
            let (r, s) = (corners[outside_idx[0]], corners[outside_idx[1]]);

            let pr = edge_crossing(p.position, p.value, r.position, r.value, isovalue);
            let ps = edge_crossing(p.position, p.value, s.position, s.value, isovalue);
            let qr = edge_crossing(q.position, q.value, r.position, r.value, isovalue);
            let qs = edge_crossing(q.position, q.value, s.position, s.value, isovalue);

            positions.push(pr);
            positions.push(ps);
            positions.push(qr);

            positions.push(qr);
            positions.push(ps);
            positions.push(qs);
        }
        _ => unreachable!(),
    }
}

/// Central-difference gradient of `grid` at fractional grid-index
/// coordinates, converted to a world-space direction (assumes an
/// axis-aligned grid, true for essentially every real `.cube` file — see
/// `ScalarGrid::steps`' own doc comment on the general case). Used for
/// per-vertex normals: pointing in the direction of *decreasing* value,
/// i.e. outward from the "inside" (value >= isovalue) region, which is
/// what the rest of the renderer's lighting model expects.
///
/// Samples via `sample_tricubic`, not `sample_trilinear` — trilinear is
/// only C0 (piecewise-linear), so its derivative jumps at every grid cell
/// boundary; differentiating that produces small grid-periodic noise in
/// the normals that a specular highlight (`pow(dot(n,h), shininess)`)
/// amplifies into clearly visible banding on curved isosurfaces. Tricubic
/// is C1 — a genuinely smooth derivative everywhere — which fixes that at
/// the source instead of needing an ever-finer grid to shrink it away.
fn gradient_normal(grid: &ScalarGrid, fi: f32, fj: f32, fk: f32) -> Vec3 {
    const H: f32 = 0.5;
    let dx = (grid.sample_tricubic(fi + H, fj, fk) - grid.sample_tricubic(fi - H, fj, fk)) / (2.0 * H * grid.steps[0].length().max(1e-6));
    let dy = (grid.sample_tricubic(fi, fj + H, fk) - grid.sample_tricubic(fi, fj - H, fk)) / (2.0 * H * grid.steps[1].length().max(1e-6));
    let dz = (grid.sample_tricubic(fi, fj, fk + H) - grid.sample_tricubic(fi, fj, fk - H)) / (2.0 * H * grid.steps[2].length().max(1e-6));
    let gradient = Vec3::new(dx, dy, dz);
    (-gradient).normalize_or_zero()
}

/// Extracts the isosurface where `grid`'s field crosses `isovalue`
/// ("inside" is `value >= isovalue`). To get the *negative*-sign lobe of
/// a signed field (an orbital, not a density), call this on
/// `grid.negated()` instead of trying to pass a negative `isovalue`
/// directly — see `ScalarGrid::negated`'s doc comment for why.
pub fn extract_isosurface(grid: &ScalarGrid, isovalue: f32) -> IsosurfaceMesh {
    let [nx, ny, nz] = grid.dims;
    let mut positions = Vec::new();

    if nx < 2 || ny < 2 || nz < 2 {
        return IsosurfaceMesh { positions, normals: Vec::new() };
    }

    for i in 0..nx - 1 {
        for j in 0..ny - 1 {
            for k in 0..nz - 1 {
                let cube_values: [f32; 8] =
                    std::array::from_fn(|c| grid.value_at(i + CORNERS[c].0 as usize, j + CORNERS[c].1 as usize, k + CORNERS[c].2 as usize));
                let min = cube_values.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = cube_values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                if isovalue < min || isovalue > max {
                    continue; // whole cube on one side — no triangles possible
                }

                let cube_positions: [Vec3; 8] = std::array::from_fn(|c| {
                    grid.world_position((i + CORNERS[c].0 as usize) as f32, (j + CORNERS[c].1 as usize) as f32, (k + CORNERS[c].2 as usize) as f32)
                });

                let tetrahedra = if cell_uses_odd_diagonal(i, j, k) { &TETRAHEDRA_ODD } else { &TETRAHEDRA_EVEN };
                for tet in tetrahedra {
                    let corners: [TetCorner; 4] = std::array::from_fn(|t| {
                        let (di, dj, dk) = tet[t];
                        let corner_index = CORNERS.iter().position(|&c| c == (di, dj, dk)).unwrap();
                        TetCorner { position: cube_positions[corner_index], value: cube_values[corner_index] }
                    });
                    triangulate_tetrahedron(corners, isovalue, &mut positions);
                }
            }
        }
    }

    // Normals computed separately from a fresh gradient sample per vertex
    // position (converted back to fractional grid-index coordinates)
    // rather than during triangulation — keeps triangulate_tetrahedron
    // free of any grid/gradient concerns, just geometry.
    let normals: Vec<Vec3> = positions
        .iter()
        .map(|&p| {
            let local = p - grid.origin;
            // Only exact for an axis-aligned grid (steps[axis] parallel
            // to that axis) — true for every real file this was tested
            // against; a sheared grid would need the full inverse basis.
            let fi = local.x / grid.steps[0].x;
            let fj = local.y / grid.steps[1].y;
            let fk = local.z / grid.steps[2].z;
            gradient_normal(grid, fi, fj, fk)
        })
        .collect();

    IsosurfaceMesh { positions, normals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetrahedron_volume(corners: [(i32, i32, i32); 4]) -> f32 {
        let p: [Vec3; 4] = std::array::from_fn(|i| Vec3::new(corners[i].0 as f32, corners[i].1 as f32, corners[i].2 as f32));
        let v1 = p[1] - p[0];
        let v2 = p[2] - p[0];
        let v3 = p[3] - p[0];
        (v1.cross(v2).dot(v3) / 6.0).abs()
    }

    /// Both diagonal decompositions must exactly partition the unit cube
    /// (6 tetrahedra summing to volume 1, no gaps or overlaps) — checked
    /// programmatically rather than trusting the by-hand derivation in
    /// the doc comments alone. `TETRAHEDRA_ODD` in particular was newly
    /// derived (added to fix a directional-ridging artifact from always
    /// splitting along the same diagonal), so this is real verification,
    /// not just a formality.
    #[test]
    fn both_diagonal_decompositions_partition_the_unit_cube() {
        for tetrahedra in [&TETRAHEDRA_EVEN, &TETRAHEDRA_ODD] {
            let total: f32 = tetrahedra.iter().map(|&t| tetrahedron_volume(t)).sum();
            assert!((total - 1.0).abs() < 1e-6, "expected 6 tetrahedra to sum to the unit cube's volume (1.0), got {total}");
            for &t in tetrahedra {
                assert!(tetrahedron_volume(t) > 1e-6, "found a degenerate (zero-volume) tetrahedron: {t:?}");
            }
        }
    }

    /// A grid where value(p) = -|p| (distance from center, negated) —
    /// extracting at isovalue = -R gives exactly the sphere of radius R,
    /// since -|p| >= -R  <=>  |p| <= R.
    fn negative_distance_grid(n: usize, extent: f32) -> ScalarGrid {
        let step = 2.0 * extent / (n - 1) as f32;
        let origin = Vec3::splat(-extent);
        let mut values = vec![0.0f32; n * n * n];
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let p = origin + Vec3::new(i as f32, j as f32, k as f32) * step;
                    values[(i * n + j) * n + k] = -p.length();
                }
            }
        }
        ScalarGrid {
            origin,
            dims: [n, n, n],
            steps: [Vec3::new(step, 0.0, 0.0), Vec3::new(0.0, step, 0.0), Vec3::new(0.0, 0.0, step)],
            values,
        }
    }

    #[test]
    fn sphere_isosurface_vertices_are_at_the_right_radius() {
        let grid = negative_distance_grid(41, 5.0);
        let radius = 3.0;
        let mesh = extract_isosurface(&grid, -radius);

        assert!(!mesh.is_empty(), "expected a non-empty mesh");
        assert_eq!(mesh.positions.len() % 3, 0, "triangle soup should have a multiple of 3 vertices");

        let grid_spacing = 2.0 * 5.0 / 40.0;
        let tolerance = grid_spacing * 1.5;
        for &p in &mesh.positions {
            let r = p.length();
            assert!((r - radius).abs() < tolerance, "vertex at radius {r}, expected ~{radius} (tolerance {tolerance})");
        }
    }

    #[test]
    fn sphere_isosurface_normals_point_outward() {
        let grid = negative_distance_grid(41, 5.0);
        let mesh = extract_isosurface(&grid, -3.0);

        assert!(!mesh.normals.is_empty());
        let mut dot_sum = 0.0f32;
        for (&p, &n) in mesh.positions.iter().zip(&mesh.normals) {
            let radial = p.normalize_or_zero();
            dot_sum += radial.dot(n);
        }
        let average_alignment = dot_sum / mesh.positions.len() as f32;
        assert!(average_alignment > 0.9, "normals should point radially outward on average, got alignment {average_alignment}");
    }

    #[test]
    fn isovalue_outside_data_range_produces_empty_mesh() {
        let grid = negative_distance_grid(11, 5.0);
        let mesh = extract_isosurface(&grid, 100.0); // no point ever reaches this
        assert!(mesh.is_empty());
    }

    #[test]
    fn tiny_grid_does_not_panic() {
        let grid = ScalarGrid {
            origin: Vec3::ZERO,
            dims: [1, 1, 1],
            steps: [Vec3::X, Vec3::Y, Vec3::Z],
            values: vec![0.0],
        };
        let mesh = extract_isosurface(&grid, 0.0);
        assert!(mesh.is_empty());
    }
}
