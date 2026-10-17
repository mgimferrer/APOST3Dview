//! The WGSL source of every shader module, assembled at compile time from
//! the files in `shaders/`. WGSL has no include mechanism, so code shared
//! between modules lives in its own files and is prepended:
//!
//! - `common.wgsl`: pure functions (color pipeline, GGX BRDF, cloud
//!   lighting, depth to world), no bindings;
//! - `scene.wgsl`: the scene uniforms at group 0, `finalize_color` and the
//!   atom/bond `shade`;
//! - `ao_sample.wgsl`: `apply_ao`, reading ambient occlusion in the shaded
//!   passes (each shader declares its three bindings at its own group);
//! - `fullscreen.wgsl`: the full-screen triangle vertex shader.
//!
//! Line numbers in a shader compilation error count from the start of the
//! assembled module, shared parts included.

macro_rules! assemble {
    ($($file:literal),+ $(,)?) => {
        concat!($(include_str!(concat!("shaders/", $file)), "\n"),+)
    };
}

pub(crate) const SPHERE: &str = assemble!("common.wgsl", "scene.wgsl", "ao_sample.wgsl", "sphere.wgsl");
pub(crate) const CYLINDER: &str = assemble!("common.wgsl", "scene.wgsl", "ao_sample.wgsl", "cylinder.wgsl");
pub(crate) const ISOSURFACE: &str = assemble!("common.wgsl", "scene.wgsl", "ao_sample.wgsl", "isosurface.wgsl");
pub(crate) const ENVELOPE: &str = assemble!("common.wgsl", "scene.wgsl", "envelope.wgsl");
pub(crate) const TEXT: &str = assemble!("common.wgsl", "scene.wgsl", "text.wgsl");
pub(crate) const AO: &str = assemble!("common.wgsl", "fullscreen.wgsl", "ao.wgsl");
pub(crate) const DOF: &str = assemble!("common.wgsl", "fullscreen.wgsl", "dof.wgsl");

#[cfg(test)]
mod tests {
    use wgpu::naga;

    /// Every assembled module parses and validates — what `cargo check`
    /// can't tell, without a GPU.
    #[test]
    fn every_shader_compiles() {
        for (name, source) in [
            ("sphere", super::SPHERE),
            ("cylinder", super::CYLINDER),
            ("isosurface", super::ISOSURFACE),
            ("envelope", super::ENVELOPE),
            ("text", super::TEXT),
            ("ao", super::AO),
            ("dof", super::DOF),
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
        }
    }
}
