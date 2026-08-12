use bytemuck::{Pod, Zeroable};

/// Vertex for the shared unit-cylinder mesh used by bond rendering: radius
/// 1 in x/z, spanning y in [-0.5, 0.5]. Instances scale/rotate/translate
/// this per bond segment.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CylinderVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// Builds an open-ended (no end caps) unit cylinder — the caps are hidden
/// inside the atom sphere impostors at each end in practice, since bond
/// radius is normally well under atom radius.
pub fn build_unit_cylinder(sides: u32) -> (Vec<CylinderVertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity((sides as usize + 1) * 2);
    for i in 0..=sides {
        let theta = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (sin, cos) = theta.sin_cos();
        let normal = [cos, 0.0, sin];
        vertices.push(CylinderVertex { position: [cos, -0.5, sin], normal });
        vertices.push(CylinderVertex { position: [cos, 0.5, sin], normal });
    }

    let mut indices = Vec::with_capacity(sides as usize * 6);
    for i in 0..sides {
        let bottom0 = i * 2;
        let top0 = i * 2 + 1;
        let bottom1 = i * 2 + 2;
        let top1 = i * 2 + 3;
        indices.extend_from_slice(&[bottom0, bottom1, top0, top0, bottom1, top1]);
    }

    (vertices, indices)
}
