//! Distance/angle/dihedral geometry — pure math over a `Molecule`'s
//! positions, no rendering or UI concerns.

use crate::molecule::Molecule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementKind {
    Distance(usize, usize),
    Angle(usize, usize, usize),
    Dihedral(usize, usize, usize, usize),
}

impl MeasurementKind {
    /// Builds the right kind from an ordered pick list — 2 atoms for a
    /// distance, 3 for an angle (vertex = the 2nd pick), 4 for a dihedral.
    pub fn from_picks(picks: &[usize]) -> Option<Self> {
        match picks {
            [a, b] => Some(Self::Distance(*a, *b)),
            [a, b, c] => Some(Self::Angle(*a, *b, *c)),
            [a, b, c, d] => Some(Self::Dihedral(*a, *b, *c, *d)),
            _ => None,
        }
    }

    /// Atom pairs to draw a dashed line along — one segment for a
    /// distance, two for an angle's arms, three for a dihedral's path.
    pub fn segments(&self) -> Vec<(usize, usize)> {
        match *self {
            MeasurementKind::Distance(a, b) => vec![(a, b)],
            MeasurementKind::Angle(a, b, c) => vec![(a, b), (b, c)],
            MeasurementKind::Dihedral(a, b, c, d) => vec![(a, b), (b, c), (c, d)],
        }
    }

    pub fn atoms(&self) -> Vec<usize> {
        match *self {
            MeasurementKind::Distance(a, b) => vec![a, b],
            MeasurementKind::Angle(a, b, c) => vec![a, b, c],
            MeasurementKind::Dihedral(a, b, c, d) => vec![a, b, c, d],
        }
    }
}

/// Distance in angstrom, or angle/dihedral in degrees.
pub fn measure(molecule: &Molecule, kind: MeasurementKind) -> f32 {
    match kind {
        MeasurementKind::Distance(a, b) => molecule.positions[a].distance(molecule.positions[b]),
        MeasurementKind::Angle(a, b, c) => {
            let v1 = (molecule.positions[a] - molecule.positions[b]).normalize_or_zero();
            let v2 = (molecule.positions[c] - molecule.positions[b]).normalize_or_zero();
            v1.dot(v2).clamp(-1.0, 1.0).acos().to_degrees()
        }
        MeasurementKind::Dihedral(a, b, c, d) => {
            // atan2 form (stable near 0 and 180 degrees) with the IUPAC
            // sign: positive when, looking from b to c, the bond a-b turns
            // clockwise to cover c-d (as Gaussian, ORCA and Chemcraft).
            // Computed in f64, as the Build window's edits.
            let p = |i: usize| molecule.positions[i].as_dvec3();
            let b1 = p(b) - p(a);
            let b2 = p(c) - p(b);
            let b3 = p(d) - p(c);
            let y = b2.length() * b1.dot(b2.cross(b3));
            let x = b1.cross(b2).dot(b2.cross(b3));
            y.atan2(x).to_degrees() as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn molecule(positions: &[Vec3]) -> Molecule {
        Molecule::from_atoms(vec![6; positions.len()], positions.to_vec())
    }

    #[test]
    fn distance_matches_known_3_4_5_triangle() {
        let m = molecule(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 0.0)]);
        assert!((measure(&m, MeasurementKind::Distance(0, 1)) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn angle_matches_known_right_angle() {
        let m = molecule(&[Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)]);
        assert!((measure(&m, MeasurementKind::Angle(0, 1, 2)) - 90.0).abs() < 1e-3);
    }

    #[test]
    fn dihedral_matches_known_trans_and_cis_arrangements() {
        let trans = molecule(&[
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
        ]);
        let angle = measure(&trans, MeasurementKind::Dihedral(0, 1, 2, 3)).abs();
        assert!((angle - 180.0).abs() < 1e-3, "expected ~180, got {angle}");

        let cis = molecule(&[
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ]);
        let angle = measure(&cis, MeasurementKind::Dihedral(0, 1, 2, 3)).abs();
        assert!(angle < 1e-3, "expected ~0, got {angle}");
    }

    #[test]
    fn dihedral_sign_follows_iupac() {
        // a up (+y), b at the origin, c along +z, d at -x: seen from b
        // toward c, +x lies left of +y, so d at -x is a clockwise quarter
        // turn from a: +90. With d at +x: -90.
        let clockwise = molecule(&[Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), Vec3::new(-1.0, 0.0, 1.0)]);
        let angle = measure(&clockwise, MeasurementKind::Dihedral(0, 1, 2, 3));
        assert!((angle - 90.0).abs() < 1e-4, "expected +90, got {angle}");
        let counter = molecule(&[Vec3::new(0.0, 1.0, 0.0), Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 1.0)]);
        let angle = measure(&counter, MeasurementKind::Dihedral(0, 1, 2, 3));
        assert!((angle + 90.0).abs() < 1e-4, "expected -90, got {angle}");
    }
}
