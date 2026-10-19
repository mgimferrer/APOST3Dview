//! APOST3Dview application: egui windows on top of the wgpu renderer.
//! A library as well as a binary, so integration tests can drive the real
//! `App` headlessly.



mod app;
mod export;
mod files;
mod isosurfaces;
mod label_layout;
mod labels;
mod names;
mod natural;
mod orbital_sets;
mod orbitals;
mod presets;
mod results;
mod results_display;
mod results_view;
mod session;
mod state;
mod util;
mod viewport;
mod windows;

pub use app::{App, StartupOptions};
