//! Isosurface extraction from a `ScalarGrid` via Surface Nets — one
//! vertex per active grid cell, positioned by averaging where the
//! isosurface actually crosses that cell's edges, connected to
//! neighboring cells' vertices via quads (split into triangles) — plus a
//! Taubin smoothing pass on the resulting mesh before it's handed out.
//!
//! This replaced an earlier marching-tetrahedra implementation. Marching
//! tetrahedra is topologically sound (no ambiguous cases, unlike marching
//! cubes' large lookup table), but it still constrains every vertex to
//! lie on one of a small, fixed set of lattice-edge directions — so even
//! with per-vertex normals computed from a perfectly smooth field, the
//! *triangle shapes themselves* stayed correlated with the grid's own
//! axes, showing up as view-angle-dependent banding under specular
//! lighting. Surface Nets' one-vertex-per-cell, averaged-position rule
//! decouples vertex placement from any single lattice direction — but
//! that averaging step is itself less numerically stable than direct
//! edge interpolation (an atypical pattern of which edges cross can pull
//! a cell's vertex slightly off from where a truly smooth surface would
//! put it), which showed up as small-scale speckling under specular
//! light instead. The Taubin pass below is the standard fix real
//! visualization tools (VMD included) apply after grid-based isosurface
//! extraction generally, not something specific to Surface Nets — it
//! nudges each vertex toward its mesh neighbors' average while
//! alternating with a compensating pass that cancels out the shrinkage
//! plain (Laplacian) neighbor-averaging would otherwise cause.
//!
//! Real Dual Contouring (Surface Nets' more sophisticated sibling) solves
//! a per-cell least-squares problem using each crossing's normal to
//! preserve sharp features; plain averaging (used here) has no
//! sharp-feature preservation, which is fine for the smooth, blobby
//! shapes orbital/density isosurfaces actually are.

use glam::Vec3;

use crate::cube::ScalarGrid;

/// A triangle-soup mesh (no shared-vertex indexing in the *public*
/// result — see `extract_isosurface`'s doc comment on why the indexed
/// mesh built internally for smoothing gets flattened back out before
/// returning): every 3 consecutive entries are one triangle. Normals
/// come from the scalar field's own gradient (central differences), not
/// triangle winding, so lighting is correct regardless of how a given
/// triangle happens to be wound.
pub struct IsosurfaceMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

impl IsosurfaceMesh {
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// The unit cube's 8 corners, as offsets from its "low" corner.
const CUBE_CORNERS: [(i32, i32, i32); 8] =
    [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)];

/// The unit cube's 12 edges, as (corner_a, corner_b) offset pairs.
const CUBE_EDGES: [((i32, i32, i32), (i32, i32, i32)); 12] = [
    // Along X
    ((0, 0, 0), (1, 0, 0)),
    ((0, 1, 0), (1, 1, 0)),
    ((0, 0, 1), (1, 0, 1)),
    ((0, 1, 1), (1, 1, 1)),
    // Along Y
    ((0, 0, 0), (0, 1, 0)),
    ((1, 0, 0), (1, 1, 0)),
    ((0, 0, 1), (0, 1, 1)),
    ((1, 0, 1), (1, 1, 1)),
    // Along Z
    ((0, 0, 0), (0, 0, 1)),
    ((1, 0, 0), (1, 0, 1)),
    ((0, 1, 0), (0, 1, 1)),
    ((1, 1, 0), (1, 1, 1)),
];

/// Position where the field crosses `isovalue` along the edge from
/// `(pa, va)` to `(pb, vb)` — `va`/`vb` must straddle `isovalue`.
fn edge_crossing(pa: Vec3, va: f32, pb: Vec3, vb: f32, isovalue: f32) -> Vec3 {
    let t = ((isovalue - va) / (vb - va)).clamp(0.0, 1.0);
    pa.lerp(pb, t)
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
/// the normals. Tricubic is C1 — a genuinely smooth derivative everywhere.
fn gradient_normal(grid: &ScalarGrid, fi: f32, fj: f32, fk: f32) -> Vec3 {
    const H: f32 = 0.5;
    let dx = (grid.sample_tricubic(fi + H, fj, fk) - grid.sample_tricubic(fi - H, fj, fk)) / (2.0 * H * grid.steps[0].length().max(1e-6));
    let dy = (grid.sample_tricubic(fi, fj + H, fk) - grid.sample_tricubic(fi, fj - H, fk)) / (2.0 * H * grid.steps[1].length().max(1e-6));
    let dz = (grid.sample_tricubic(fi, fj, fk + H) - grid.sample_tricubic(fi, fj, fk - H)) / (2.0 * H * grid.steps[2].length().max(1e-6));
    let gradient = Vec3::new(dx, dy, dz);
    (-gradient).normalize_or_zero()
}

/// Builds each vertex's set of unique mesh neighbors (vertices sharing a
/// triangle edge with it) from a triangle index list — needed for
/// smoothing, since "average of my neighbors" only means something once
/// vertices are actually shared rather than duplicated per-triangle.
fn build_adjacency(vertex_count: usize, indices: &[u32]) -> Vec<Vec<u32>> {
    let mut neighbor_sets: Vec<std::collections::HashSet<u32>> = vec![std::collections::HashSet::new(); vertex_count];
    for tri in indices.chunks_exact(3) {
        for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            neighbor_sets[a as usize].insert(b);
            neighbor_sets[b as usize].insert(a);
        }
    }
    neighbor_sets.into_iter().map(|s| s.into_iter().collect()).collect()
}

