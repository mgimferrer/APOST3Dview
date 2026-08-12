//! Data model and file parsing (.fchk, .cube, later .apost). No rendering
//! dependencies — kept separate so the render/app crates can't accidentally
//! couple parsing logic to wgpu or egui.

pub mod coordinates;
pub mod cube;
pub mod element;
pub mod fchk;
pub mod interpolation;
pub mod isosurface;
pub mod measurement;
pub mod molecule;
pub mod units;
pub mod xyz;

pub use coordinates::{format_coordinates, CoordinateFormat};
pub use cube::{parse_cube, CubeFile, ScalarGrid};
pub use element::{atomic_number_from_symbol, element_data, ElementData};
pub use interpolation::refine_grid;
pub use isosurface::{extract_isosurface, IsosurfaceMesh};
pub use measurement::{measure, MeasurementKind};
pub use molecule::{Bond, Molecule};
pub use units::LengthUnit;
pub use xyz::parse_xyz;
