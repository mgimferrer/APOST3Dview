use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::material::Material;

/// GPU-side scene uniform block. Field groups are padded to 16 bytes so the
/// layout matches WGSL's uniform address-space alignment rules directly.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SceneUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 4],
    /// ambient, diffuse, specular, shininess
    pub material: [f32; 4],
}

impl SceneUniforms {
    pub fn new(view_proj: Mat4, light_dir: Vec3, material: &Material) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
            material: [material.ambient, material.diffuse, material.specular, material.shininess],
        }
    }
}
