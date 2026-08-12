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
            // Standard atan2-based torsion angle (Praxeolitic formula):
            // numerically stable near 0/180 degrees, unlike an acos of a
            // dot product between the two half-planes.
            let b1 = molecule.positions[b] - molecule.positions[a];
            let b2 = molecule.positions[c] - molecule.positions[b];
            let b3 = molecule.positions[d] - molecule.positions[c];

            let n1 = b1.cross(b2).normalize_or_zero();
            let n2 = b2.cross(b3).normalize_or_zero();
            let m1 = n1.cross(b2.normalize_or_zero());

            let x = n1.dot(n2);
            let y = m1.dot(n2);
            y.atan2(x).to_degrees()
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
}
