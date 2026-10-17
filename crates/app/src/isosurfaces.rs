//! Isosurfaces as layers of a structure: every `.cube` opened for a
//! molecule and every orbital generated from its wavefunction is one
//! `Surface` in that structure's list, shown or hidden on its own, so
//! several are composed into one picture just by showing them together.

use std::path::PathBuf;

use apost3dview_core::{extract_isosurface, refine_grid, IsosurfaceMesh, ScalarGrid};
use apost3dview_render::{push_isosurface_vertices, IsosurfaceVertex};
use egui::Color32;

use crate::app::App;
use crate::orbitals::OrbitalChoice;
use glam::Vec3;
use crate::util::color32_to_rgb;

/// Isovalue is a fixed constant rather than derived from each grid's own
/// data (an earlier approach: 25% of the grid's max |value|) — Martí's
/// explicit preference after tuning it live against the real EFFAO test
/// cubes, simpler and more predictable than a per-file heuristic.
pub(crate) const DEFAULT_ISOSURFACE_ISOVALUE: f32 = 0.075;
/// Fully opaque by default — tried translucent (0.5, then 0.75) first,
/// but a real hands-on check (2026-08-29) came back preferring solid
/// lobes, and Martí confirmed it once translucency was drawn correctly
/// (2026-10-17); the Translucent checkbox and the opacity slider are
/// there for anyone who wants to see through them.
pub(crate) const DEFAULT_ISOSURFACE_OPACITY: f32 = 1.0;
/// What the Translucent checkbox sets for an orbital or a cube: enough to
/// see the atoms inside and the lobes behind, while the lobes keep their
/// shape (chosen from comparison renders, 2026-10-17).
pub(crate) const TRANSLUCENT_ORBITAL_OPACITY: f32 = 0.5;
/// The total density's default, and what its Translucent checkbox sets: it
/// wraps every atom, so it stays lighter than an orbital — at 0.35 its
/// shape reads clearly while the atoms keep their colors; from 0.45 up
/// everything inside takes a blue cast (comparison renders of water and
/// the Bi complex, 2026-10-17).
pub(crate) const TRANSLUCENT_DENSITY_OPACITY: f32 = 0.35;

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
    /// For an analysis orbital (EFO, OSLO): colored by its fragment(s)
    /// instead of the positive/negative colors. Off by default: orbitals
    /// keep the usual blue/red signs.
    pub(crate) by_fragment: bool,
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
            by_fragment: false,
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
    Orbital {
        choice: OrbitalChoice,
        spacing_bohr: f64,
        padding_bohr: f64,
        /// The analysis set ("eos", "geos", "oslo") for an EFO or OSLO;
        /// `None` for the SCF orbitals.
        set: Option<String>,
    },
    /// The electron density (or, `spin`, the spin density) of the
    /// structure's wavefunction, from its occupied orbitals.
    Density { spin: bool, spacing_bohr: f64, padding_bohr: f64 },
}

/// How a density is first drawn, chosen from comparison renders (H2O
/// triplet, BiCl3): total at 0.1 e/bohr^3, light blue, translucent
/// (`TRANSLUCENT_DENSITY_OPACITY`, so the molecule shows inside; lower
/// isovalues swallow the molecule); spin at +/-0.01 with both signs (alpha
/// excess positive, beta excess negative), opaque. Not the last-used
/// orbital settings, whose isovalues mean nothing for a density.
pub(crate) fn density_settings(spin: bool) -> SurfaceSettings {
    let defaults = SurfaceSettings::default();
    if spin {
        SurfaceSettings { isovalue: 0.01, both_signs: true, ..defaults }
    } else {
        SurfaceSettings {
            isovalue: 0.1,
            both_signs: false,
            positive_color: Color32::from_rgb(120, 160, 210),
            opacity: TRANSLUCENT_DENSITY_OPACITY,
            ..defaults
        }
    }
}

