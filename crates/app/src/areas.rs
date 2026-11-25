//! The feature areas' app-wide state. Each area also keeps a part of every
//! structure (`LoadedStructure::surfaces`, `::apost`, `::frames`), and its
//! code is in its own files (`impl App` blocks and `windows/`). The core
//! (camera, look, structures, windows, export) stays in `App` itself.
//!
//! An area may read another area's state; it changes it only through that
//! area's methods.

use crate::app::App;
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

/// What a click on an atom in the view does.
pub(crate) enum AtomClick {
    /// The atom joins or leaves the selection.
    Select,
    /// The atom (or its fragment) becomes the focus of the pair quantity
    /// shown.
    Focus(usize),
}

impl App {
    /// Who gets a click on an atom, decided here only, in this order: the
    /// New input tab (its fragments are made from the selection), the
    /// APOST-3D focus view (a pair quantity shown), otherwise the selection.
    pub(crate) fn atom_click(&self, atom: usize) -> AtomClick {
        if self.editing_input() {
            return AtomClick::Select;
        }
        match self.focus_for_atom(atom) {
            Some(focus) => AtomClick::Focus(focus),
            None => AtomClick::Select,
        }
    }
}

/// The one list of what the areas do each frame and which windows they
/// draw. A new area adds its lines here (and, with a window, a
/// `ToolWindow`), nowhere else in the frame loop.
impl App {
    /// Each frame, before anything is drawn (after the core has opened any
    /// dropped files).
    pub(crate) fn update_areas(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt.min(0.1));
        if self.advance_playback(dt) {
            ctx.request_repaint();
        }
        self.update_surfaces(ctx);
        self.update_results_display(ctx);
        self.update_figure_preview(ctx);
    }

    /// The tool windows, in toolbar order, then the others.
    pub(crate) fn show_windows(&mut self, ctx: &egui::Context) {
        self.show_molecule_window(ctx);
        self.show_surfaces_window(ctx);
        self.show_measure_window(ctx);
        self.show_apost_window(ctx);
        self.show_appearance_window(ctx);
        self.show_export_window(ctx);
        self.show_figure_preview_window(ctx);
        self.show_coordinates_window(ctx);
        self.show_about_window(ctx);
    }
}
