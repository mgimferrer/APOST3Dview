//! Data model and file parsing (.fchk, .cube, later .apost). No rendering
//! dependencies — kept separate so the render/app crates can't accidentally
//! couple parsing logic to wgpu or egui.

pub mod coordinates;
pub mod element;
pub mod fchk;
pub mod molecule;
pub mod units;

pub use coordinates::{format_coordinates, CoordinateFormat};
pub use element::{element_data, ElementData};
pub use molecule::{Bond, Molecule};
pub use units::LengthUnit;
