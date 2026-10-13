//! Isosurface state, extraction and composition.

use apost3dview_core::{extract_isosurface, refine_grid};
use apost3dview_render::{push_isosurface_vertices, IsosurfaceMaterial, IsosurfaceVertex, ViewportResources};
use egui::Color32;

use crate::app::App;
use crate::util::color32_to_rgb;

/// Per-structure isosurface state — only present for structures opened
/// from a `.cube` file. Isovalue/refinement/both-signs changes don't
/// re-extract automatically (marching tetrahedra over a refined grid is a
/// real cost, seconds in a debug build) — only an explicit "Update
/// surface" click (or the one-time automatic extraction the first time a
/// cube structure becomes active) does, tracked via `extracted`.
pub(crate) struct IsosurfaceState {
    pub(crate) grid: apost3dview_core::ScalarGrid,
    pub(crate) show: bool,
    pub(crate) isovalue: f32,
    /// 1x/2x/3x — see `apost3dview_core::refine_grid`.
    pub(crate) refinement: usize,
    pub(crate) both_signs: bool,
    pub(crate) positive_color: Color32,
    pub(crate) negative_color: Color32,
    /// Always applied directly — no separate "Transparent" toggle (there
    /// used to be one, gating a fully-opaque override that was also the
    /// default, which made the toggle+slider pair pure redundant
    /// complexity: 1.0 here already means the same "fully opaque" thing).
    /// Defaults fully opaque — Martí's explicit preference after trying
    /// the translucent look live (2026-08-29): with two competing lobes
    /// this size, one at less than 1.0 doesn't read as an appealing
    /// translucent field so much as a washed-out, lower-contrast version
    /// of the opaque look. Free to lower for anyone who does want to see
    /// through it.
    pub(crate) opacity: f32,
    pub(crate) cached_positive: Option<apost3dview_core::IsosurfaceMesh>,
    pub(crate) cached_negative: Option<apost3dview_core::IsosurfaceMesh>,
    pub(crate) extracted: bool,
}

/// Isovalue is a fixed constant rather than derived from each grid's own
/// data (an earlier approach: 25% of the grid's max |value|) — Martí's
/// explicit preference after tuning it live against the real EFFAO test
/// cubes, simpler and more predictable than a per-file heuristic.
pub(crate) const DEFAULT_ISOSURFACE_ISOVALUE: f32 = 0.075;
/// Fully opaque by default — tried translucent (0.5, then 0.75) first,
/// but a real hands-on check (2026-08-29) came back preferring solid
/// lobes; the opacity slider (Visualization panel) is still there for
/// anyone who wants to see through them.
pub(crate) const DEFAULT_ISOSURFACE_OPACITY: f32 = 1.0;

impl IsosurfaceState {
    pub(crate) fn new(grid: apost3dview_core::ScalarGrid) -> Self {
        Self {
            grid,
            show: true,
            isovalue: DEFAULT_ISOSURFACE_ISOVALUE,
            refinement: 1,
            both_signs: true,
            positive_color: Color32::from_rgb(60, 90, 230),
            negative_color: Color32::from_rgb(220, 70, 60),
            opacity: DEFAULT_ISOSURFACE_OPACITY,
            cached_positive: None,
            cached_negative: None,
            extracted: false,
        }
    }

    /// Resets every tunable appearance setting back to the defaults —
    /// the Isosurfaces panel's "Default" button, mirroring Style's. Only
    /// touches settings, not the grid or any already-extracted mesh (the
    /// caller re-extracts afterward so the shown surface matches the
    /// isovalue that just reset).
    pub(crate) fn reset_to_default(&mut self) {
        self.show = true;
        self.isovalue = DEFAULT_ISOSURFACE_ISOVALUE;
        self.refinement = 1;
        self.both_signs = true;
        self.positive_color = Color32::from_rgb(60, 90, 230);
        self.negative_color = Color32::from_rgb(220, 70, 60);
        self.opacity = DEFAULT_ISOSURFACE_OPACITY;
    }
}

/// A frozen isosurface snapshot taken by "Keep surface" — see
/// `App::kept_isosurfaces`.
pub(crate) struct KeptIsosurface {
    pub(crate) vertices: Vec<IsosurfaceVertex>,
}

