//! Setting a distance, angle or dihedral, turning a group about a bond,
//! and moving atoms by a vector. Every edit returns new positions and
//! moves only the atoms it is given (rigidly), computed in `f64`.

use glam::{DQuat, DVec3, Vec3};

use crate::sides::{moving_atoms, Graph, Which};

/// What moves when a value is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Moves {
    /// The last atom picked and everything on its side (the default).
    #[default]
    Side,
    /// The last atom picked alone.
    Alone,
    /// Both ends with their sides, each by half the change.
    Halves,
}

/// Why an edit could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The sides can't be separated (a ring, or the side holds the other
    /// end): only the atom alone can move.
    NoSide,
    /// The value is not defined for these positions (atoms on top of each
    /// other, three in a line for a dihedral).
    Undefined(&'static str),
    /// Atoms missing or repeated, or out of range.
    BadAtoms,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::NoSide => write!(f, "these atoms are in one ring (or joined another way), so their sides can't move apart"),
            EditError::Undefined(why) => write!(f, "{why}"),
            EditError::BadAtoms => write!(f, "pick different atoms"),
        }
    }
}

fn d(v: Vec3) -> DVec3 {
    v.as_dvec3()
}

/// Distance between two positions, angstrom.
pub fn distance(positions: &[Vec3], a: usize, b: usize) -> f64 {
    d(positions[a]).distance(d(positions[b]))
}

/// Angle a-b-c, degrees (0 to 180).
pub fn angle_degrees(positions: &[Vec3], a: usize, b: usize, c: usize) -> f64 {
    let u = d(positions[a]) - d(positions[b]);
    let v = d(positions[c]) - d(positions[b]);
    u.angle_between(v).to_degrees()
}

/// Dihedral a-b-c-d, degrees (-180 to 180), with the IUPAC sign: positive
/// when, looking from b to c, the bond a-b turns clockwise (by less than
/// 180 degrees) to cover c-d.
pub fn dihedral_degrees(positions: &[Vec3], a: usize, b: usize, c: usize, d_: usize) -> f64 {
    let (pa, pb, pc, pd) = (d(positions[a]), d(positions[b]), d(positions[c]), d(positions[d_]));
    let b1 = pb - pa;
    let b2 = pc - pb;
    let b3 = pd - pc;
    let y = b2.length() * b1.dot(b2.cross(b3));
    let x = b1.cross(b2).dot(b2.cross(b3));
    y.atan2(x).to_degrees()
}

/// The atoms that move for `moves` on the measurement `atoms`, last end
/// first, and the other end's (only for `Moves::Halves`).
fn movers(graph: &Graph, atoms: &[usize], moves: Moves) -> Result<(Vec<usize>, Vec<usize>), EditError> {
    let mut sorted = atoms.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != atoms.len() || !(2..=4).contains(&atoms.len()) {
        return Err(EditError::BadAtoms);
    }
    match moves {
        Moves::Alone => Ok((vec![*atoms.last().unwrap()], Vec::new())),
        Moves::Side => Ok((moving_atoms(graph, atoms, Which::Last).ok_or(EditError::NoSide)?, Vec::new())),
        Moves::Halves => {
            let last = moving_atoms(graph, atoms, Which::Last).ok_or(EditError::NoSide)?;
            let first = moving_atoms(graph, atoms, Which::First).ok_or(EditError::NoSide)?;
            if first.iter().any(|a| last.binary_search(a).is_ok()) {
                return Err(EditError::NoSide);
            }
            Ok((last, first))
        }
    }
}

fn in_range(positions: &[Vec3], atoms: &[usize]) -> Result<(), EditError> {
    if atoms.iter().all(|&a| a < positions.len()) { Ok(()) } else { Err(EditError::BadAtoms) }
}

/// `atoms` moved by `shift`.
fn shifted(positions: &mut [DVec3], atoms: &[usize], shift: DVec3) {
    for &i in atoms {
        positions[i] += shift;
    }
}

/// `atoms` turned by `angle` radians about the axis through `pivot` along
/// `axis` (right hand).
fn turned(positions: &mut [DVec3], atoms: &[usize], pivot: DVec3, axis: DVec3, angle: f64) {
    let rotation = DQuat::from_axis_angle(axis.normalize(), angle);
    for &i in atoms {
        positions[i] = pivot + rotation * (positions[i] - pivot);
    }
}

fn finish(positions: Vec<DVec3>) -> Vec<Vec3> {
    positions.into_iter().map(|p| p.as_vec3()).collect()
}

