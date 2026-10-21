//! Per-structure state and the shared label styles.

use std::collections::HashSet;
use std::path::PathBuf;

use apost3dview_core::{MeasurementKind, Molecule, Wavefunction};
use apost3dview_render::BondVisualStyle;
use egui::Color32;

use crate::isosurfaces::Surface;
use crate::natural::Densities;
use crate::orbital_sets::LoadedOrbitalSet;
use crate::results::ApostAttachment;

/// A committed distance/angle/dihedral, with a screen-space label position
/// the user can drag away from the default (the involved atoms' centroid)
/// — for composing publication images without labels overlapping.
pub(crate) struct Measurement {
    pub(crate) kind: MeasurementKind,
    pub(crate) label_offset: egui::Vec2,
}

/// Shared across every measurement/structure — tune once, applies
/// everywhere, same reasoning as the Appearance settings.
pub(crate) struct MeasurementStyle {
    pub(crate) font_size: f32,
    pub(crate) decimal_places: usize,
    pub(crate) text_color: Color32,
    pub(crate) line_color: [f32; 3],
    pub(crate) bold: bool,
}

impl Default for MeasurementStyle {
    fn default() -> Self {
        Self {
            font_size: 30.0,
            decimal_places: 2,
            text_color: Color32::from_rgb(20, 20, 20),
            line_color: [0.05, 0.05, 0.05],
            bold: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AtomLabelMode {
    None,
    /// 1-based index, matching the Coordinates window's atom numbering.
    Number,
    /// Element symbol only — H, C, Bi, ...
    Type,
    /// Symbol + number — C1, C2, H1, ... the common convention.
    NumberType,
}

/// Shared across every structure, same reasoning as MeasurementStyle.
/// Separate from it rather than reusing it — different typical needs (an
/// atom label sits on top of a CPK-colored sphere rather than on empty
/// space, so it usually wants a smaller size and its own contrast choice).
pub(crate) struct AtomLabelStyle {
    /// Label height as a fraction of the atom's own rendered radius. A
    /// real world-space size (not a screen-pixel one) so labels are true
    /// 3D geometry that scales with zoom exactly like the atom they're
    /// attached to, instead of holding a constant on-screen size the way
    /// a 2D overlay would.
    pub(crate) relative_size: f32,
    pub(crate) text_color: Color32,
}

impl Default for AtomLabelStyle {
    fn default() -> Self {
        Self { relative_size: 1.0, text_color: Color32::BLACK }
    }
}

/// One opened structure — its own geometry and its own hide/selection/
/// bond-style state. Deliberately does NOT own the Appearance settings (material, AO, ...) — that
/// stays a single value shared across every structure, so tuning it once
/// applies everywhere instead of needing to be redone per file.
pub(crate) struct LoadedStructure {
    pub(crate) label: String,
    pub(crate) molecule: Molecule,
    /// The file this structure was opened from, if any — used e.g. to
    /// default a "save coordinates as .xyz" export next to the source
    /// `.fchk`.
    pub(crate) source_path: Option<PathBuf>,
    pub(crate) hidden_atoms: HashSet<usize>,
    pub(crate) hidden_bonds: HashSet<usize>,
    pub(crate) bond_styles: Vec<BondVisualStyle>,
    /// How many of `molecule.bonds` came from bond perception; the ones
    /// after were created by hand ("Create bond"), which a session records.
    pub(crate) perceived_bonds: usize,
    pub(crate) selected_atoms: Vec<usize>,
    pub(crate) selected_bonds: Vec<usize>,
    pub(crate) measurements: Vec<Measurement>,
    /// Isosurfaces drawn on this molecule: `.cube` files opened for it and
    /// orbitals generated from its wavefunction, each shown or hidden on
    /// its own.
    pub(crate) surfaces: Vec<Surface>,
    /// The surface the Surfaces window's settings edit.
    pub(crate) selected_surface: Option<usize>,
    /// `Some` only for structures opened from a `.fchk` — the basis set +
    /// MO coefficients, parsed once up front so the "Generate orbitals"
    /// section can evaluate any MO on demand without re-parsing the file.
    /// `wavefunction.beta` is itself `Some` only for an unrestricted
    /// (open-shell) file — see `parse_fchk_wavefunction`.
    pub(crate) wavefunction: Option<Wavefunction>,
    /// The correlated densities of a `.fchk` and their natural orbitals
    /// (`None` for a mean-field file, a `.molden`, or no wavefunction).
    pub(crate) densities: Option<Densities>,
    /// Which MOs are ticked in the "Generate orbitals" picker — 0-based,
    /// only meaningful when `wavefunction` is `Some`. Beta is only ever
    /// populated for an unrestricted file (see above); it stays empty,
    /// unused, and hidden in the UI otherwise.
    pub(crate) selected_alpha_mos: HashSet<usize>,
    /// The densities ticked for generation, beside the orbitals.
    pub(crate) selected_total_density: bool,
    pub(crate) selected_spin_density: bool,
    pub(crate) selected_beta_mos: HashSet<usize>,
    /// APOST-3D results opened for this structure.
    pub(crate) apost: Option<ApostAttachment>,
    /// The `<job>.apost.json` found next to this structure's `.fchk`, not
    /// loaded: results are opened on request (Martí, 2026-10-19), here
    /// with the APOST-3D window's "Open these results".
    pub(crate) results_nearby: Option<std::path::PathBuf>,
    /// The analysis orbital sets next to the results (EOS, GEOS, OSLO).
    pub(crate) orbital_sets: Vec<LoadedOrbitalSet>,
    /// The orbitals the Surfaces window generates from: 0 the SCF ones,
    /// i the set `orbital_sets[i - 1]`.
    pub(crate) active_orbital_set: usize,
    /// A new APOST-3D input being prepared for this `.fchk`.
    pub(crate) input_draft: Option<crate::input_generator::InputDraft>,
}

impl LoadedStructure {
    pub(crate) fn new(label: String, molecule: Molecule, source_path: Option<PathBuf>) -> Self {
        let bond_styles = vec![BondVisualStyle::Single; molecule.bonds.len()];
        let perceived_bonds = molecule.bonds.len();
        Self {
            label,
            molecule,
            source_path,
            hidden_atoms: HashSet::new(),
            hidden_bonds: HashSet::new(),
            bond_styles,
            perceived_bonds,
            selected_atoms: Vec::new(),
            selected_bonds: Vec::new(),
            measurements: Vec::new(),
            surfaces: Vec::new(),
            selected_surface: None,
            wavefunction: None,
            densities: None,
            selected_alpha_mos: HashSet::new(),
            selected_total_density: false,
            selected_spin_density: false,
            selected_beta_mos: HashSet::new(),
            apost: None,
            results_nearby: None,
            orbital_sets: Vec::new(),
            active_orbital_set: 0,
            input_draft: None,
        }
    }

    /// Shows surface `index` and hides the others; it becomes the one the
    /// Surfaces window edits.
    pub(crate) fn show_surface_alone(&mut self, index: usize) {
        for (i, surface) in self.surfaces.iter_mut().enumerate() {
            surface.visible = i == index;
        }
        self.selected_surface = Some(index);
    }
}
