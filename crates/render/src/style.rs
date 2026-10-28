//! How atoms and bonds are drawn (Molecule window > Style): ball and
//! stick, sticks, thin sticks or space-filling. Colors are the same in
//! every style; only sizes change, so the same impostor pipelines draw
//! them all.
//!
//! Every part that needs an atom's size (the instances, the selection
//! highlight, picking, which labels are visible) takes it from one
//! [`DrawnSizes`], so they all follow the style.

use std::collections::HashSet;

use apost3dview_core::{element_data, Molecule};

/// One style for the whole view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoleculeStyle {
    /// Balls at a fraction of the van der Waals radius, joined by
    /// cylinders (the default).
    #[default]
    BallAndStick,
    /// Atoms as round joints as thick as the bonds (licorice), so
    /// surfaces aren't covered by balls.
    Sticks,
    /// The same, much thinner: the molecule as a skeleton under large
    /// surfaces.
    ThinSticks,
    /// Van der Waals spheres, no bonds.
    SpaceFilling,
}

impl MoleculeStyle {
    pub const ALL: [MoleculeStyle; 4] = [MoleculeStyle::BallAndStick, MoleculeStyle::Sticks, MoleculeStyle::ThinSticks, MoleculeStyle::SpaceFilling];
}

/// The chosen style and the sizes of the styles other than ball and stick
/// (whose atom size and bond radius stay in `Material`). Each style keeps
/// its own sizes, so switching back and forth loses nothing.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MoleculeStyleSettings {
    pub style: MoleculeStyle,
    /// Sticks: radius of bonds and joints, angstrom.
    pub stick_radius: f32,
    /// Thin sticks: radius of bonds and joints, angstrom.
    pub thin_stick_radius: f32,
    /// Space-filling: multiplier on the van der Waals radii.
    pub space_filling_scale: f32,
}

/// The ball-and-stick bond radius (`Material::default`), so sticks look
/// like ball and stick with the balls taken away.
pub const STICK_RADIUS: f32 = 0.16;
pub const THIN_STICK_RADIUS: f32 = 0.06;

impl Default for MoleculeStyleSettings {
    fn default() -> Self {
        Self { style: MoleculeStyle::BallAndStick, stick_radius: STICK_RADIUS, thin_stick_radius: THIN_STICK_RADIUS, space_filling_scale: 1.0 }
    }
}

impl MoleculeStyleSettings {
    /// Stick radius of a stick style, `None` for the others.
    pub fn stick(&self) -> Option<f32> {
        match self.style {
            MoleculeStyle::Sticks => Some(self.stick_radius),
            MoleculeStyle::ThinSticks => Some(self.thin_stick_radius),
            MoleculeStyle::BallAndStick | MoleculeStyle::SpaceFilling => None,
        }
    }

    /// Whether bonds are drawn.
    pub fn draws_bonds(&self) -> bool {
        self.style != MoleculeStyle::SpaceFilling
    }

    /// The scale the GPU multiplies each atom instance's radius by, and
    /// the bond radius, for this style; `atom_scale` and `bond_radius` are
    /// the ball-and-stick sizes. Ball and stick returns them unchanged.
    pub fn gpu_sizes(&self, atom_scale: f32, bond_radius: f32) -> (f32, f32) {
        match self.style {
            MoleculeStyle::BallAndStick => (atom_scale, bond_radius),
            // Stick instances carry their radius in angstrom.
            MoleculeStyle::Sticks | MoleculeStyle::ThinSticks => (1.0, self.stick().unwrap_or(bond_radius)),
            MoleculeStyle::SpaceFilling => (self.space_filling_scale, bond_radius),
        }
    }
}

/// Atom and bond sizes as drawn in one style, for one molecule.
#[derive(Debug, Clone)]
pub struct DrawnSizes {
    /// Per atom: the radius an atom instance carries, multiplied by
    /// `scale` on the GPU.
    pub base: Vec<f32>,
    /// What the GPU multiplies `base` by (`SceneUniforms` style.x).
    pub scale: f32,
    /// Bond cylinder radius, or `None` when bonds aren't drawn.
    pub bond_radius: Option<f32>,
    /// Each atom's ball-and-stick radius: halos, envelopes and atom names
    /// are never smaller than this, so they read the same in every style.
    pub ball: Vec<f32>,
}

