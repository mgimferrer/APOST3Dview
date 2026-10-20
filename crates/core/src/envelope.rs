//! Envelopes: smooth closed surfaces around a group of atoms (a fragment),
//! or a sphere around one atom, for showing a fragment's or an atom's value
//! on the molecule while the atoms keep their own colors.
//!
//! A fragment's envelope is the level set of a sum of Gaussians, one per
//! atom, sized so that an isolated atom's surface passes at its requested
//! radius; neighbouring atoms merge into one smooth surface, the way
//! bonded atoms should. Each atom only adds to the grid points within
//! three widths of it, and the grid planes are filled in parallel, so even
//! a large fragment takes milliseconds.

use glam::Vec3;
use rayon::prelude::*;

use crate::cube::ScalarGrid;
use crate::isosurface::{extract_isosurface, IsosurfaceMesh};

/// The field level the surface is drawn at.
const LEVEL: f32 = 0.35;
/// How far an atom's Gaussian reaches, in widths.
const REACH: f32 = 3.0;

/// The envelope around atoms at `centers`, whose surface passes at
/// `radii[i]` from an isolated atom `i` (angstrom). Normals point outward.
pub fn envelope_mesh(centers: &[Vec3], radii: &[f32]) -> IsosurfaceMesh {
    if centers.is_empty() {
        return IsosurfaceMesh { positions: Vec::new(), normals: Vec::new() };
    }
    // Gaussian width giving the requested radius at the drawing level.
    let widths: Vec<f32> = radii.iter().map(|r| r.max(0.05) / (1.0 / LEVEL).ln().sqrt()).collect();
    let smallest = radii.iter().copied().fold(f32::MAX, f32::min).max(0.05);
    let step = (smallest * 0.18).clamp(0.05, 0.16);

    let reach: Vec<f32> = widths.iter().map(|w| w * REACH).collect();
    let mut lo = centers[0];
    let mut hi = centers[0];
    for (c, r) in centers.iter().zip(&reach) {
        lo = lo.min(*c - Vec3::splat(*r));
        hi = hi.max(*c + Vec3::splat(*r));
    }
    lo -= Vec3::splat(step);
    hi += Vec3::splat(step);
    let dims = [
        ((hi.x - lo.x) / step).ceil() as usize + 1,
        ((hi.y - lo.y) / step).ceil() as usize + 1,
        ((hi.z - lo.z) / step).ceil() as usize + 1,
    ];
    let plane = dims[1] * dims[2];
    let mut values = vec![0.0f32; dims[0] * plane];

    values.par_chunks_mut(plane).enumerate().for_each(|(i, slab)| {
        let x = lo.x + i as f32 * step;
        for (atom, c) in centers.iter().enumerate() {
            let r = reach[atom];
            let dx = x - c.x;
            if dx.abs() > r {
                continue;
            }
            let inv_w2 = 1.0 / (widths[atom] * widths[atom]);
            let j0 = (((c.y - r - lo.y) / step).floor().max(0.0)) as usize;
            let j1 = ((((c.y + r - lo.y) / step).ceil()) as usize).min(dims[1] - 1);
            let k0 = (((c.z - r - lo.z) / step).floor().max(0.0)) as usize;
            let k1 = ((((c.z + r - lo.z) / step).ceil()) as usize).min(dims[2] - 1);
            for j in j0..=j1 {
                let dy = lo.y + j as f32 * step - c.y;
                let row = j * dims[2];
                for k in k0..=k1 {
                    let dz = lo.z + k as f32 * step - c.z;
                    let d2 = dx * dx + dy * dy + dz * dz;
                    if d2 < r * r {
                        slab[row + k] += (-d2 * inv_w2).exp();
                    }
                }
            }
        }
    });

    let grid = ScalarGrid::axis_aligned(lo, dims, step, values);
    extract_isosurface(&grid, LEVEL)
}

/// A unit sphere as triangles (an icosahedron subdivided three times:
/// 1280 triangles), positions equal to outward normals. Scale and move it
/// with `sphere_mesh`.
pub fn unit_sphere() -> IsosurfaceMesh {
    let t = (1.0 + 5.0f32.sqrt()) / 2.0;
    let mut vertices: Vec<Vec3> = [
        (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
        (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
        (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
    .collect();
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..3 {
        let mut midpoints = std::collections::HashMap::new();
        let mut midpoint = |a: usize, b: usize, vertices: &mut Vec<Vec3>| -> usize {
            *midpoints.entry((a.min(b), a.max(b))).or_insert_with(|| {
                vertices.push(((vertices[a] + vertices[b]) * 0.5).normalize());
                vertices.len() - 1
            })
        };
        let mut next = Vec::with_capacity(faces.len() * 4);
        for [a, b, c] in faces {
            let ab = midpoint(a, b, &mut vertices);
            let bc = midpoint(b, c, &mut vertices);
            let ca = midpoint(c, a, &mut vertices);
            next.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        faces = next;
    }
    let positions: Vec<Vec3> = faces.iter().flat_map(|f| f.iter().map(|&i| vertices[i])).collect();
    IsosurfaceMesh { normals: positions.clone(), positions }
}

/// `unit` scaled to `radius` and moved to `center`.
pub fn sphere_mesh(unit: &IsosurfaceMesh, center: Vec3, radius: f32) -> IsosurfaceMesh {
    IsosurfaceMesh { positions: unit.positions.iter().map(|p| center + *p * radius).collect(), normals: unit.normals.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_isolated_atom_gets_a_sphere_of_the_requested_radius() {
        let mesh = envelope_mesh(&[Vec3::new(1.0, 2.0, 3.0)], &[0.9]);
        assert!(!mesh.positions.is_empty());
        for p in &mesh.positions {
            let r = p.distance(Vec3::new(1.0, 2.0, 3.0));
            assert!((r - 0.9).abs() < 0.06, "radius {r}");
        }
    }

    #[test]
    fn bonded_atoms_merge_and_normals_point_outward() {
        let centers = [Vec3::ZERO, Vec3::new(1.16, 0.0, 0.0)];
        let mesh = envelope_mesh(&centers, &[0.9, 0.9]);
        // One merged surface: nothing crosses between the atoms at the bond
        // midpoint (the middle of the bond is inside).
        let mid = Vec3::new(0.58, 0.0, 0.0);
        assert!(mesh.positions.iter().all(|p| p.distance(mid) > 0.5));
        let outward = mesh
            .positions
            .iter()
            .zip(&mesh.normals)
            .filter(|(p, n)| {
                let nearest = if p.distance(centers[0]) < p.distance(centers[1]) { centers[0] } else { centers[1] };
                n.dot(**p - nearest) > 0.0
            })
            .count();
        assert!(outward as f32 > 0.98 * mesh.positions.len() as f32);
    }

    #[test]
    fn spheres_are_round_and_outward() {
        let unit = unit_sphere();
        assert_eq!(unit.positions.len(), 1280 * 3);
        let s = sphere_mesh(&unit, Vec3::splat(2.0), 0.5);
        for (p, n) in s.positions.iter().zip(&s.normals) {
            assert!((p.distance(Vec3::splat(2.0)) - 0.5).abs() < 1e-5);
            assert!(n.dot(*p - Vec3::splat(2.0)) > 0.0);
        }
    }
}
