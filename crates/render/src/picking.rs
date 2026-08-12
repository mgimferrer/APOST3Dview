//! CPU-side ray picking against the molecule's atoms/bonds — the same
//! geometry already in memory for rendering, so no GPU readback needed.
//! Fine at these molecule sizes (a linear scan per pick, triggered only on
//! click, not per frame).

use std::collections::HashSet;

use glam::{Vec3, Vec4};

use apost3dview_core::{element_data, Molecule};

use crate::camera::OrbitCamera;

/// World-space ray through a normalized-device-coordinate point (each in
/// [-1, 1], Y-up) on the near/far planes of the current camera.
pub fn ray_from_ndc(camera: &OrbitCamera, aspect_ratio: f32, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
    let inverse_view_proj = camera.view_projection_matrix(aspect_ratio).inverse();
    let far = inverse_view_proj * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let far_world = Vec3::new(far.x, far.y, far.z) / far.w;

    let origin = camera.eye();
    let direction = (far_world - origin).normalize();
    (origin, direction)
}

fn ray_sphere_hit(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    let sqrt_disc = discriminant.sqrt();
    let mut t = -b - sqrt_disc;
    if t < 0.0 {
        t = -b + sqrt_disc;
    }
    (t >= 0.0).then_some(t)
}

/// Nearest atom hit by the ray. Hidden atoms are never pickable, matching
/// what's actually visible.
pub fn pick_atom(molecule: &Molecule, origin: Vec3, dir: Vec3, atom_scale: f32, hidden_atoms: &HashSet<usize>) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, (&z, &position)) in molecule.atomic_numbers.iter().zip(&molecule.positions).enumerate() {
        if hidden_atoms.contains(&index) {
            continue;
        }
        let radius = element_data(z).vdw_radius * atom_scale;
        if let Some(t) = ray_sphere_hit(origin, dir, position, radius) {
            if best.is_none_or(|(_, best_t)| t < best_t) {
                best = Some((index, t));
            }
        }
    }
    best.map(|(index, _)| index)
}

/// (t along the ray, squared perpendicular distance) to the nearest point
/// on a finite segment — used for bond picking.
fn ray_segment_closest(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let seg = b - a;
    let seg_len2 = seg.length_squared();
    if seg_len2 < 1e-10 {
        let t = (a - origin).dot(dir).max(0.0);
        return (t, (origin + dir * t - a).length_squared());
    }

    let w0 = origin - a;
    let a_dot = dir.dot(dir);
    let b_dot = dir.dot(seg);
    let c_dot = seg.dot(seg);
    let d_dot = dir.dot(w0);
    let e_dot = seg.dot(w0);
    let denom = a_dot * c_dot - b_dot * b_dot;

    // Closest point between two infinite lines (standard 2x2 linear
    // system in t, s — see e.g. Ericson, "Real-Time Collision Detection"
    // §5.1.9): both t and s are divided by the SAME `denom`, not `c_dot`
    // for s — that was the original bug here (verified against a known
    // point: projecting a bond's exact midpoint through the camera and
    // picking back should recover s=0.5 on that same bond, and it didn't
    // until this was fixed).
    //
    // When the ray and segment are nearly parallel, `denom` collapses and
    // the linear system has no unique solution — fall back to projecting
    // the segment's start point onto the ray for a sensible `t`, and the
    // segment's own midpoint parameter for `s`.
    let (t, s) = if denom.abs() > 1e-6 {
        let t = (b_dot * e_dot - c_dot * d_dot) / denom;
        let s = (a_dot * e_dot - b_dot * d_dot) / denom;
        (t, s)
    } else {
        (dir.dot(a - origin), 0.5)
    };
    let t = t.max(0.0);
    let s = s.clamp(0.0, 1.0);

    let closest_on_ray = origin + dir * t;
    let closest_on_segment = a + seg * s;
    (t, (closest_on_ray - closest_on_segment).length_squared())
}

/// Nearest bond hit by the ray, within a generous pick radius — bonds are
/// visually thin, so the clickable tolerance is padded beyond what's
/// actually drawn to keep them easy to click.
pub fn pick_bond(
    molecule: &Molecule,
    origin: Vec3,
    dir: Vec3,
    bond_radius: f32,
    hidden_atoms: &HashSet<usize>,
    hidden_bonds: &HashSet<usize>,
) -> Option<usize> {
    let pick_radius = bond_radius.max(0.12) * 2.0;
    let mut best: Option<(usize, f32)> = None;
    for (index, bond) in molecule.bonds.iter().enumerate() {
        if hidden_bonds.contains(&index) || hidden_atoms.contains(&bond.atom_a) || hidden_atoms.contains(&bond.atom_b) {
            continue;
        }
        let a = molecule.positions[bond.atom_a];
        let b = molecule.positions[bond.atom_b];
        let (t, dist2) = ray_segment_closest(origin, dir, a, b);
        if dist2 <= pick_radius * pick_radius && best.is_none_or(|(_, best_t)| t < best_t) {
            best = Some((index, t));
        }
    }
    best.map(|(index, _)| index)
}
