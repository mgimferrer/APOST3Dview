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
    /// The reference look — matches what Martí tuned to and confirmed
    /// against the VMD/CYLview reference images (2026-08-12). This is what
    /// the style panel's "Default" button resets to.
    fn default() -> Self {
        Self {
            ambient: 0.30,
            diffuse: 0.75,
            specular: 0.45,
            shininess: 32.0,
            // Aligned with the camera (see light_dir below) rather than
            // offset from it, so no part of the visible hemisphere ever
            // falls into shadow, at any orbit angle.
            light_yaw: 0.0,
            light_pitch: 0.0,
            background: [1.0, 1.0, 1.0],
            atom_scale: 0.20,
            bond_radius: 0.15,
        }
    }
}

impl Material {
    /// Light direction as an offset from camera-forward, in the camera's
    /// own (right, up, forward) basis — a "headlight" rig. Rotates with
    /// the camera so the lit/shadowed pattern on the molecule stays fixed
    /// relative to the screen as you orbit, instead of sweeping across the
    /// molecule the way a world-fixed light direction would. At the
    /// default zero offset the light sits exactly on the camera (a ring
    /// light/flash rig): shading becomes a pure function of view
    /// direction, so it's rotation-invariant, not just less noticeable —
    /// no point on the visible hemisphere can fall into shadow.
    pub fn light_dir(&self, camera_right: Vec3, camera_up: Vec3, camera_forward: Vec3) -> Vec3 {
        // Subtract the forward component: a headlight sits near the
        // camera and shines into the scene, so the surface-to-light
        // direction points back out toward the viewer, not further in.
        (camera_right * self.light_pitch.cos() * self.light_yaw.sin()
            + camera_up * self.light_pitch.sin()
            - camera_forward * self.light_pitch.cos() * self.light_yaw.cos())
        .normalize()
    }
}
