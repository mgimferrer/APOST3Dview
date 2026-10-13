//! APOST3Dview application: egui windows on top of the wgpu renderer.
//! A library as well as a binary, so integration tests can drive the real
//! `App` headlessly.



mod app;
mod export;
mod files;
mod isosurfaces;
mod labels;
mod orbitals;
mod presets;
mod results;
mod state;
mod util;
mod viewport;
mod windows;

pub use app::{App, StartupOptions};
