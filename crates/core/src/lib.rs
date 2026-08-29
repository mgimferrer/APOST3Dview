//! Data model and file parsing (.fchk, .cube, later .apost). No rendering
//! dependencies — kept separate so the render/app crates can't accidentally
//! couple parsing logic to wgpu or egui.

pub mod coordinates;
pub mod cube;
pub mod element;
pub mod fchk;
pub mod gto;
pub mod interpolation;
pub mod isosurface;
pub mod measurement;
pub mod molden;
pub mod molecule;
pub mod units;
pub mod wavefunction;
pub mod xyz;

pub use coordinates::{format_coordinates, CoordinateFormat};
pub use cube::{parse_cube, CubeFile, ScalarGrid};
pub use element::{atomic_number_from_symbol, element_data, ElementData};
pub use gto::{evaluate_basis_functions, evaluate_mo, generate_mo_grids};
pub use interpolation::refine_grid;
pub use isosurface::{extract_isosurface, IsosurfaceMesh};
pub use measurement::{measure, MeasurementKind};
pub use molden::{parse_molden_geometry, parse_molden_wavefunction, MoldenGeometry};
pub use molecule::{Bond, Molecule};
pub use units::LengthUnit;
pub use wavefunction::{parse_fchk_wavefunction, BasisSet, MolecularOrbitals, Shell, Wavefunction};
pub use xyz::parse_xyz;
