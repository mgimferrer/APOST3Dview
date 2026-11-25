//! Changes to the atoms themselves: deleting some, changing an element.

use apost3dview_core::{Bond, Molecule};

/// A molecule after deleting atoms, with where every old atom and bond
/// went (`None`: deleted), so the app can carry its per-atom and per-bond
/// state (selection, hidden atoms, bond styles, measurements).
pub struct Deleted {
    pub molecule: Molecule,
    pub atom_map: Vec<Option<usize>>,
    pub bond_map: Vec<Option<usize>>,
}

/// `molecule` without the atoms in `atoms` (0-based) and their bonds.
pub fn delete_atoms(molecule: &Molecule, atoms: &[usize]) -> Deleted {
    let n = molecule.atomic_numbers.len();
    let mut gone = vec![false; n];
    for &a in atoms {
        if a < n {
            gone[a] = true;
        }
    }
    let mut atom_map = vec![None; n];
    let mut atomic_numbers = Vec::new();
    let mut positions = Vec::new();
    for i in 0..n {
        if !gone[i] {
            atom_map[i] = Some(atomic_numbers.len());
            atomic_numbers.push(molecule.atomic_numbers[i]);
            positions.push(molecule.positions[i]);
        }
    }
    let mut bond_map = vec![None; molecule.bonds.len()];
    let mut bonds = Vec::new();
    for (k, bond) in molecule.bonds.iter().enumerate() {
        if let (Some(a), Some(b)) = (atom_map.get(bond.atom_a).copied().flatten(), atom_map.get(bond.atom_b).copied().flatten()) {
            bond_map[k] = Some(bonds.len());
            bonds.push(Bond { atom_a: a, atom_b: b });
        }
    }
    Deleted { molecule: Molecule { atomic_numbers, positions, bonds }, atom_map, bond_map }
}

/// `molecule` with the atoms in `atoms` turned into element `atomic_number`
/// (positions and bonds unchanged).
pub fn change_element(molecule: &Molecule, atoms: &[usize], atomic_number: u32) -> Molecule {
    let mut changed = Molecule { atomic_numbers: molecule.atomic_numbers.clone(), positions: molecule.positions.clone(), bonds: molecule.bonds.clone() };
    for &a in atoms {
        if let Some(z) = changed.atomic_numbers.get_mut(a) {
            *z = atomic_number;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn water_dimer() -> Molecule {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.96, 0.0, 0.0),
            Vec3::new(-0.24, 0.93, 0.0),
            Vec3::new(2.9, 0.0, 0.0),
            Vec3::new(3.2, 0.9, 0.0),
            Vec3::new(3.2, -0.9, 0.0),
        ];
        Molecule::from_atoms(vec![8, 1, 1, 8, 1, 1], positions)
    }

    #[test]
    fn deleting_atoms_renumbers_atoms_and_bonds() {
        let m = water_dimer();
        assert_eq!(m.bonds.len(), 4);
        let d = delete_atoms(&m, &[1, 2]);
        assert_eq!(d.molecule.atomic_numbers, vec![8, 8, 1, 1]);
        assert_eq!(d.atom_map, vec![Some(0), None, None, Some(1), Some(2), Some(3)]);
        assert_eq!(d.molecule.bonds.len(), 2);
        assert!(d.molecule.bonds.iter().all(|b| b.atom_a == 1 || b.atom_b == 1));
        assert_eq!(d.bond_map.iter().filter(|b| b.is_some()).count(), 2);
        assert_eq!(d.molecule.positions[1], m.positions[3]);
    }

    #[test]
    fn changing_an_element_keeps_the_geometry() {
        let m = water_dimer();
        let c = change_element(&m, &[0, 3], 16);
        assert_eq!(c.atomic_numbers, vec![16, 1, 1, 16, 1, 1]);
        assert_eq!(c.positions, m.positions);
        assert_eq!(c.bonds.len(), m.bonds.len());
    }
}