/// The positions with the distance a-b set to `target` angstrom.
pub fn set_distance(positions: &[Vec3], graph: &Graph, a: usize, b: usize, target: f64, moves: Moves) -> Result<Vec<Vec3>, EditError> {
    in_range(positions, &[a, b])?;
    if target <= 0.0 {
        return Err(EditError::Undefined("a distance must be positive"));
    }
    let (last, first) = movers(graph, &[a, b], moves)?;
    let mut p: Vec<DVec3> = positions.iter().map(|&v| d(v)).collect();
    let axis = p[b] - p[a];
    let current = axis.length();
    if current < 1e-6 {
        return Err(EditError::Undefined("the two atoms are on top of each other"));
    }
    let direction = axis / current;
    let change = target - current;
    if moves == Moves::Halves {
        shifted(&mut p, &last, direction * change / 2.0);
        shifted(&mut p, &first, -direction * change / 2.0);
    } else {
        shifted(&mut p, &last, direction * change);
    }
    Ok(finish(p))
}

/// A unit vector perpendicular to `v` (for a straight angle, where any
/// perpendicular axis turns it the same way).
fn perpendicular(v: DVec3) -> DVec3 {
    let other = if v.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    v.cross(other).normalize()
}

/// The positions with the angle a-b-c set to `target` degrees (0 to 180).
pub fn set_angle(positions: &[Vec3], graph: &Graph, a: usize, b: usize, c: usize, target: f64, moves: Moves) -> Result<Vec<Vec3>, EditError> {
    in_range(positions, &[a, b, c])?;
    if !(0.0..=180.0).contains(&target) {
        return Err(EditError::Undefined("an angle is between 0 and 180 degrees"));
    }
    let (last, first) = movers(graph, &[a, b, c], moves)?;
    let mut p: Vec<DVec3> = positions.iter().map(|&v| d(v)).collect();
    let u = p[a] - p[b];
    let v = p[c] - p[b];
    if u.length() < 1e-6 || v.length() < 1e-6 {
        return Err(EditError::Undefined("two of the atoms are on top of each other"));
    }
    let normal = u.cross(v);
    // Turning c about u x v opens the angle.
    let axis = if normal.length() > 1e-9 * u.length() * v.length() { normal.normalize() } else { perpendicular(v) };
    let change = (target - u.angle_between(v).to_degrees()).to_radians();
    let pivot = p[b];
    if moves == Moves::Halves {
        turned(&mut p, &last, pivot, axis, change / 2.0);
        turned(&mut p, &first, pivot, axis, -change / 2.0);
    } else {
        turned(&mut p, &last, pivot, axis, change);
    }
    Ok(finish(p))
}

/// The positions with the dihedral a-b-c-d set to `target` degrees.
pub fn set_dihedral(positions: &[Vec3], graph: &Graph, atoms: [usize; 4], target: f64, moves: Moves) -> Result<Vec<Vec3>, EditError> {
    let [a, b, c, d_] = atoms;
    in_range(positions, &atoms)?;
    let (last, first) = movers(graph, &atoms, moves)?;
    let mut p: Vec<DVec3> = positions.iter().map(|&v| d(v)).collect();
    let axis = p[c] - p[b];
    let line = |x: DVec3, y: DVec3, z: DVec3| (y - x).cross(z - y).length() < 1e-9 * (y - x).length().max(1e-9) * (z - y).length().max(1e-9);
    if axis.length() < 1e-6 || line(p[a], p[b], p[c]) || line(p[b], p[c], p[d_]) {
        return Err(EditError::Undefined("three of the atoms are in a straight line, so the dihedral is not defined"));
    }
    let current = dihedral_degrees(positions, a, b, c, d_);
    let mut change = target - current;
    change -= 360.0 * (change / 360.0).round();
    // Turning d about b->c (right hand) raises the dihedral.
    let change = change.to_radians();
    let pivot = p[b];
    if moves == Moves::Halves {
        turned(&mut p, &last, pivot, axis, change / 2.0);
        turned(&mut p, &first, pivot, axis, -change / 2.0);
    } else {
        turned(&mut p, &last, pivot, axis, change);
    }
    Ok(finish(p))
}

/// The positions with the side of b (cut at the bond a-b) turned by
/// `degrees` about the axis a->b (right hand); with `Moves::Halves`, each
/// side by half, the other way. `Moves::Alone` turns nothing (an atom on
/// the axis), so it is refused.
pub fn rotate_about_bond(positions: &[Vec3], graph: &Graph, a: usize, b: usize, degrees: f64, moves: Moves) -> Result<Vec<Vec3>, EditError> {
    in_range(positions, &[a, b])?;
    if moves == Moves::Alone {
        return Err(EditError::Undefined("turning one atom about its own bond moves nothing"));
    }
    let (last, first) = movers(graph, &[a, b], moves)?;
    let mut p: Vec<DVec3> = positions.iter().map(|&v| d(v)).collect();
    let axis = p[b] - p[a];
    if axis.length() < 1e-6 {
        return Err(EditError::Undefined("the two atoms are on top of each other"));
    }
    let angle = degrees.to_radians();
    let pivot = p[b];
    if moves == Moves::Halves {
        turned(&mut p, &last, pivot, axis, angle / 2.0);
        turned(&mut p, &first, pivot, axis, -angle / 2.0);
    } else {
        turned(&mut p, &last, pivot, axis, angle);
    }
    Ok(finish(p))
}

