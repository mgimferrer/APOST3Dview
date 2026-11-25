//! The Build area's part of a session: structures made in the Build
//! window. A copy saved as a file is written like any structure, plus its
//! name; one not saved yet is written into the session itself (its atoms,
//! coordinates and bonds), which makes the session version 2.

use std::path::{Path, PathBuf};

use apost3dview_core::{Bond, Molecule};
use glam::Vec3;
use serde::{Deserialize, Serialize};

use super::{file_name, relative_path, resolve, StructureState};
use crate::app::App;
use crate::build::StructureBuild;
use crate::state::LoadedStructure;

#[derive(Serialize, Deserialize)]
pub(super) struct EditedState {
    /// The structure's name.
    pub(super) name: String,
    /// The file the first copy was made from (relative, then absolute).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) from: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) from_absolute: String,
    /// Angstrom, one per atom of `atomic_numbers`; empty once the copy is
    /// saved as a file (then `file` is that file).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) coordinates_angstrom: Vec<[f32; 3]>,
    /// Every bond, 1-based atom pairs (with the coordinates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) bonds: Vec<[usize; 2]>,
}

impl EditedState {
    /// Written into the session (not saved as a file).
    pub(super) fn is_inline(&self) -> bool {
        !self.coordinates_angstrom.is_empty()
    }
}

/// The file a structure's entry names: its own, or for a copy not saved
/// yet, the file it was made from. `None`: not saved in sessions.
pub(super) fn session_file(structure: &LoadedStructure) -> Option<&PathBuf> {
    structure.source_path.as_ref().or_else(|| structure.build.as_ref().and_then(|b| b.from.as_ref()))
}

/// A copy whose atoms go into the session (not saved as a file since its
/// last edit).
pub(super) fn written_inline(structure: &LoadedStructure) -> bool {
    structure.build.as_ref().is_some_and(|b| !b.saved || structure.source_path.is_none())
}

/// The `edited` entry of a structure, if it is a copy.
pub(super) fn saved_edited(structure: &LoadedStructure, base: &Path) -> Option<EditedState> {
    let build = structure.build.as_ref()?;
    let inline = written_inline(structure);
    let molecule = &structure.molecule;
    Some(EditedState {
        name: structure.label.clone(),
        from: build.from.as_ref().map(|p| relative_path(base, p)).unwrap_or_default(),
        from_absolute: build.from.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        coordinates_angstrom: if inline { molecule.positions.iter().map(|p| p.to_array()).collect() } else { Vec::new() },
        bonds: if inline { molecule.bonds.iter().map(|b| [b.atom_a + 1, b.atom_b + 1]).collect() } else { Vec::new() },
    })
}

impl App {
    /// A copy written into the session, rebuilt from its entry and placed
    /// last. `None` (with the reason) when the entry doesn't hold together.
    pub(super) fn restore_edited_copy(&mut self, state: &StructureState, edited: &EditedState, base: &Path, problems: &mut Vec<String>) -> Option<usize> {
        let n = state.atomic_numbers.len();
        if edited.coordinates_angstrom.len() != n {
            problems.push(format!("{} could not be rebuilt (its coordinates and atoms don't match)", edited.name));
            return None;
        }
        let positions = edited.coordinates_angstrom.iter().map(|p| Vec3::from_array(*p)).collect();
        let bonds = edited.bonds.iter().filter(|[a, b]| (1..=n).contains(a) && (1..=n).contains(b) && a != b).map(|[a, b]| Bond { atom_a: a - 1, atom_b: b - 1 }).collect();
        let molecule = Molecule { atomic_numbers: state.atomic_numbers.clone(), positions, bonds };
        let mut structure = LoadedStructure::new(edited.name.clone(), molecule, None);
        structure.build = Some(StructureBuild { from: from_file(edited, base), saved: false, undo: Vec::new(), redo: Vec::new() });
        self.structures.push(structure);
        Some(self.structures.len() - 1)
    }

    /// A copy saved as a file, opened from it: its name and origin back.
    pub(super) fn mark_saved_copy(&mut self, index: usize, edited: &EditedState, base: &Path) {
        let structure = &mut self.structures[index];
        structure.label = edited.name.clone();
        structure.build = Some(StructureBuild { from: from_file(edited, base), saved: true, undo: Vec::new(), redo: Vec::new() });
    }
}

/// Where the copy was made from: found again, or as recorded.
fn from_file(edited: &EditedState, base: &Path) -> Option<PathBuf> {
    if edited.from.is_empty() && edited.from_absolute.is_empty() {
        return None;
    }
    resolve(base, &edited.from, &edited.from_absolute).or_else(|| (!edited.from_absolute.is_empty()).then(|| PathBuf::from(&edited.from_absolute))).or_else(|| Some(PathBuf::from(file_name(&edited.from))))
}
