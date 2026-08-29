use bytemuck::{Pod, Zeroable};

use crate::camera::OrbitCamera;
use crate::material::Material;

/// GPU-side scene uniform block. Field groups are padded to 16 bytes so the
/// layout matches WGSL's uniform address-space alignment rules directly.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SceneUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub camera_eye: [f32; 4],
    pub camera_right: [f32; 4],
    pub camera_up: [f32; 4],
    pub light_dir: [f32; 4],
    /// ambient, roughness, reflectance (F0), light_intensity — see
    /// `Material`'s doc for why this is a GGX parameterization, not
    /// Blinn-Phong's.
    pub material: [f32; 4],
    /// atom_scale, bond_radius, exposure, srgb_target (1.0 if the render
    /// target this frame writes to is an sRGB-encoding format — see
    /// `set_srgb_target` — 0.0 by default, correct for every headless
    /// example, which all render to a plain `Bgra8Unorm` target)
    pub style: [f32; 4],
}

impl SceneUniforms {
    pub fn new(camera: &OrbitCamera, aspect_ratio: f32, material: &Material) -> Self {
        let view_proj = camera.view_projection_matrix(aspect_ratio);
        let eye = camera.eye();
        let (right, up) = camera.screen_basis();
        let light_dir = material.light_dir(right, up, camera.forward());
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            camera_eye: [eye.x, eye.y, eye.z, 0.0],
            camera_right: [right.x, right.y, right.z, 0.0],
            camera_up: [up.x, up.y, up.z, 0.0],
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            material: [material.ambient, material.roughness, material.reflectance, material.light_intensity],
            style: [material.atom_scale, material.bond_radius, material.exposure, 0.0],
        }
    }

    /// Sets whether this frame's render target is an sRGB-encoding texture
    /// format (`is_srgb()` on the format passed to `ViewportResources::new`/
    /// `render_offscreen`) — the shaders need to know this to do the
    /// linear->sRGB encode themselves only when the hardware won't already
    /// do it automatically on write, and skip it (staying linear) when the
    /// hardware will. Getting this wrong either way visibly washes out or
    /// darkens the whole image, so every real call site (the live view,
    /// PNG export) must call this; only headless examples that hardcode a
    /// plain `Bgra8Unorm` target can skip it and rely on the `0.0` default.
    pub fn set_srgb_target(&mut self, is_srgb: bool) {
        self.style[3] = if is_srgb { 1.0 } else { 0.0 };
    }
}
