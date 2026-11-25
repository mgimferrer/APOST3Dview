//! The feature areas' app-wide state. Each area also keeps a part of every
//! structure (`LoadedStructure::surfaces`, `::apost`, `::frames`), and its
//! code is in its own files (`impl App` blocks and `windows/`). The core
//! (camera, look, structures, windows, export) stays in `App` itself.
//!
//! An area may read another area's state; it changes it only through that
//! area's methods.

use crate::isosurfaces::SurfaceSettings;
use crate::orbitals::OrbitalGenerationState;

/// The Surfaces area: orbitals, densities and cube files.
#[derive(Default)]
pub(crate) struct SurfacesArea {
    /// The orbital picker's grid and box settings.
    pub(crate) generation: OrbitalGenerationState,
    /// How newly opened or generated orbitals and cubes are drawn: the
    /// default settings in `App::orbital_colors`, or a surface's own after
    /// "Use for new surfaces".
    pub(crate) defaults: SurfaceSettings,
}

/// The APOST-3D area: results on the molecule, tables, the input generator.
#[derive(Default)]
pub(crate) struct ApostArea {
    /// Fixed color ranges chosen by the user, by quantity id, shared by
    /// every structure so figures of different systems compare.
    pub(crate) color_ranges: std::collections::BTreeMap<String, apost3dview_render::ColorScale>,
    /// List, in the Show menu, the values APOST-3D prints only for
    /// comparison (Mulliken's, in a run with another atomic definition)
    /// and the secondary ones (`results_view::is_secondary`).
    pub(crate) list_other_values: bool,
    /// The table chosen in the APOST-3D window's Tables section (entry id).
    pub(crate) table: Option<String>,
    /// The APOST-3D window's part shown; `None`: the new input for a
    /// structure without results (loaded or next to it), the results
    /// otherwise.
    pub(crate) tab: Option<crate::input_generator::ApostTab>,
    /// What the results display was last built for (see
    /// `update_results_display`).
    pub(crate) results_signature: String,
}
