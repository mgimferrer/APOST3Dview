//! The APOST-3D area's part of a session: the results file a structure had
//! open and what it showed on the molecule. Saved as fields of the
//! structure's entry (`results`, `results_absolute`, `results_fingerprint`,
//! `results_view`), as they always were.

use std::path::Path;

use super::{check_fingerprint, file_name, fingerprint, relative_path, resolve, Fingerprint, StructureState};
use crate::app::App;
use crate::results_view::ResultsView;
use crate::state::LoadedStructure;

/// A structure's results entries as saved.
pub(super) struct SavedResults {
    pub(super) results: Option<String>,
    pub(super) results_absolute: Option<String>,
    pub(super) results_fingerprint: Option<Fingerprint>,
    pub(super) results_view: Option<ResultsView>,
}

/// The results of `structure` (opened from `path`) as saved, paths relative
/// to `base`. The results file is recorded only when it isn't `path` itself.
pub(super) fn saved_results(structure: &LoadedStructure, path: &Path, base: &Path) -> SavedResults {
    let results = structure.apost.results.as_ref().map(|a| &a.results_path).filter(|r| !crate::files::same_file(r, path));
    SavedResults {
        results: results.map(|r| relative_path(base, r)),
        results_absolute: results.map(|r| r.to_string_lossy().into_owned()),
        results_fingerprint: results.and_then(|r| fingerprint(r)),
        results_view: structure.apost.results.as_ref().and_then(|a| a.view.clone()),
    }
}

/// What the results show on the molecule, as saved (after the surfaces are
/// back).
pub(super) fn restore_results_view(structure: &mut LoadedStructure, state: &StructureState) {
    if let (Some(view), Some(attachment)) = (&state.results_view, structure.apost.results.as_mut())
        && attachment.results.is_ok()
    {
        attachment.view = Some(view.clone());
    }
}

impl App {
    /// Attaches the APOST-3D results the session had open for structure
    /// `index`.
    pub(super) fn reopen_results(&mut self, index: usize, state: &StructureState, base: &Path, problems: &mut Vec<String>) {
        let results = match (&state.results, &state.results_absolute) {
            (Some(relative), absolute) => {
                let found = resolve(base, relative, absolute.as_deref().unwrap_or(""));
                if found.is_none() {
                    problems.push(format!("{} was not found", file_name(relative)));
                }
                found
            }
            (None, _) if state.results_view.is_some() && self.structures[index].apost.results.is_none() => self.structures[index].apost.nearby.clone(),
            _ => None,
        };
        if let Some(results) = results {
            check_fingerprint(&results, state.results_fingerprint.as_ref(), problems);
            self.attach_results(index, &results);
        }
    }
}
