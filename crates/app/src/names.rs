//! How atoms and fragments are named in the APOST-3D views, in one form
//! everywhere: "O1" (element and 1-based number), "1 CH3" (fragment id and
//! Hill formula).

use apost3dview_core::{element_data, hill_formula, ApostResults, Fragment};

/// "O1", from a 0-based atom index.
pub(crate) fn atom_name(atomic_numbers: &[u32], a: usize) -> String {
    match atomic_numbers.get(a) {
        Some(&z) => format!("{}{}", element_data(z).symbol, a + 1),
        None => (a + 1).to_string(),
    }
}

/// "1 CH3".
pub(crate) fn fragment_name(fragment: &Fragment, atomic_numbers: &[u32]) -> String {
    format!("{} {}", fragment.id, hill_formula(fragment.atoms.iter().filter_map(|&a| atomic_numbers.get(a - 1).copied())))
}

/// The same from a fragment id (the bare id if the run has no such
/// fragment).
pub(crate) fn fragment_name_by_id(results: &ApostResults, atomic_numbers: &[u32], id: u32) -> String {
    results.fragments().iter().find(|f| f.id == id).map(|f| fragment_name(f, atomic_numbers)).unwrap_or_else(|| id.to_string())
}
