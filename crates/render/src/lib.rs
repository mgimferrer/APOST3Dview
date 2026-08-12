//! wgpu renderer and WGSL shaders. Owns the orbit camera, material/lighting
//! uniforms, and the atom/bond raymarched-impostor rendering pipeline.

pub mod camera;
pub mod consts;
pub mod export;
pub mod glyphs;
pub mod instances;
pub mod isosurface_mesh;
pub mod label;
pub mod material;
pub mod mesh;
pub mod picking;
pub mod uniforms;
pub mod viewport;

pub use camera::OrbitCamera;
pub use consts::{DEPTH_BUFFER_BITS, DEPTH_FORMAT, MSAA_SAMPLES};
pub use export::{downsample_rgba, ExportSettings};
pub use glyphs::{glyph_scale_for_font_size, glyph_scale_for_world_size, GlyphAtlas};
pub use instances::{BondVisualStyle, HIGHLIGHT_COLOR};
pub use isosurface_mesh::{push_isosurface_vertices, IsosurfaceMaterial, IsosurfaceVertex};
pub use label::{layout_label, GlyphInstance};
pub use material::Material;
pub use picking::{is_atom_visible, pick_atom, pick_bond, ray_from_ndc};
pub use uniforms::SceneUniforms;
pub use viewport::{ViewportCallback, ViewportResources};
