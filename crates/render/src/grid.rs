use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// Ground-plane reference grid. Purely a bring-up aid so the orbit camera
/// has something to orbit around before real molecular geometry exists.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GridVertex {
    pub position: [f32; 3],
}

pub fn build_grid_lines(half_extent: i32, spacing: f32) -> Vec<GridVertex> {
    let mut vertices = Vec::new();
    let limit = half_extent as f32 * spacing;
    for i in -half_extent..=half_extent {
        let offset = i as f32 * spacing;
        vertices.push(GridVertex { position: Vec3::new(offset, 0.0, -limit).to_array() });
        vertices.push(GridVertex { position: Vec3::new(offset, 0.0, limit).to_array() });
        vertices.push(GridVertex { position: Vec3::new(-limit, 0.0, offset).to_array() });
        vertices.push(GridVertex { position: Vec3::new(limit, 0.0, offset).to_array() });
    }
    vertices
}