/// Taubin smoothing (Taubin 1995, "Curve and Surface Smoothing without
/// Shrinkage"): alternates a normal Laplacian pass (move each vertex
/// toward its neighbors' average, factor `LAMBDA`) with a compensating
/// pass in the opposite direction with slightly larger magnitude (factor
/// `MU`) — damps high-frequency positional noise (exactly the kind
/// Surface Nets' per-cell averaging can introduce) while the two passes'
/// opposite-signed shrink/inflate cancel out, unlike plain repeated
/// Laplacian smoothing, which visibly shrinks the mesh.
fn taubin_smooth(positions: &mut [Vec3], adjacency: &[Vec<u32>], iterations: u32) {
    const LAMBDA: f32 = 0.5;
    const MU: f32 = -0.53;
    let mut scratch = positions.to_vec();
    for _ in 0..iterations {
        for &factor in &[LAMBDA, MU] {
            for (v, neighbors) in adjacency.iter().enumerate() {
                scratch[v] = if neighbors.is_empty() {
                    positions[v]
                } else {
                    let avg = neighbors.iter().fold(Vec3::ZERO, |acc, &n| acc + positions[n as usize]) / neighbors.len() as f32;
                    positions[v] + (avg - positions[v]) * factor
                };
            }
            positions.copy_from_slice(&scratch);
        }
    }
}

