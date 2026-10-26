//! Data model and file parsing (.fchk, .cube, .molden, APOST-3D results).
//! No rendering dependencies — kept separate so the render/app crates can't accidentally
//! couple parsing logic to wgpu or egui.

pub mod apost;
pub mod apost_input;
pub mod atomic_weights;
pub mod coordinates;
pub mod cube;
pub mod density_matrix;
pub mod element;
pub mod envelope;
pub mod fchk;
pub mod fragmentation;
pub mod gto;
pub mod interpolation;
pub mod isosurface;
pub mod measurement;
pub mod molden;
pub mod molecule;
pub mod superpose;
pub mod units;
pub mod wavefunction;
pub mod xyz;

pub use apost::{
    check_against_fchk, check_orbital_file, check_orbital_file_id, orbital_file_run_id, find_results_for, fragment_coverage, fragment_of_atom, is_results_file, load_apost_results, parse_apost_results,
    report_path_for, results_path_for, wavefunction_path_for, ApostResults, Enpart, EnpartFunctional, Fragment, FunctionalComponent, TwoElectronIntegration, ENPART_TERMS, FragmentCoverage, FrontierEfo, FrontierEfos, FrontierKind, FrontierOrbital, IndexEntry, MatrixObject, OrbitalDetail, OrbitalLabel, OrbitalSet,
    Quantity, QuantityKind, TableCell, TableColumn, TableObject,
};
pub use apost_input::{read_apost_input, read_fchk_facts, AimScheme, ApostInput, DmFiles, EnpartMethod, EnpartOptions, FchkFacts, FchkProgram, OxidationStates, ReadInput, ENPART_FUNCTIONALS};
pub use atomic_weights::{AtomCells, AtomicWeights, CellScratch};
pub use fragmentation::{fragments_around, separate_molecules};
pub use coordinates::{format_coordinates, CoordinateFormat};
pub use cube::{parse_cube, CubeFile, ScalarGrid};
pub use density_matrix::{natural_orbitals, read_fchk_density, scan_fchk_densities, DensityMatrix, FchkDensities};
pub use envelope::{envelope_mesh, sphere_mesh, unit_sphere};
pub use fchk::{parse_fchk_summary, FchkSummary};

/// The error of every parser here for a file that isn't what it claims.
pub(crate) fn invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// Parses a number written by a Fortran program, also in the two forms a
/// plain parse refuses: a `D` exponent (`1.5D-03`), and the exponent of an
/// `E16.8` field below 1e-99 or above 1e99, where Fortran has no room for
/// the `E` (`1.27052287-118` means 1.27052287E-118). The repair is only
/// tried when the plain parse fails.
pub(crate) fn parse_fortran<T: std::str::FromStr>(token: &str) -> Option<T> {
    if let Ok(v) = token.parse::<T>() {
        return Some(v);
    }
    if token.contains(['D', 'd'])
        && let Ok(v) = token.replace(['D', 'd'], "E").parse::<T>()
    {
        return Some(v);
    }
    // Missing E: a sign not preceded by E/e (index 0 may be the mantissa's sign).
    let bytes = token.as_bytes();
    for i in 1..bytes.len() {
        if matches!(bytes[i], b'+' | b'-')
            && !matches!(bytes[i - 1], b'E' | b'e')
            && let Ok(v) = format!("{}E{}", &token[..i], &token[i..]).parse::<T>()
        {
            return Some(v);
        }
    }
    None
}
pub use element::{atomic_number_from_symbol, element_data, hill_formula, ElementData};
pub use gto::{evaluate_basis_functions, evaluate_mo, generate_density_grid, generate_mo_grids, generate_weighted_mo_grids};
pub use interpolation::refine_grid;
pub use isosurface::{extract_isosurface, IsosurfaceMesh};
pub use measurement::{measure, MeasurementKind};
pub use molden::{parse_molden_geometry, parse_molden_wavefunction};
pub use molecule::{Bond, Molecule};
pub use superpose::{superpose, Superposition};
pub use units::LengthUnit;
pub use wavefunction::{parse_fchk_wavefunction, BasisSet, MolecularOrbitals, Shell, Wavefunction};
pub use xyz::parse_xyz;

#[cfg(test)]
mod fortran_numbers {
    use super::parse_fortran;

    #[test]
    fn plain_d_exponent_and_missing_e() {
        assert_eq!(parse_fortran::<f64>("-1.25E-03"), Some(-1.25e-3));
        assert_eq!(parse_fortran::<f64>("1.5D-03"), Some(1.5e-3));
        assert_eq!(parse_fortran::<f64>("1.27052287-118"), Some(1.27052287e-118));
        assert_eq!(parse_fortran::<f64>("-4.5+101"), Some(-4.5e101));
        assert_eq!(parse_fortran::<f32>("0.78396-17"), Some(0.78396e-17));
        assert_eq!(parse_fortran::<f64>("abc"), None);
        assert_eq!(parse_fortran::<i64>("12"), Some(12));
    }
}
