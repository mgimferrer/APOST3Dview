use bytemuck::{Pod, Zeroable};

use apost3dview_core::{element_data, Molecule};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct AtomInstance {
    pub center: [f32; 3],
    pub vdw_radius: f32,
    pub color: [f32; 3],
    pub _padding: f32,
}

/// One half of a bond's cylinder — split at the midpoint so each half can
/// take its nearer atom's CPK color (matches VMD's default CPK-coloring
/// convention for bonds).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct BondInstance {
    pub center: [f32; 3],
    pub length: f32,
    pub axis: [f32; 3],
    pub _padding: f32,
    pub color: [f32; 3],
    pub _padding2: f32,
}

pub fn build_atom_instances(molecule: &Molecule) -> Vec<AtomInstance> {
    molecule
        .atomic_numbers
        .iter()
        .zip(&molecule.positions)
        .map(|(&z, &position)| {
            let element = element_data(z);
            AtomInstance {
                center: position.to_array(),
                vdw_radius: element.vdw_radius,
                color: element.cpk_color,
                _padding: 0.0,
            }
        })
        .collect()
}

pub fn build_bond_instances(molecule: &Molecule) -> Vec<BondInstance> {
    let mut instances = Vec::with_capacity(molecule.bonds.len() * 2);
    for bond in &molecule.bonds {
        let a = molecule.positions[bond.atom_a];
        let b = molecule.positions[bond.atom_b];
        let color_a = element_data(molecule.atomic_numbers[bond.atom_a]).cpk_color;
        let color_b = element_data(molecule.atomic_numbers[bond.atom_b]).cpk_color;
        let midpoint = (a + b) * 0.5;

        for (start, end, color) in [(a, midpoint, color_a), (midpoint, b, color_b)] {
            let segment = end - start;
            let length = segment.length();
            if length < 1e-5 {
                continue;
            }
            let axis = segment / length;
            let center = (start + end) * 0.5;
            instances.push(BondInstance {
                center: center.to_array(),
                length,
                axis: axis.to_array(),
                _padding: 0.0,
                color,
                _padding2: 0.0,
            });
        }
    }
    instances
}
