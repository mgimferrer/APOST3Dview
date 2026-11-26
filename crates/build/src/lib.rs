//! Editing molecular geometries, for the Build window of APOST-3Dview:
//! which atoms move when a distance, angle or dihedral is set (`sides`),
//! the edits themselves (`internal`), and changes to the atoms (`atoms`).
//! No UI and no GPU, so every edit is tested here on its own.
//!
//! Positions are angstrom, as in `apost3dview_core::Molecule`. They are
//! stored as `f32`, but every edit is computed in `f64` and rounded once at
//! the end, so setting a value gives it to the precision of the storage.

pub mod atoms;
pub mod clean;
pub mod internal;
pub mod rebuild;
pub mod sides;
pub mod symmetry;

pub use atoms::{change_element, delete_atoms, Deleted};
pub use clean::{clean, is_metal, CleanOptions, Cleaned, KeptSymmetry};
pub use internal::{
    angle_degrees, center, dihedral_degrees, distance, rotate_about_bond, set_angle, set_dihedral, set_distance, transform_atoms, translate, EditError, Moves,
};
pub use rebuild::rebuild;
pub use sides::{moving_atoms, Graph, Which};
pub use symmetry::{analyze, standard_frame, standard_orientation, Analysis, Candidate, Element, FIND_TOLERANCE, PROGRAM_TOLERANCE};
