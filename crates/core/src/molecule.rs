use std::io;
use std::path::Path;

use glam::Vec3;

use crate::element::element_data;
use crate::fchk::parse_fchk_geometry;

#[derive(Clone)]
pub struct Bond {
    pub atom_a: usize,
    pub atom_b: usize,
}

#[derive(Clone)]
pub struct Molecule {
    pub atomic_numbers: Vec<u32>,
    /// Angstrom.
    pub positions: Vec<Vec3>,
    pub bonds: Vec<Bond>,
}

/// Multiplier on the sum of covalent radii for bond perception. Standard
/// value used by most viewers (VMD, Chemcraft, ...) to tolerate slightly
/// stretched bonds without merging non-bonded contacts.
const BOND_TOLERANCE: f32 = 1.2;

fn perceive_bonds(atomic_numbers: &[u32], positions: &[Vec3]) -> Vec<Bond> {
    let mut bonds = Vec::new();
    for i in 0..positions.len() {
        let radius_i = element_data(atomic_numbers[i]).covalent_radius;
        for j in (i + 1)..positions.len() {
            let radius_j = element_data(atomic_numbers[j]).covalent_radius;
            let cutoff = (radius_i + radius_j) * BOND_TOLERANCE;
            let distance = positions[i].distance(positions[j]);
            if distance > 0.1 && distance < cutoff {
                bonds.push(Bond { atom_a: i, atom_b: j });
            }
        }
    }
    bonds
}

impl Molecule {
    pub fn from_atoms(atomic_numbers: Vec<u32>, positions: Vec<Vec3>) -> Self {
        let bonds = perceive_bonds(&atomic_numbers, &positions);
        Self { atomic_numbers, positions, bonds }
    }

    pub fn from_fchk(path: &Path) -> io::Result<Self> {
        let geometry = parse_fchk_geometry(path)?;
        let positions: Vec<Vec3> = geometry.positions.iter().map(|&p| Vec3::from(p)).collect();
        Ok(Self::from_atoms(geometry.atomic_numbers, positions))
    }

    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        if self.positions.is_empty() {
            return (Vec3::ZERO, 1.0);
        }
        let center = self.positions.iter().fold(Vec3::ZERO, |acc, &p| acc + p) / self.positions.len() as f32;
        let radius = self
            .positions
            .iter()
            .map(|&p| p.distance(center))
            .fold(0.0_f32, f32::max)
            .max(1.0);
        (center, radius)
    }
}