impl DrawnSizes {
    /// The sizes for `molecule` in `settings`' style. `atom_scale` and
    /// `bond_radius` are the ball-and-stick sizes; `bond_scales` the
    /// per-bond thickness (from bond orders), if drawn.
    ///
    /// In the stick styles, an atom is a joint as thick as its thickest
    /// visible bond; an atom without one (an ion, or one whose bonds are
    /// hidden) stays a ball-and-stick ball, so it doesn't vanish.
    pub fn new(
        molecule: &Molecule,
        settings: &MoleculeStyleSettings,
        atom_scale: f32,
        bond_radius: f32,
        hidden_atoms: &HashSet<usize>,
        hidden_bonds: &HashSet<usize>,
        bond_scales: Option<&[f32]>,
    ) -> Self {
        let vdw: Vec<f32> = molecule.atomic_numbers.iter().map(|&z| element_data(z).vdw_radius).collect();
        let ball: Vec<f32> = vdw.iter().map(|r| r * atom_scale).collect();
        let (scale, gpu_bond_radius) = settings.gpu_sizes(atom_scale, bond_radius);
        let base = match settings.stick() {
            None => vdw,
            Some(stick) => {
                let mut thickest: Vec<Option<f32>> = vec![None; molecule.positions.len()];
                for (index, bond) in molecule.bonds.iter().enumerate() {
                    if hidden_bonds.contains(&index) || hidden_atoms.contains(&bond.atom_a) || hidden_atoms.contains(&bond.atom_b) {
                        continue;
                    }
                    let scale = bond_scales.and_then(|s| s.get(index).copied()).unwrap_or(1.0);
                    for atom in [bond.atom_a, bond.atom_b] {
                        thickest[atom] = Some(thickest[atom].map_or(scale, |s: f32| s.max(scale)));
                    }
                }
                thickest.iter().zip(&ball).map(|(t, &b)| t.map_or(b, |t| stick * t)).collect()
            }
        };
        Self { base, scale, bond_radius: settings.draws_bonds().then_some(gpu_bond_radius), ball }
    }

    /// Ball and stick with the given sizes (no hidden bonds matter).
    pub fn ball_and_stick(molecule: &Molecule, atom_scale: f32, bond_radius: f32) -> Self {
        Self::new(molecule, &MoleculeStyleSettings::default(), atom_scale, bond_radius, &HashSet::new(), &HashSet::new(), None)
    }

    /// Atom `index`'s radius as drawn, angstrom.
    pub fn radius(&self, index: usize) -> f32 {
        self.base[index] * self.scale
    }

    /// Atom `index`'s radius as drawn, but never below its ball-and-stick
    /// radius: for halos, envelopes and labels.
    pub fn radius_at_least_ball(&self, index: usize) -> f32 {
        self.radius(index).max(self.ball[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apost3dview_core::Bond;
    use glam::Vec3;

    /// Water plus a lone sodium.
    fn water_and_sodium() -> Molecule {
        Molecule {
            atomic_numbers: vec![8, 1, 1, 11],
            positions: vec![Vec3::ZERO, Vec3::new(0.96, 0.0, 0.0), Vec3::new(-0.24, 0.93, 0.0), Vec3::new(5.0, 0.0, 0.0)],
            bonds: vec![Bond { atom_a: 0, atom_b: 1 }, Bond { atom_a: 0, atom_b: 2 }],
        }
    }

    #[test]
    fn ball_and_stick_is_unchanged() {
        let molecule = water_and_sodium();
        let sizes = DrawnSizes::ball_and_stick(&molecule, 0.24, 0.16);
        for (i, &z) in molecule.atomic_numbers.iter().enumerate() {
            assert_eq!(sizes.base[i], element_data(z).vdw_radius);
            assert_eq!(sizes.radius(i), element_data(z).vdw_radius * 0.24);
        }
        assert_eq!(sizes.scale, 0.24);
        assert_eq!(sizes.bond_radius, Some(0.16));
    }

    #[test]
    fn sticks_are_joints_as_thick_as_their_bonds() {
        let molecule = water_and_sodium();
        let settings = MoleculeStyleSettings { style: MoleculeStyle::Sticks, ..Default::default() };
        let none = HashSet::new();
        let sizes = DrawnSizes::new(&molecule, &settings, 0.24, 0.3, &none, &none, None);
        assert_eq!(sizes.bond_radius, Some(STICK_RADIUS));
        for i in 0..3 {
            assert_eq!(sizes.radius(i), STICK_RADIUS);
        }
        // The lone sodium stays a ball.
        assert_eq!(sizes.radius(3), element_data(11).vdw_radius * 0.24);
        // A hydrogen whose bond is hidden too.
        let hidden_bonds: HashSet<usize> = [1].into();
        let sizes = DrawnSizes::new(&molecule, &settings, 0.24, 0.3, &none, &hidden_bonds, None);
        assert_eq!(sizes.radius(2), element_data(1).vdw_radius * 0.24);
        // Thicker bonds, thicker joints: the oxygen takes its thickest.
        let sizes = DrawnSizes::new(&molecule, &settings, 0.24, 0.3, &none, &none, Some(&[1.5, 0.5]));
        assert_eq!(sizes.radius(0), STICK_RADIUS * 1.5);
        assert_eq!(sizes.radius(2), STICK_RADIUS * 0.5);
        // Halos and labels never shrink below the ball.
        assert_eq!(sizes.radius_at_least_ball(2), element_data(1).vdw_radius * 0.24);
    }

    #[test]
    fn space_filling_has_no_bonds() {
        let molecule = water_and_sodium();
        let settings = MoleculeStyleSettings { style: MoleculeStyle::SpaceFilling, ..Default::default() };
        let none = HashSet::new();
        let sizes = DrawnSizes::new(&molecule, &settings, 0.24, 0.16, &none, &none, None);
        assert_eq!(sizes.bond_radius, None);
        assert_eq!(sizes.radius(0), element_data(8).vdw_radius);
    }
}
