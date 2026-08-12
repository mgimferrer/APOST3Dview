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
    /// ambient, diffuse, specular, shininess
    pub material: [f32; 4],
    /// atom_scale, bond_radius, unused, unused
    pub style: [f32; 4],
}

impl SceneUniforms {
    pub fn new(camera: &OrbitCamera, aspect_ratio: f32, material: &Material) -> Self {
        let view_proj = camera.view_projection_matrix(aspect_ratio);
        let eye = camera.eye();
        let (right, up) = camera.screen_basis();
        let light_dir = material.light_dir();
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            camera_eye: [eye.x, eye.y, eye.z, 0.0],
            camera_right: [right.x, right.y, right.z, 0.0],
            camera_up: [up.x, up.y, up.z, 0.0],
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            material: [material.ambient, material.diffuse, material.specular, material.shininess],
            style: [material.atom_scale, material.bond_radius, 0.0, 0.0],
        }
    }
}
