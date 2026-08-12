use glam::Vec3;

/// Live-bound material/lighting parameters, driven by the side panel.
/// Mirrors CYLview's Styles panel. Phase 1 has no atom/bond geometry yet,
/// so these are wired through to the uniform buffer but only visibly affect
/// the placeholder grid — real shading plugs in once impostor rendering
/// lands.
#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub ambient: f32,
    pub diffuse: f32,
    pub specular: f32,
    pub shininess: f32,
    pub light_yaw: f32,
    pub light_pitch: f32,
    pub background: [f32; 3],
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
            background: [0.043, 0.055, 0.075],
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