/// The positions with `atoms` moved by `shift` angstrom.
pub fn translate(positions: &[Vec3], atoms: &[usize], shift: [f64; 3]) -> Result<Vec<Vec3>, EditError> {
    in_range(positions, atoms)?;
    let mut p: Vec<DVec3> = positions.iter().map(|&v| d(v)).collect();
    let mut unique = atoms.to_vec();
    unique.sort_unstable();
    unique.dedup();
    shifted(&mut p, &unique, DVec3::from_array(shift));
    Ok(finish(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// n-butane, staggered, with one hydrogen on each end carbon:
    /// H4-C0-C1-C2-C3-H5 (no other hydrogens: enough to test sides).
    fn butane() -> (Vec<Vec3>, Graph) {
        let positions = vec![
            Vec3::new(-1.92, 0.51, 0.0),
            Vec3::new(-0.76, -0.47, 0.0),
            Vec3::new(0.76, 0.47, 0.0),
            Vec3::new(1.92, -0.51, 0.0),
            Vec3::new(-2.88, -0.02, 0.0),
            Vec3::new(2.70, 0.10, 0.75),
        ];
        let graph = Graph::new(6, [(0, 1), (1, 2), (2, 3), (0, 4), (3, 5)]);
        (positions, graph)
    }

    /// Every pairwise distance within `atoms` is the same before and after
    /// (a rigid motion of the group).
    fn rigid(before: &[Vec3], after: &[Vec3], atoms: &[usize]) -> bool {
        atoms.iter().all(|&i| atoms.iter().all(|&j| (before[i].distance(before[j]) - after[i].distance(after[j])).abs() < 2e-5))
    }

    fn unmoved(before: &[Vec3], after: &[Vec3], atoms: &[usize]) -> bool {
        atoms.iter().all(|&i| before[i] == after[i])
    }

    #[test]
    fn distances_are_set_moving_the_side_chosen() {
        let (p, g) = butane();
        let q = set_distance(&p, &g, 1, 2, 2.0, Moves::Side).unwrap();
        assert!((distance(&q, 1, 2) - 2.0).abs() < 1e-5);
        assert!(unmoved(&p, &q, &[0, 1, 4]));
        assert!(rigid(&p, &q, &[2, 3, 5]));
        let q = set_distance(&p, &g, 1, 2, 2.0, Moves::Alone).unwrap();
        assert!(unmoved(&p, &q, &[0, 1, 3, 4, 5]));
        let q = set_distance(&p, &g, 1, 2, 2.0, Moves::Halves).unwrap();
        assert!((distance(&q, 1, 2) - 2.0).abs() < 1e-5);
        // Each side moved by the same amount, the other way.
        let moved_last = q[2].distance(p[2]);
        let moved_first = q[1].distance(p[1]);
        assert!((moved_last - moved_first).abs() < 1e-5);
        assert!(rigid(&p, &q, &[2, 3, 5]) && rigid(&p, &q, &[0, 1, 4]));
    }

    #[test]
    fn angles_are_set_and_straightened() {
        let (p, g) = butane();
        for target in [60.0, 109.47, 150.0, 180.0] {
            for moves in [Moves::Side, Moves::Alone, Moves::Halves] {
                let q = set_angle(&p, &g, 0, 1, 2, target, moves).unwrap();
                assert!((angle_degrees(&q, 0, 1, 2) - target).abs() < 1e-3, "{target} {moves:?}: {}", angle_degrees(&q, 0, 1, 2));
                // Bond lengths at the vertex are kept.
                assert!((distance(&q, 0, 1) - distance(&p, 0, 1)).abs() < 1e-5);
                assert!((distance(&q, 1, 2) - distance(&p, 1, 2)).abs() < 1e-5);
            }
        }
        let q = set_angle(&p, &g, 0, 1, 2, 180.0, Moves::Side).unwrap();
        // From a straight angle back to a bent one.
        let r = set_angle(&q, &g, 0, 1, 2, 120.0, Moves::Side).unwrap();
        assert!((angle_degrees(&r, 0, 1, 2) - 120.0).abs() < 1e-3);
        let q = set_angle(&p, &g, 0, 1, 2, 120.0, Moves::Side).unwrap();
        assert!(unmoved(&p, &q, &[0, 1, 4]) && rigid(&p, &q, &[2, 3, 5]));
    }

    #[test]
    fn dihedrals_turn_the_whole_group_behind_the_middle_bond() {
        let (p, g) = butane();
        for target in [-179.0, -120.0, -60.0, 0.0, 60.0, 120.0, 180.0] {
            for moves in [Moves::Side, Moves::Alone, Moves::Halves] {
                let q = set_dihedral(&p, &g, [0, 1, 2, 3], target, moves).unwrap();
                let got = dihedral_degrees(&q, 0, 1, 2, 3);
                let mut diff = got - target;
                diff -= 360.0 * (diff / 360.0).round();
                assert!(diff.abs() < 1e-3, "{target} {moves:?}: got {got}");
            }
        }
        let q = set_dihedral(&p, &g, [0, 1, 2, 3], 60.0, Moves::Side).unwrap();
        assert!(unmoved(&p, &q, &[0, 1, 4]));
        // The group behind C2 turned rigidly, the middle bond untouched.
        assert!(rigid(&p, &q, &[2, 3, 5]));
        assert!(rigid(&p, &q, &[1, 2, 3, 5]));
        let q = set_dihedral(&p, &g, [0, 1, 2, 3], 60.0, Moves::Halves).unwrap();
        assert!(rigid(&p, &q, &[0, 1, 4]) && rigid(&p, &q, &[2, 3, 5]));
    }

    #[test]
    fn the_dihedral_sign_follows_iupac() {
        // Looking from b to c, positive when a-b turns clockwise to c-d.
        // Here a is up (+y) and d at +x; seen from b toward c (along +z),
        // +x lies to the left of +y, a counterclockwise quarter turn: -90.
        let p = vec![Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 1.0)];
        assert!((dihedral_degrees(&p, 0, 1, 2, 3) + 90.0).abs() < 1e-9);
        // Mirrored (d at -x): +90.
        let q = vec![p[0], p[1], p[2], Vec3::new(-1.0, 0.0, 1.0)];
        assert!((dihedral_degrees(&q, 0, 1, 2, 3) - 90.0).abs() < 1e-9);
    }

    #[test]
    fn rings_refuse_sides_but_not_a_single_atom() {
        // Square ring 0-1-2-3.
        let p = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.5, 0.0, 0.0), Vec3::new(1.5, 1.5, 0.0), Vec3::new(0.0, 1.5, 0.0)];
        let g = Graph::new(4, [(0, 1), (1, 2), (2, 3), (3, 0)]);
        assert_eq!(set_distance(&p, &g, 0, 1, 1.4, Moves::Side), Err(EditError::NoSide));
        assert_eq!(set_distance(&p, &g, 0, 1, 1.4, Moves::Halves), Err(EditError::NoSide));
        let q = set_distance(&p, &g, 0, 1, 1.4, Moves::Alone).unwrap();
        assert!((distance(&q, 0, 1) - 1.4).abs() < 1e-5);
    }

    #[test]
    fn turning_about_a_bond_and_moving_by_a_vector() {
        let (p, g) = butane();
        let q = rotate_about_bond(&p, &g, 1, 2, 120.0, Moves::Side).unwrap();
        let mut change = dihedral_degrees(&q, 0, 1, 2, 3) - dihedral_degrees(&p, 0, 1, 2, 3);
        change -= 360.0 * (change / 360.0).round();
        assert!((change.abs() - 120.0).abs() < 1e-3);
        assert!(unmoved(&p, &q, &[0, 1, 4]) && rigid(&p, &q, &[1, 2, 3, 5]));
        assert!(rotate_about_bond(&p, &g, 1, 2, 120.0, Moves::Alone).is_err());
        let q = translate(&p, &[3, 5], [1.0, -2.0, 0.5]).unwrap();
        assert!((q[3] - p[3] - Vec3::new(1.0, -2.0, 0.5)).length() < 1e-6);
        assert!(unmoved(&p, &q, &[0, 1, 2, 4]));
    }

    #[test]
    fn undefined_values_are_refused() {
        let p = vec![Vec3::ZERO, Vec3::X, Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 0.0)];
        let g = Graph::new(4, [(0, 1), (1, 2), (2, 3)]);
        assert!(matches!(set_dihedral(&p, &g, [0, 1, 2, 3], 60.0, Moves::Side), Err(EditError::Undefined(_))));
        assert_eq!(set_distance(&p, &g, 0, 0, 1.0, Moves::Side), Err(EditError::BadAtoms));
        assert!(set_distance(&p, &g, 0, 1, -1.0, Moves::Side).is_err());
    }
}
