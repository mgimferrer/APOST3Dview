//! APOST3Dview application: egui windows on top of the wgpu renderer.
//! A library as well as a binary, so integration tests can drive the real
//! `App` headlessly.



mod app;
mod export;
mod files;
mod input_generator;
mod isosurfaces;
mod label_layout;
mod labels;
mod names;
mod natural;
mod orbital_sets;
mod orbitals;
mod panels;
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

/// The app's name for the system: eframe's folder for saved state (and our
/// `presets/` inside it), and the window's app id, which on Linux must
/// match the menu entry `APOST3Dview.desktop`.
pub const APP_ID: &str = "APOST3Dview";
