//! APOST-3D results attached to a structure (see `apost3dview_core::apost`
//! for the file itself).

use std::path::{Path, PathBuf};

use apost3dview_core::{check_against_fchk, load_apost_results, parse_fchk_summary, report_path_for, ApostResults, IsosurfaceMesh};

use crate::app::App;
use crate::results_view::ResultsView;

/// Fragment colors (their envelopes), in a fixed order: fragment 1 always
/// gets the first. Eight distinct hues chosen to stay apart for
/// color-blind viewers too (validated categorical order, light
/// background); a ninth fragment reuses the first color, so the fragment
/// list (swatch + click) stays the reference for which atoms belong where.
pub(crate) const FRAGMENT_PALETTE: [[u8; 3]; 8] = [
    [0x2a, 0x78, 0xd6], // blue
    [0xeb, 0x68, 0x34], // orange
    [0x1b, 0xaf, 0x7a], // aqua
    [0xed, 0xa1, 0x00], // yellow
    [0xe8, 0x7b, 0xa4], // magenta
    [0x00, 0x83, 0x00], // green
    [0x4a, 0x3a, 0xa7], // violet
    [0xe3, 0x49, 0x48], // red
];
/// Atoms in no fragment: a neutral gray, unlike any fragment's hue (the
/// fragment list's swatch for them).
pub(crate) const UNASSIGNED_COLOR: [u8; 3] = [0xb0, 0xae, 0xa6];

/// One swatch per fragment id (in its palette color), side by side.
pub(crate) fn fragment_swatches(ui: &mut egui::Ui, results: &ApostResults, ids: impl IntoIterator<Item = u32>) {
    for id in ids {
        if let Some(position) = results.fragments().iter().position(|f| f.id == id) {
            let [r, g, b] = fragment_color(position);
            crate::util::swatch(ui, egui::Color32::from_rgb(r, g, b), 10.0);
        }
    }
}

pub(crate) fn fragment_color(position: usize) -> [u8; 3] {
    FRAGMENT_PALETTE[position % FRAGMENT_PALETTE.len()]
}

/// What fragment envelopes depend on: the atom size (its bits) and the
/// hidden atoms, sorted.
pub(crate) type EnvelopeKey = (u32, Vec<usize>);

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
    /// What is shown on the molecule (`None` when the results were refused).
    pub(crate) view: Option<ResultsView>,
    /// Fragment envelope meshes, kept while the atom size and the hidden
    /// atoms stay the same (the key).
    pub(crate) envelope_cache: Option<(EnvelopeKey, Vec<IsosurfaceMesh>)>,
}

impl ApostAttachment {
    fn new(results_path: &Path, results: Result<ApostResults, String>, has_fchk: bool) -> Self {
        let report_path = results.as_ref().ok().and_then(|r| report_path_for(results_path, r));
        let view = results.as_ref().ok().map(ResultsView::new);
        Self { results_path: results_path.to_path_buf(), results, has_fchk, report_path, report_lines: None, view, envelope_cache: None }
    }

    pub(crate) fn without_fchk(results_path: &Path, results: ApostResults) -> Self {
        Self::new(results_path, Ok(results), false)
    }

    pub(crate) fn file_name(&self) -> String {
        crate::util::file_display_name(&self.results_path)
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
        let results_name = crate::util::file_display_name(results_path);
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
        // The density drawn is the one APOST-3D analysed (a Gaussian file
        // with an MP2 density, analysed without DENS 2, shows the SCF one).
        if let Ok(r) = &results
            && let Some(densities) = self.structures[index].densities.as_mut()
            && let Some(kind) = crate::natural::analysed_density_kind(r, &densities.info)
        {
            densities.choose_kind(&kind);
        }
        let sets = results.as_ref().map(|r| crate::orbital_sets::find_orbital_sets(r, results_path)).unwrap_or_default();
        self.structures[index].orbital_sets = sets;
        self.structures[index].active_orbital_set = 0;
        self.structures[index].apost = Some(ApostAttachment::new(results_path, results, true));
        self.open_window(crate::windows::ToolWindow::Apost);
    }
}
