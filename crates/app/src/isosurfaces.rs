//! Isosurfaces as layers of a structure: every `.cube` opened for a
//! molecule and every orbital generated from its wavefunction is one
//! `Surface` in that structure's list, shown or hidden on its own, so
//! several are composed into one picture just by showing them together.

use std::path::PathBuf;

use apost3dview_core::{extract_isosurface, refine_grid, IsosurfaceMesh, ScalarGrid};
use apost3dview_render::{push_isosurface_vertices, IsosurfaceVertex, ViewportResources};
use egui::Color32;

use crate::app::App;
use crate::orbitals::OrbitalChoice;
use crate::util::color32_to_rgb;

/// Isovalue is a fixed constant rather than derived from each grid's own
/// data (an earlier approach: 25% of the grid's max |value|) — Martí's
/// explicit preference after tuning it live against the real EFFAO test
/// cubes, simpler and more predictable than a per-file heuristic.
pub(crate) const DEFAULT_ISOSURFACE_ISOVALUE: f32 = 0.075;
/// Fully opaque by default — tried translucent (0.5, then 0.75) first,
/// but a real hands-on check (2026-08-29) came back preferring solid
/// lobes; the opacity slider is still there for anyone who wants to see
/// through them.
pub(crate) const DEFAULT_ISOSURFACE_OPACITY: f32 = 1.0;

/// How a surface is drawn: everything about it except its data.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct SurfaceSettings {
    pub(crate) isovalue: f32,
    /// 1x/2x/3x — see `apost3dview_core::refine_grid`.
    pub(crate) refinement: usize,
    /// Show the negative lobe (at -isovalue) too.
    pub(crate) both_signs: bool,
    pub(crate) positive_color: Color32,
    pub(crate) negative_color: Color32,
    /// 1.0 = opaque (the default), lower = translucent.
    pub(crate) opacity: f32,
}

impl Default for SurfaceSettings {
    fn default() -> Self {
        Self {
            isovalue: DEFAULT_ISOSURFACE_ISOVALUE,
            refinement: 1,
            both_signs: true,
            positive_color: Color32::from_rgb(60, 90, 230),
            negative_color: Color32::from_rgb(220, 70, 60),
            opacity: DEFAULT_ISOSURFACE_OPACITY,
        }
    }
}

/// The settings that change the extracted mesh itself (the others only
/// change how an existing mesh is colored).
#[derive(Clone, Copy, PartialEq)]
struct ExtractionKey {
    isovalue: f32,
    refinement: usize,
    both_signs: bool,
}

/// Where a surface's grid came from — what a session records to recreate
/// it: the file for an opened cube, the recipe for a generated orbital
/// (recomputed on reopening, rather than storing the grid itself).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SurfaceSource {
    Cube(PathBuf),
    Orbital { choice: OrbitalChoice, spacing_bohr: f64, padding_bohr: f64 },
}

pub(crate) struct Surface {
    /// "H2O-HOMO.cube", "HOMO", "MO 12 (beta)", ...
    pub(crate) label: String,
    pub(crate) source: SurfaceSource,
    pub(crate) grid: ScalarGrid,
    pub(crate) visible: bool,
    pub(crate) settings: SurfaceSettings,
    positive: Option<IsosurfaceMesh>,
    negative: Option<IsosurfaceMesh>,
    /// What `positive`/`negative` were extracted with; `None` before the
    /// first extraction.
    extracted_with: Option<ExtractionKey>,
}

impl Surface {
    pub(crate) fn new(label: String, grid: ScalarGrid, settings: SurfaceSettings, source: SurfaceSource) -> Self {
        Self { label, source, grid, visible: false, settings, positive: None, negative: None, extracted_with: None }
    }

    fn extraction_key(&self) -> ExtractionKey {
        ExtractionKey { isovalue: self.settings.isovalue, refinement: self.settings.refinement, both_signs: self.settings.both_signs }
    }

