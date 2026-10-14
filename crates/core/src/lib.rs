//! Data model and file parsing (.fchk, .cube, .molden, APOST-3D results).
//! No rendering dependencies — kept separate so the render/app crates can't accidentally
//! couple parsing logic to wgpu or egui.

pub mod apost;
pub mod coordinates;
pub mod cube;
pub mod element;
pub mod envelope;
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

pub use apost::{
    check_against_fchk, check_orbital_file, find_results_for, fragment_coverage, fragment_of_atom, is_results_file, load_apost_results, parse_apost_results,
    report_path_for, results_path_for, wavefunction_path_for, ApostResults, Fragment, FragmentCoverage, IndexEntry, MatrixObject, OrbitalDetail, OrbitalLabel, OrbitalSet,
    Quantity, QuantityKind, TableCell, TableColumn, TableObject,
};
pub use coordinates::{format_coordinates, CoordinateFormat};
pub use cube::{parse_cube, CubeFile, ScalarGrid};
pub use envelope::{envelope_mesh, sphere_mesh, unit_sphere};
pub use fchk::{parse_fchk_summary, FchkSummary};
pub use element::{atomic_number_from_symbol, element_data, hill_formula, ElementData};
pub use gto::{evaluate_basis_functions, evaluate_mo, generate_mo_grids};
pub use interpolation::refine_grid;
pub use isosurface::{extract_isosurface, IsosurfaceMesh};
pub use measurement::{measure, MeasurementKind};
pub use molden::{parse_molden_geometry, parse_molden_wavefunction, MoldenGeometry};
pub use molecule::{Bond, Molecule};
pub use units::LengthUnit;
pub use wavefunction::{parse_fchk_wavefunction, BasisSet, MolecularOrbitals, Shell, Wavefunction};
pub use xyz::parse_xyz;