pub(crate) struct Surface {
    /// "H2O-HOMO.cube", "HOMO", "MO 12 (beta)", ...
    pub(crate) label: String,
    pub(crate) source: SurfaceSource,
    pub(crate) grid: ScalarGrid,
    pub(crate) visible: bool,
    pub(crate) settings: SurfaceSettings,
    /// For an analysis orbital: its fragments' atoms and colors, to color
    /// the surface by fragment (blended across space for a shared one).
    pub(crate) fragment_tint: Option<Vec<(Vec3, [f32; 3])>>,
    positive: Option<IsosurfaceMesh>,
    negative: Option<IsosurfaceMesh>,
    /// What `positive`/`negative` were extracted with; `None` before the
    /// first extraction.
    extracted_with: Option<ExtractionKey>,
}

impl Surface {
    pub(crate) fn new(label: String, grid: ScalarGrid, settings: SurfaceSettings, source: SurfaceSource) -> Self {
        Self { label, source, grid, visible: false, settings, fragment_tint: None, positive: None, negative: None, extracted_with: None }
    }

    /// A total or spin density, which has its own defaults
    /// (`density_settings`) rather than the orbitals' ones.
    pub(crate) fn density(&self) -> Option<bool> {
        match self.source {
            SurfaceSource::Density { spin, .. } => Some(spin),
            _ => None,
        }
    }

    /// What the Default button goes back to.
    pub(crate) fn default_settings(&self) -> SurfaceSettings {
        self.density().map_or_else(SurfaceSettings::default, density_settings)
    }

    /// What the Translucent checkbox sets.
    pub(crate) fn translucent_opacity(&self) -> f32 {
        if self.density().is_some() {
            TRANSLUCENT_DENSITY_OPACITY
        } else {
            TRANSLUCENT_ORBITAL_OPACITY
        }
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
        if let (true, Some(tint)) = (s.by_fragment, &self.fragment_tint) {
            if !tint.is_empty() {
                // Positive lobe in the fragment color, negative in a paler
                // tint of it: the phase stays visible, the fragment too.
                for (mesh, pale) in [(&self.positive, false), (&self.negative, true)] {
                    if pale && !s.both_signs {
                        continue;
                    }
                    if let Some(mesh) = mesh {
                        push_tinted_vertices(vertices, mesh, tint, pale, s.opacity);
                    }
                }
                return;
            }
        }
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

/// Each vertex colored by the fragments near it: Gaussian weights on the
/// distance to each tint atom, so a shared orbital blends smoothly from
/// one fragment's color to the other's.
fn push_tinted_vertices(vertices: &mut Vec<IsosurfaceVertex>, mesh: &IsosurfaceMesh, tint: &[(Vec3, [f32; 3])], pale: bool, opacity: f32) {
    let single = tint.iter().all(|(_, c)| *c == tint[0].1);
    const WIDTH2: f32 = 0.8 * 0.8;
    for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
        let mut color = tint[0].1;
        if !single {
            let mut total = 0.0f32;
            let mut sum = [0.0f32; 3];
            let mut nearest = (f32::MAX, tint[0].1);
            for (atom, c) in tint {
                let d2 = position.distance_squared(*atom);
                if d2 < nearest.0 {
                    nearest = (d2, *c);
                }
                let w = (-d2 / WIDTH2).exp();
                total += w;
                for i in 0..3 {
                    sum[i] += w * c[i];
                }
            }
            color = if total > 1e-6 { sum.map(|v| v / total) } else { nearest.1 };
        }
        if pale {
            color = color.map(|v| v + (1.0 - v) * 0.55);
        }
        vertices.push(IsosurfaceVertex { position: position.to_array(), _padding0: 0.0, normal: normal.to_array(), _padding1: 0.0, color, opacity });
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
        let material = self.ao_render_isosurface_material();
        self.with_resources(|resources| {
            resources.update_isosurface(&self.render_state.device, &vertices);
            resources.update_isosurface_material(&self.render_state.queue, &material);
        });
    }
}