    /// Shown, but its mesh doesn't match its current settings yet.
    fn needs_extraction(&self) -> bool {
        self.visible && self.extracted_with != Some(self.extraction_key())
    }

    /// Re-runs Surface Nets at the current isovalue/refinement/both-signs.
    /// The real cost here (tens of ms in a release build, more on a
    /// refined grid), so it only runs for visible surfaces whose settings
    /// changed — see `App::update_surfaces`.
    fn extract(&mut self) {
        let key = self.extraction_key();
        let refined;
        let grid = if key.refinement > 1 {
            refined = refine_grid(&self.grid, key.refinement);
            &refined
        } else {
            &self.grid
        };
        self.positive = Some(extract_isosurface(grid, key.isovalue));
        self.negative = key.both_signs.then(|| extract_isosurface(&grid.negated(), key.isovalue));
        self.extracted_with = Some(key);
    }

    fn push_vertices(&self, vertices: &mut Vec<IsosurfaceVertex>) {
        let s = &self.settings;
        if let Some(mesh) = &self.positive {
            push_isosurface_vertices(vertices, mesh, color32_to_rgb(s.positive_color), s.opacity);
        }
        if s.both_signs {
            if let Some(mesh) = &self.negative {
                push_isosurface_vertices(vertices, mesh, color32_to_rgb(s.negative_color), s.opacity);
            }
        }
    }
}

impl App {
    /// Adds surfaces to a structure and, when `show` is set, shows the
    /// first of them alone — what opening a `.cube` or generating orbitals
    /// leads to (several cubes opened at once show the first). A surface
    /// with the same label as an existing one replaces it (generating an
    /// orbital again at another accuracy updates it), keeping how it is
    /// drawn.
    pub(crate) fn add_surfaces(&mut self, structure_index: usize, new_surfaces: Vec<Surface>, show: bool) {
        let Some(structure) = self.structures.get_mut(structure_index) else { return };
        let mut first = None;
        for mut surface in new_surfaces {
            let index = match structure.surfaces.iter().position(|s| s.label == surface.label) {
                Some(index) => {
                    surface.settings = structure.surfaces[index].settings;
                    structure.surfaces[index] = surface;
                    index
                }
                None => {
                    structure.surfaces.push(surface);
                    structure.surfaces.len() - 1
                }
            };
            first.get_or_insert(index);
        }
        if let Some(index) = first.filter(|_| show) {
            structure.show_surface_alone(index);
        }
        if self.active_structure == Some(structure_index) {
            self.rebuild_isosurface();
        }
    }

    /// Extracts every shown surface of the active structure whose settings
    /// changed, then re-uploads. Called once per frame, so a change shows
    /// up by itself, without an "Update" button; while a value is being
    /// dragged (the isovalue field, a slider) extraction waits for the
    /// release, so it runs once per edit rather than on every frame of it.
    pub(crate) fn update_surfaces(&mut self, ctx: &egui::Context) {
        if ctx.dragged_id().is_some() {
            return;
        }
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let mut changed = false;
        for surface in structure.surfaces.iter_mut().filter(|s| s.needs_extraction()) {
            surface.extract();
            changed = true;
        }
        if changed {
            self.rebuild_isosurface();
        }
    }

    /// Combines the active structure's shown, already-extracted surfaces
    /// into one vertex buffer and uploads it — cheap (concatenating
    /// computed vertex data), safe to call whenever anything about how the
    /// surfaces look changes, unlike the extraction itself.
    pub(crate) fn rebuild_isosurface(&self) {
        let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
        if let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) {
            for surface in structure.surfaces.iter().filter(|s| s.visible) {
                surface.push_vertices(&mut vertices);
            }
        }
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_isosurface(&self.render_state.device, &vertices);
            resources.update_isosurface_material(&self.render_state.queue, &self.ao_render_isosurface_material());
        }
    }
}
