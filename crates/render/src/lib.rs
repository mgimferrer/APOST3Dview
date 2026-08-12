//! wgpu renderer and WGSL shaders. Owns the orbit camera, material/lighting
//! uniforms, and (for now) a placeholder reference grid — real atom/bond
//! impostor rendering lands once `.fchk` parsing exists in `apost3dview-core`.

pub mod camera;
pub mod grid;
pub mod material;
pub mod uniforms;
pub mod viewport;

pub use camera::OrbitCamera;
pub use material::Material;
pub use uniforms::SceneUniforms;
pub use viewport::{ViewportCallback, ViewportResources};