/// Extracts the isosurface where `grid`'s field crosses `isovalue`
/// ("inside" is `value >= isovalue`). To get the *negative*-sign lobe of
/// a signed field (an orbital, not a density), call this on
/// `grid.negated()` instead of trying to pass a negative `isovalue`
/// directly — see `ScalarGrid::negated`'s doc comment for why.
pub fn extract_isosurface(grid: &ScalarGrid, isovalue: f32) -> IsosurfaceMesh {
    let [nx, ny, nz] = grid.dims;
    if nx < 2 || ny < 2 || nz < 2 {
        return IsosurfaceMesh { positions: Vec::new(), normals: Vec::new() };
    }
    let (cells_x, cells_y, cells_z) = (nx - 1, ny - 1, nz - 1);
    let inside = |i: usize, j: usize, k: usize| grid.value_at(i, j, k) >= isovalue;
    let cell_index = |i: usize, j: usize, k: usize| (i * cells_y + j) * cells_z + k;

    // Pass 1: one vertex per active cell, averaged from wherever the
    // isosurface crosses that cell's 12 edges — compacted into a dense
    // vertex list (not one slot per cell, most of which are inactive),
    // with `cell_to_vertex` mapping a cell back to its index in it.
    let mut vertex_positions: Vec<Vec3> = Vec::new();
    let mut cell_to_vertex: Vec<i32> = vec![-1; cells_x * cells_y * cells_z];
    for i in 0..cells_x {
        for j in 0..cells_y {
            for k in 0..cells_z {
                let corner_value = |di: i32, dj: i32, dk: i32| grid.value_at(i + di as usize, j + dj as usize, k + dk as usize);
                let corner_values = CUBE_CORNERS.map(|(di, dj, dk)| corner_value(di, dj, dk));
                let min = corner_values.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = corner_values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                if isovalue < min || isovalue > max {
                    continue; // whole cell on one side — inactive
                }

                let mut sum = Vec3::ZERO;
                let mut count = 0u32;
                for &((adi, adj, adk), (bdi, bdj, bdk)) in &CUBE_EDGES {
                    let va = corner_value(adi, adj, adk);
                    let vb = corner_value(bdi, bdj, bdk);
                    if (va >= isovalue) != (vb >= isovalue) {
                        let pa = grid.world_position((i as i32 + adi) as f32, (j as i32 + adj) as f32, (k as i32 + adk) as f32);
                        let pb = grid.world_position((i as i32 + bdi) as f32, (j as i32 + bdj) as f32, (k as i32 + bdk) as f32);
                        sum += edge_crossing(pa, va, pb, vb, isovalue);
                        count += 1;
                    }
                }
                if count > 0 {
                    cell_to_vertex[cell_index(i, j, k)] = vertex_positions.len() as i32;
                    vertex_positions.push(sum / count as f32);
                }
            }
        }
    }
    let get_cell_vertex = |ci: i32, cj: i32, ck: i32| -> Option<u32> {
        if ci < 0 || cj < 0 || ck < 0 || ci as usize >= cells_x || cj as usize >= cells_y || ck as usize >= cells_z {
            return None;
        }
        let idx = cell_to_vertex[cell_index(ci as usize, cj as usize, ck as usize)];
        (idx >= 0).then_some(idx as u32)
    };

    // Pass 2: a quad (as 2 triangle-index triples) for every sign-changing
    // *grid* edge, connecting the 4 cells that share it (skipped if any of
    // the 4 aren't active — happens only at the grid's own boundary,
    // where an isosurface shouldn't be sitting anyway for a well-sized
    // cube file).
    let mut indices: Vec<u32> = Vec::new();
    let mut push_quad = |a: Option<u32>, b: Option<u32>, c: Option<u32>, d: Option<u32>| {
        if let (Some(a), Some(b), Some(c), Some(d)) = (a, b, c, d) {
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    };

    // X-direction grid edges: the 4 cells surrounding edge (i,j,k)-(i+1,j,k)
    // sit in the (j,k) plane at cell columns j-1/j, k-1/k.
    for i in 0..cells_x {
        for j in 0..ny {
            for k in 0..nz {
                if inside(i, j, k) != inside(i + 1, j, k) {
                    let (ci, cj, ck) = (i as i32, j as i32, k as i32);
                    push_quad(
                        get_cell_vertex(ci, cj - 1, ck - 1),
                        get_cell_vertex(ci, cj, ck - 1),
                        get_cell_vertex(ci, cj, ck),
                        get_cell_vertex(ci, cj - 1, ck),
                    );
                }
            }
        }
    }
    // Y-direction grid edges: surrounding cells in the (i,k) plane.
    for i in 0..nx {
        for j in 0..cells_y {
            for k in 0..nz {
                if inside(i, j, k) != inside(i, j + 1, k) {
                    let (ci, cj, ck) = (i as i32, j as i32, k as i32);
                    push_quad(
                        get_cell_vertex(ci - 1, cj, ck - 1),
                        get_cell_vertex(ci, cj, ck - 1),
                        get_cell_vertex(ci, cj, ck),
                        get_cell_vertex(ci - 1, cj, ck),
                    );
                }
            }
        }
    }
    // Z-direction grid edges: surrounding cells in the (i,j) plane.
    for i in 0..nx {
        for j in 0..ny {
            for k in 0..cells_z {
                if inside(i, j, k) != inside(i, j, k + 1) {
                    let (ci, cj, ck) = (i as i32, j as i32, k as i32);
                    push_quad(
                        get_cell_vertex(ci - 1, cj - 1, ck),
                        get_cell_vertex(ci, cj - 1, ck),
                        get_cell_vertex(ci, cj, ck),
                        get_cell_vertex(ci - 1, cj, ck),
                    );
                }
            }
        }
    }

    if vertex_positions.is_empty() {
        return IsosurfaceMesh { positions: Vec::new(), normals: Vec::new() };
    }

    // Smooth the indexed mesh — this is *why* it's built as a proper
    // indexed mesh (shared vertices) rather than a triangle soup in the
    // first place: "average of my neighbors" needs real adjacency.
    const SMOOTHING_ITERATIONS: u32 = 8;
    let adjacency = build_adjacency(vertex_positions.len(), &indices);
    taubin_smooth(&mut vertex_positions, &adjacency, SMOOTHING_ITERATIONS);

    // Normals from a fresh gradient sample at each (now smoothed) vertex
    // position, converted back to fractional grid-index coordinates.
    let vertex_normals: Vec<Vec3> = vertex_positions
        .iter()
        .map(|&p| {
            let local = p - grid.origin;
            let fi = local.x / grid.steps[0].x;
            let fj = local.y / grid.steps[1].y;
            let fk = local.z / grid.steps[2].z;
            gradient_normal(grid, fi, fj, fk)
        })
        .collect();

    // Flatten back to a triangle soup for the public result — the
    // renderer draws isosurfaces as unshared triangles (see
    // `apost3dview_render::isosurface_mesh`), same as before switching to
    // an internally-indexed representation for smoothing.
    let mut positions = Vec::with_capacity(indices.len());
    let mut normals = Vec::with_capacity(indices.len());
    for &idx in &indices {
        positions.push(vertex_positions[idx as usize]);
        normals.push(vertex_normals[idx as usize]);
    }

    IsosurfaceMesh { positions, normals }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let grid = ScalarGrid { origin: Vec3::ZERO, dims: [1, 1, 1], steps: [Vec3::X, Vec3::Y, Vec3::Z], values: vec![0.0] };
        let mesh = extract_isosurface(&grid, 0.0);
        assert!(mesh.is_empty());
    }

    /// The whole point of Surface Nets over marching tetrahedra: vertex
    /// positions should be freely distributed within their cells (an
    /// averaged position), not snapped to a small fixed set of lattice
    /// edge directions.
    #[test]
    fn vertices_are_not_snapped_to_a_small_set_of_lattice_fractions() {
        let grid = negative_distance_grid(41, 5.0);
        let mesh = extract_isosurface(&grid, -3.0);
        assert!(!mesh.is_empty());

        let step = grid.steps[0].x;
        let mut distinct_fine_fractions = std::collections::HashSet::new();
        for &p in &mesh.positions {
            let local = (p - grid.origin) / step;
            let bucket = ((local.x.fract() * 16.0).round() as i32, (local.y.fract() * 16.0).round() as i32, (local.z.fract() * 16.0).round() as i32);
            distinct_fine_fractions.insert(bucket);
        }
        assert!(
            distinct_fine_fractions.len() > 20,
            "expected vertex positions to vary continuously within cells, only saw {} distinct fine-grained positions",
            distinct_fine_fractions.len()
        );
    }

    #[test]
    fn adjacency_is_symmetric_and_covers_every_triangle_edge() {
        // (a,b,c) and (c,d,a) — a quad's two triangles, sharing edge (a,c).
        let indices = [0u32, 1, 2, 0, 2, 3];
        let adjacency = build_adjacency(4, &indices);
        for (v, neighbors) in adjacency.iter().enumerate() {
            for &n in neighbors {
                assert!(adjacency[n as usize].contains(&(v as u32)), "adjacency should be symmetric: {v} lists {n} but not vice versa");
            }
        }
        // Vertex 0 touches both triangles: neighbors 1, 2, 3.
        assert_eq!(adjacency[0].len(), 3);
        // Vertex 2 also touches both triangles: neighbors 0, 1, 3.
        assert_eq!(adjacency[2].len(), 3);
        // Vertices 1 and 3 each only appear in one triangle: 2 neighbors.
        assert_eq!(adjacency[1].len(), 2);
        assert_eq!(adjacency[3].len(), 2);
    }

    #[test]
    fn taubin_smoothing_damps_a_local_perturbation_without_global_drift() {
        // A 9x9 flat grid (real isosurfaces are closed meshes with no
        // free boundary at all, so a small open sheet's corners — which
        // *do* inherently get pulled toward their 2-neighbor average by
        // any smoothing, boundary shrinkage being a normal, expected
        // property of free mesh edges, nothing to do with Taubin
        // specifically — aren't representative; using a grid large
        // enough that the boundary is far from the perturbation sidesteps
        // that entirely) with a spike on the center vertex.
        const N: i32 = 9;
        let idx = |x: i32, y: i32| -> Option<u32> {
            if (0..N).contains(&x) && (0..N).contains(&y) { Some((x * N + y) as u32) } else { None }
        };
        let mut positions: Vec<Vec3> =
            (0..N).flat_map(|x| (0..N).map(move |y| Vec3::new(x as f32, y as f32, 0.0))).collect();
        let center = idx(N / 2, N / 2).unwrap();
        positions[center as usize].z = 1.0;

        let mut adjacency = vec![Vec::new(); (N * N) as usize];
        for x in 0..N {
            for y in 0..N {
                let v = idx(x, y).unwrap();
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some(n) = idx(x + dx, y + dy) {
                        adjacency[v as usize].push(n);
                    }
                }
            }
        }

        let original_center_z = positions[center as usize].z;
        taubin_smooth(&mut positions, &adjacency, 8);
        assert!(
            positions[center as usize].z.abs() < original_center_z * 0.9,
            "smoothing should meaningfully reduce the perturbation, got {}",
            positions[center as usize].z
        );
        // A corner is 8 grid-steps away from the center in Manhattan
        // distance — 8 iterations (16 total alternating passes) of a
        // *local* averaging operation shouldn't have propagated the
        // center's Z spike out there. (Its X/Y position does shift a
        // little regardless of any perturbation — a corner only having 2
        // neighbors means even a perfectly flat, unperturbed grid pulls
        // it slightly toward them under any Laplacian-family smoothing.
        // That's expected free-boundary behavior, irrelevant to real
        // isosurfaces, which are always closed meshes with no boundary
        // at all — so this only checks the Z component, which is what
        // the injected perturbation actually affects.)
        let corner = idx(0, 0).unwrap();
        assert!(positions[corner as usize].z.abs() < 0.01, "the perturbation propagated further than it should have: {:?}", positions[corner as usize]);
    }
}
