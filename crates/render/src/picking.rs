//! CPU-side ray picking against the molecule's atoms/bonds — the same
//! geometry already in memory for rendering, so no GPU readback needed.
//! Fine at these molecule sizes (a linear scan per pick, triggered only on
//! click, not per frame).

use std::collections::HashSet;

use glam::{Vec3, Vec4};

use apost3dview_core::Molecule;

use crate::camera::OrbitCamera;
use crate::style::DrawnSizes;

/// World-space ray through a normalized-device-coordinate point (each in
/// [-1, 1], Y-up) on the near/far planes of the current camera.
pub fn ray_from_ndc(camera: &OrbitCamera, aspect_ratio: f32, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
    let inverse_view_proj = camera.view_projection_matrix(aspect_ratio).inverse();
    let far = inverse_view_proj * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let far_world = Vec3::new(far.x, far.y, far.z) / far.w;
    if camera.orthographic {
        // Parallel rays: from the near plane, along the view direction.
        let near = inverse_view_proj * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let origin = Vec3::new(near.x, near.y, near.z) / near.w;
        return (origin, (far_world - origin).normalize());
    }

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
pub fn pick_atom(molecule: &Molecule, origin: Vec3, dir: Vec3, sizes: &DrawnSizes, hidden_atoms: &HashSet<usize>) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, &position) in molecule.positions.iter().enumerate() {
        if hidden_atoms.contains(&index) {
            continue;
        }
        let radius = sizes.radius(index);
        if let Some(t) = ray_sphere_hit(origin, dir, position, radius)
            && best.is_none_or(|(_, best_t)| t < best_t)
        {
            best = Some((index, t));
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
/// actually drawn to keep them easy to click. The caller skips it when
/// bonds aren't drawn (space-filling).
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

/// Whether an atom's center is unoccluded from the camera — nothing else
/// (another atom or a bond not attached to it) sits strictly closer along
/// the ray to it. Used to decide whether to draw that atom's label.
///
/// This is a binary visibility test on the center point, not true
/// per-pixel occlusion — a bond grazing the edge of an atom's disc will
/// still let its whole label through rather than clipping part of it.
/// Real per-pixel coverage would mean rendering the label as actual
/// depth-tested 3D geometry rather than a 2D overlay; this is the cheaper
/// approximation, and it's the one that matters for the common case (a
/// label floating over an unrelated bond because its atom is hidden
/// behind a cluster of other atoms).
pub fn is_atom_visible(
    molecule: &Molecule,
    camera_eye: Vec3,
    atom_index: usize,
    sizes: &DrawnSizes,
    hidden_atoms: &HashSet<usize>,
    hidden_bonds: &HashSet<usize>,
) -> bool {
    let position = molecule.positions[atom_index];
    if (position - camera_eye).length() < 1e-5 {
        return true;
    }
    ray_reaches_atom(molecule, camera_eye, atom_index, position, sizes, hidden_atoms, hidden_bonds)
}

/// How much of an atom can be seen from the camera, roughly: the share of
/// nine rays (to its center and to eight points around it, at 0.6 of its
/// radius across the line of sight) that reach it before anything else.
/// An atom whose center is just covered by a small neighbor (a hydrogen in
/// front of a carbon) still counts as mostly visible.
pub fn atom_visible_fraction(
    molecule: &Molecule,
    camera_eye: Vec3,
    atom_index: usize,
    sizes: &DrawnSizes,
    hidden_atoms: &HashSet<usize>,
    hidden_bonds: &HashSet<usize>,
) -> f32 {
    let position = molecule.positions[atom_index];
    let view = (position - camera_eye).normalize_or_zero();
    if view == Vec3::ZERO {
        return 1.0;
    }
    let side = view.any_orthonormal_vector();
    let other = view.cross(side);
    let reach = sizes.radius(atom_index) * 0.6;
    let targets = std::iter::once(position).chain((0..8).map(|k| {
        let angle = k as f32 * std::f32::consts::FRAC_PI_4;
        position + (side * angle.cos() + other * angle.sin()) * reach
    }));
    let reached = targets.filter(|&t| ray_reaches_atom(molecule, camera_eye, atom_index, t, sizes, hidden_atoms, hidden_bonds)).count();
    reached as f32 / 9.0
}

/// Whether the ray from the camera toward `target` (a point on or in atom
/// `atom_index`) meets that atom before any other atom or bond (one not
/// attached to it).
fn ray_reaches_atom(
    molecule: &Molecule,
    camera_eye: Vec3,
    atom_index: usize,
    target: Vec3,
    sizes: &DrawnSizes,
    hidden_atoms: &HashSet<usize>,
    hidden_bonds: &HashSet<usize>,
) -> bool {
    let to_target = target - camera_eye;
    let distance = to_target.length();
    if distance < 1e-5 {
        return true;
    }
    let dir = to_target / distance;
    let own_radius = sizes.radius(atom_index);
    // Where the ray enters this atom (for the center ray, its distance
    // minus its radius).
    let Some(own_surface_t) = ray_sphere_hit(camera_eye, dir, molecule.positions[atom_index], own_radius) else { return false };
    // A little slack so a bond/atom that just grazes this atom's own
    // surface (as anything actually touching it geometrically will)
    // doesn't register as a false occluder.
    let epsilon = own_radius * 0.15 + 0.02;

    for (index, &other_position) in molecule.positions.iter().enumerate() {
        if index == atom_index || hidden_atoms.contains(&index) {
            continue;
        }
        let other_radius = sizes.radius(index);
        if let Some(t) = ray_sphere_hit(camera_eye, dir, other_position, other_radius)
            && t < own_surface_t - epsilon
        {
            return false;
        }
    }

    let Some(bond_radius) = sizes.bond_radius else { return true };
    for (bond_index, bond) in molecule.bonds.iter().enumerate() {
        if hidden_bonds.contains(&bond_index) || bond.atom_a == atom_index || bond.atom_b == atom_index {
            continue;
        }
        if hidden_atoms.contains(&bond.atom_a) || hidden_atoms.contains(&bond.atom_b) {
            continue;
        }
        let a = molecule.positions[bond.atom_a];
        let b = molecule.positions[bond.atom_b];
        let (t, dist2) = ray_segment_closest(camera_eye, dir, a, b);
        if dist2 <= bond_radius * bond_radius && t < own_surface_t - epsilon {
            return false;
        }
    }

    true
}
