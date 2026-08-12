use glam::Vec3;

/// Live-bound material/lighting parameters, driven by the side panel.
/// Mirrors CYLview's Styles panel.
#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub ambient: f32,
    pub diffuse: f32,
    pub specular: f32,
    pub shininess: f32,
    pub light_yaw: f32,
    pub light_pitch: f32,
    pub background: [f32; 3],
    /// Multiplier on each atom's van der Waals radius (VMD "Sphere Scale").
    pub atom_scale: f32,
    /// Bond cylinder radius, angstrom (VMD "Bond Radius" — an absolute
    /// value there, not a multiplier).
    pub bond_radius: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            ambient: 0.25,
            diffuse: 0.7,
            specular: 0.4,
            shininess: 32.0,
            light_yaw: -0.6,
            light_pitch: 0.9,
            background: [1.0, 1.0, 1.0],
            // Starting point, not a settled value: Martí's "0.7"/"0.3" were
            // calibrated against VMD's own internal radius table, which we
            // don't have access to, and against covalent radii (since
            // replaced by van der Waals radii here — see element.rs for
            // why). Live-tune from here once you can see it.
            atom_scale: 0.35,
            bond_radius: 0.15,
        }
    }
}

impl Material {
    pub fn light_dir(&self) -> Vec3 {
        Vec3::new(
            self.light_pitch.cos() * self.light_yaw.sin(),
            self.light_pitch.sin(),
            self.light_pitch.cos() * self.light_yaw.cos(),
        )
        .normalize()
    }
}
