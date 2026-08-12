//! wgpu renderer and WGSL shaders. Owns the orbit camera, material/lighting
//! uniforms, and the atom/bond raymarched-impostor rendering pipeline.

pub mod camera;
pub mod consts;
pub mod instances;
pub mod material;
pub mod mesh;
pub mod uniforms;
pub mod viewport;

pub use camera::OrbitCamera;
pub use consts::{DEPTH_BUFFER_BITS, DEPTH_FORMAT, MSAA_SAMPLES};
pub use material::Material;
pub use uniforms::SceneUniforms;
pub use viewport::{ViewportCallback, ViewportResources};
