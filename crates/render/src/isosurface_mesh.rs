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
use glam::Vec3;
use rayon::prelude::*;

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
///
/// GGX, not Blinn-Phong — see `Material`'s doc for the full reasoning; a
/// real side-by-side preview (`test_ggx_ao_preview.rs`, since removed)
/// showed the isosurface specifically as where GGX's advantage over the
/// old model showed up *most*: it's the largest continuous smooth surface
/// in a typical scene, so a roughness-shaped highlight has the most room
/// to read as genuinely more dimensional than Blinn-Phong's flatter
/// response on the same geometry.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, serde::Serialize, serde::Deserialize)]
pub struct IsosurfaceMaterial {
    /// ambient, roughness, reflectance (F0), light_intensity
    pub material: [f32; 4],
    /// fresnel power, fresnel/rim strength, unused, unused
    pub fresnel: [f32; 4],
}

impl Default for IsosurfaceMaterial {
    fn default() -> Self {
        // Rougher than the atom/bond default (0.42) on purpose — a
        // broader, dimmer highlight reads as soft translucent glass/jelly
        // rather than the polished-ball look that suits CPK spheres.
        // Reflectance and light_intensity otherwise match the atom/bond
        // default, so the isosurface and the geometry it enloses feel lit
        // by the same light, not two different scenes. The Fresnel rim
        // (power 2.2, strength 0.85 — tuned live against a real large
        // orbital, 2026-08-29) is a deliberate *addition* on top of GGX's
        // own physically-correct Fresnel term, not a substitute for it:
        // it's meant to read as light escaping/glowing at the edges of a
        // translucent field, which isn't really surface reflectance and
        // so doesn't belong in the BRDF itself.
        Self { material: [0.35, 0.7, 0.045, 3.0], fresnel: [2.2, 0.85, 0.0, 0.0] }
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

/// Whether a vertex belongs to a translucent surface: drawn sorted, without
/// writing depth, and left out of ambient occlusion.
pub(crate) fn is_translucent(vertex: &IsosurfaceVertex) -> bool {
    vertex.opacity < 1.0
}

/// `triangles` (a triangle soup, 3 vertices each) reordered farthest from
/// `eye` first, by triangle center: blended in that order, every layer of
/// every translucent surface shows, each over what lies behind it. Exact
/// except where two triangles cross each other (surfaces intersecting),
/// where the error is at most one small triangle wide.
pub(crate) fn sort_back_to_front(triangles: &[IsosurfaceVertex], eye: Vec3) -> Vec<IsosurfaceVertex> {
    let mut order: Vec<(f32, u32)> = triangles
        .par_chunks_exact(3)
        .enumerate()
        .map(|(i, t)| {
            let center = (Vec3::from_array(t[0].position) + Vec3::from_array(t[1].position) + Vec3::from_array(t[2].position)) / 3.0;
            (center.distance_squared(eye), i as u32)
        })
        .collect();
    // Ties (and so the result) ordered by triangle index, for the same
    // picture every time.
    order.par_sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    order.iter().flat_map(|&(_, i)| triangles[i as usize * 3..i as usize * 3 + 3].iter().copied()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(z: f32, tag: f32) -> [IsosurfaceVertex; 3] {
        let v = |x: f32, y: f32| IsosurfaceVertex {
            position: [x, y, z],
            _padding0: 0.0,
            normal: [0.0, 0.0, 1.0],
            _padding1: 0.0,
            color: [tag, 0.0, 0.0],
            opacity: 0.5,
        };
        [v(0.0, 0.0), v(1.0, 0.0), v(0.0, 1.0)]
    }

    #[test]
    fn triangles_are_sorted_farthest_first_and_kept_whole() {
        let soup: Vec<IsosurfaceVertex> = [triangle(-1.0, 1.0), triangle(-5.0, 2.0), triangle(-3.0, 3.0)].concat();
        let sorted = sort_back_to_front(&soup, Vec3::new(0.0, 0.0, 10.0));
        let tags: Vec<f32> = sorted.chunks(3).map(|t| t[0].color[0]).collect();
        assert_eq!(tags, vec![2.0, 3.0, 1.0]);
        for t in sorted.chunks(3) {
            assert!(t.iter().all(|v| v.color[0] == t[0].color[0]), "a triangle's vertices stay together");
        }
        // From the other side, the order reverses.
        let sorted = sort_back_to_front(&soup, Vec3::new(0.0, 0.0, -10.0));
        let tags: Vec<f32> = sorted.chunks(3).map(|t| t[0].color[0]).collect();
        assert_eq!(tags, vec![1.0, 3.0, 2.0]);
    }

    #[test]
    fn opacity_one_is_opaque() {
        let mut v = triangle(0.0, 0.0)[0];
        assert!(is_translucent(&v));
        v.opacity = 1.0;
        assert!(!is_translucent(&v));
    }
}
