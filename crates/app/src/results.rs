//! APOST-3D results attached to a structure (see `apost3dview_core::apost`
//! for the file itself).

use std::path::{Path, PathBuf};

use apost3dview_core::{check_against_fchk, load_apost_results, parse_fchk_summary, report_path_for, ApostResults};

use crate::app::App;

/// The APOST-3D results file found for (or opened with) a structure.
pub(crate) struct ApostAttachment {
    pub(crate) results_path: PathBuf,
    /// `Err` when the file was found but not attached: unreadable, or
    /// computed from another calculation than the structure's `.fchk`.
    /// The reason is shown in the APOST-3D window for as long as the
    /// structure is open, not only in a passing warning.
    pub(crate) results: Result<ApostResults, String>,
    /// False when the results file was opened without its `.fchk`, so the
    /// geometry comes from the results file and there are no orbitals.
    pub(crate) has_fchk: bool,
    /// The text report `<job>.apost`, if it exists, and its lines once the
    /// user first expands it (it can be several MB, so it is not read up
    /// front).
    pub(crate) report_path: Option<PathBuf>,
    pub(crate) report_lines: Option<Result<Vec<String>, String>>,
}

impl ApostAttachment {
    fn new(results_path: &Path, results: Result<ApostResults, String>, has_fchk: bool) -> Self {
        let report_path = results.as_ref().ok().and_then(|r| report_path_for(results_path, r));
        Self { results_path: results_path.to_path_buf(), results, has_fchk, report_path, report_lines: None }
    }

    pub(crate) fn without_fchk(results_path: &Path, results: ApostResults) -> Self {
        Self::new(results_path, Ok(results), false)
    }

    pub(crate) fn file_name(&self) -> String {
        self.results_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub(crate) fn load_report(&mut self) {
        if self.report_lines.is_some() {
            return;
        }
        let Some(path) = &self.report_path else { return };
        self.report_lines = Some(std::fs::read_to_string(path).map(|text| text.lines().map(str::to_string).collect()).map_err(|e| e.to_string()));
    }
}

impl App {
    /// Reads `results_path` and attaches it to structure `index` (opened
    /// from a `.fchk`) if it was computed from that `.fchk`: same atoms,
    /// coordinates, basis set size and energy (`check_against_fchk`).
    /// Opens the APOST-3D window either way, so a refused file is
    /// explained rather than silently ignored.
    pub(crate) fn attach_results(&mut self, index: usize, results_path: &Path) {
        let Some(structure) = self.structures.get(index) else { return };
        let results_name = results_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let fchk_name = structure.label.clone();

        let results = load_apost_results(results_path)
            .map_err(|err| format!("{results_name} could not be read: {err}"))
            .and_then(|results| {
                let Some(fchk) = &structure.source_path else { return Ok(results) };
                let summary = parse_fchk_summary(fchk).map_err(|err| format!("{fchk_name} could not be read for the comparison: {err}"))?;
                check_against_fchk(&results, &summary).map_err(|reason| format!("{results_name} does not belong to {fchk_name}: {reason}."))?;
                Ok(results)
            });

        match &results {
            Ok(r) if r.stopped() => self.show_warning(format!("{results_name}: APOST-3D stopped before the end of this run.")),
            Ok(_) => self.show_status(format!("APOST-3D results loaded from {results_name}")),
            Err(reason) => self.show_warning(format!("{reason} The results were not loaded.")),
        }
        self.structures[index].apost = Some(ApostAttachment::new(results_path, results, true));
        self.show_apost = true;
    }
}