impl App {
    /// Combines the active structure's live isosurface (if it has one,
    /// shown, and already extracted) with every "kept" snapshot into one
    /// vertex buffer and uploads it — cheap (just concatenating already-
    /// computed vertex data), safe to call every time anything isosurface-
    /// related changes, unlike the actual extraction.
    pub(crate) fn rebuild_isosurface(&self) {
        let mut vertices: Vec<IsosurfaceVertex> = Vec::new();
        for kept in &self.kept_isosurfaces {
            vertices.extend_from_slice(&kept.vertices);
        }
        if let Some(active) = self.active_structure {
            if let Some(iso) = self.structures.get(active).and_then(|s| s.isosurface.as_ref()) {
                if iso.show {
                    let opacity = iso.opacity;
                    if let Some(mesh) = &iso.cached_positive {
                        push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.positive_color), opacity);
                    }
                    if iso.both_signs {
                        if let Some(mesh) = &iso.cached_negative {
                            push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.negative_color), opacity);
                        }
                    }
                }
            }
        }
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_isosurface(&self.render_state.device, &vertices);
            resources.update_isosurface_material(&self.render_state.queue, &self.ao_render_isosurface_material());
        }
    }

    /// Re-runs marching tetrahedra for the active structure's isosurface
    /// at its current isovalue/refinement/both-signs settings — the real
    /// cost (a refined grid can take seconds in a debug build), so this
    /// only runs on an explicit "Update surface" click or the one-time
    /// automatic extraction the first time a `.cube` structure becomes
    /// active (see `set_active`), never continuously while adjusting a
    /// slider.
    pub(crate) fn extract_active_isosurface(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get_mut(active) else { return };
        let Some(iso) = &mut structure.isosurface else { return };

        let isovalue = iso.isovalue;
        let refinement = iso.refinement;
        let both_signs = iso.both_signs;

        let refined_grid;
        let grid_ref = if refinement > 1 {
            refined_grid = refine_grid(&iso.grid, refinement);
            &refined_grid
        } else {
            &iso.grid
        };

        let positive = extract_isosurface(grid_ref, isovalue);
        let negative = if both_signs { Some(extract_isosurface(&grid_ref.negated(), isovalue)) } else { None };

        iso.cached_positive = Some(positive);
        iso.cached_negative = negative;
        iso.extracted = true;

        self.rebuild_isosurface();
    }

    /// Resets the active structure's isosurface settings (isovalue,
    /// refinement, colors, opacity, ...) *and* the shared isosurface
    /// material (ambient/diffuse/specular/shininess) back to the defaults,
    /// then re-extracts immediately — the "Default" button in the
    /// Isosurfaces panel, mirroring Style's "Default" for the atom/bond
    /// material.
    pub(crate) fn reset_active_isosurface_to_default(&mut self) {
        self.isosurface_material = IsosurfaceMaterial::default();
        let Some(active) = self.active_structure else { return };
        let Some(structure) = self.structures.get_mut(active) else { return };
        let Some(iso) = &mut structure.isosurface else { return };
        iso.reset_to_default();
        self.extract_active_isosurface();
    }

    /// Freezes the active structure's current isosurface (geometry *and*
    /// its color/opacity at this exact moment) into `kept_isosurfaces`,
    /// so it stays visible even after switching to a different structure.
    /// The natural way to compose several `.cube` files sharing one
    /// geometry (several orbitals of one molecule) into a single image.
    pub(crate) fn keep_active_isosurface(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(iso) = self.structures.get(active).and_then(|s| s.isosurface.as_ref()) else { return };
        if !iso.show {
            self.show_warning("Isosurface is hidden — nothing to keep");
            return;
        }
        let mut vertices = Vec::new();
        let opacity = iso.opacity;
        if let Some(mesh) = &iso.cached_positive {
            push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.positive_color), opacity);
        }
        if iso.both_signs {
            if let Some(mesh) = &iso.cached_negative {
                push_isosurface_vertices(&mut vertices, mesh, color32_to_rgb(iso.negative_color), opacity);
            }
        }
        if vertices.is_empty() {
            self.show_warning("Nothing extracted yet — click Update surface first");
            return;
        }
        self.kept_isosurfaces.push(KeptIsosurface { vertices });
        self.show_status("Isosurface kept");
    }

    /// Removes every kept isosurface *and* hides the active structure's
    /// own live one — a full reset back to no isosurfaces shown at all.
    pub(crate) fn clean_isosurfaces(&mut self) {
        self.kept_isosurfaces.clear();
        if let Some(active) = self.active_structure {
            if let Some(iso) = self.structures.get_mut(active).and_then(|s| s.isosurface.as_mut()) {
                iso.show = false;
            }
        }
        self.rebuild_isosurface();
        self.show_status("Cleared all isosurfaces");
    }
}
