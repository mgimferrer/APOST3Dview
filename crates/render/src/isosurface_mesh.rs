//! GPU-side isosurface geometry: converts `apost3dview_core::IsosurfaceMesh`
//! (pure positions + normals, no rendering concerns) into vertex data
//! carrying its own color/opacity — baked in per-vertex at build time
//! rather than pulled from a shared uniform, since a single draw call
//! needs to render several differently-colored/opacity'd isosurfaces at
//! once (positive lobe, negative lobe, any "kept" ones from other
//! structures), the same reason atom/bond color is already per-instance
//! rather than a shared uniform in this renderer.

use apost3dview_core::IsosurfaceMesh;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IsosurfaceVertex {
    pub position: [f32; 3],
    pub _padding0: f32,
    pub normal: [f32; 3],
    pub _padding1: f32,
    pub color: [f32; 3],
    pub opacity: f32,
}

/// Isosurface-only lighting response — kept completely separate from the
/// atom/bond `Material` so tuning one never touches the other.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IsosurfaceMaterial {
    /// ambient, diffuse, specular, shininess
    pub material: [f32; 4],
}

impl Default for IsosurfaceMaterial {
    fn default() -> Self {
        // A softer, glossier default than the atom/bond material — smooth
        // translucent lobes read better with more specular and less flat
        // ambient than a matte CPK sphere.
        Self { material: [0.35, 0.65, 0.55, 48.0] }
    }
}

/// Appends `mesh`'s triangles as colored/opacity'd vertices onto
/// `out` — several calls (one per lobe, one per kept surface) can share
/// one growing buffer, since it's all drawn in a single pass either way.
pub fn push_isosurface_vertices(out: &mut Vec<IsosurfaceVertex>, mesh: &IsosurfaceMesh, color: [f32; 3], opacity: f32) {
    out.extend(mesh.positions.iter().zip(&mesh.normals).map(|(&position, &normal)| IsosurfaceVertex {
        position: position.to_array(),
        _padding0: 0.0,
        normal: normal.to_array(),
        _padding1: 0.0,
        color,
        opacity,
    }));
}
