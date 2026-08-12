use std::collections::HashSet;

use bytemuck::{Pod, Zeroable};

use apost3dview_core::{element_data, Molecule};

/// Selection-highlight overlay color — drawn translucent, on top of the
/// normal opaque render, via a separate alpha-blended pass.
pub const HIGHLIGHT_COLOR: [f32; 3] = [1.0, 0.84, 0.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondVisualStyle {
    /// Solid cylinder — the default.
    Single,
    /// Dashed cylinder, for depicting a bond being formed/broken in a
    /// transition state.
    TransitionState,
}

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
    /// 0.0 = solid (Single), 1.0 = dashed (TransitionState).
    pub dashed: f32,
    pub color: [f32; 3],
    pub _padding: f32,
}

pub fn build_atom_instances(molecule: &Molecule, hidden_atoms: &HashSet<usize>) -> Vec<AtomInstance> {
    molecule
        .atomic_numbers
        .iter()
        .zip(&molecule.positions)
        .enumerate()
        .filter(|(index, _)| !hidden_atoms.contains(index))
        .map(|(_, (&z, &position))| {
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

pub fn build_bond_instances(
    molecule: &Molecule,
    hidden_atoms: &HashSet<usize>,
    hidden_bonds: &HashSet<usize>,
    bond_styles: &[BondVisualStyle],
) -> Vec<BondInstance> {
    let mut instances = Vec::with_capacity(molecule.bonds.len() * 2);
    for (bond_index, bond) in molecule.bonds.iter().enumerate() {
        if hidden_bonds.contains(&bond_index) {
            continue;
        }
        if hidden_atoms.contains(&bond.atom_a) || hidden_atoms.contains(&bond.atom_b) {
            continue;
        }

        let dashed = match bond_styles.get(bond_index) {
            Some(BondVisualStyle::TransitionState) => 1.0,
            _ => 0.0,
        };

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
                dashed,
                color,
                _padding: 0.0,
            });
        }
    }
    instances
}

/// Slightly larger than the real atoms, so the halo peeks out as a visible
/// rim rather than being fully hidden behind the opaque atom underneath.
const HIGHLIGHT_ATOM_SCALE: f32 = 1.18;

/// Same shape as the real atoms (scaled up a little), just the selected
/// subset with a fixed highlight color — drawn as a second, translucent
/// pass on top.
pub fn build_atom_highlight_instances(molecule: &Molecule, selected_atoms: &[usize]) -> Vec<AtomInstance> {
    selected_atoms
        .iter()
        .filter_map(|&index| {
            let z = *molecule.atomic_numbers.get(index)?;
            let position = *molecule.positions.get(index)?;
            Some(AtomInstance {
                center: position.to_array(),
                vdw_radius: element_data(z).vdw_radius * HIGHLIGHT_ATOM_SCALE,
                color: HIGHLIGHT_COLOR,
                _padding: 0.0,
            })
        })
        .collect()
}

pub fn build_bond_highlight_instances(molecule: &Molecule, selected_bonds: &[usize]) -> Vec<BondInstance> {
    let mut instances = Vec::with_capacity(selected_bonds.len() * 2);
    for &bond_index in selected_bonds {
        let Some(bond) = molecule.bonds.get(bond_index) else { continue };
        let a = molecule.positions[bond.atom_a];
        let b = molecule.positions[bond.atom_b];
        let segment = b - a;
        let length = segment.length();
        if length < 1e-5 {
            continue;
        }
        let axis = segment / length;
        let center = (a + b) * 0.5;
        instances.push(BondInstance {
            center: center.to_array(),
            length,
            axis: axis.to_array(),
            dashed: 0.0,
            color: HIGHLIGHT_COLOR,
            _padding: 0.0,
        });
    }
    instances
}
