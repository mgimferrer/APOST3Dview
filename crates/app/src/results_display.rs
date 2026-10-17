//! Drawing the APOST-3D results on the molecule: envelopes, halos, value
//! labels, legend, bond thickness (see `results_view` for what is shown).

use apost3dview_core::Quantity;
use apost3dview_render::{glyph_scale_for_font_size, layout_label, GlyphInstance, Halo, Legend, OrbitCamera};
use egui::{Pos2, Rect};

use crate::app::App;
use crate::results_view::{
    bond_scales, build_scene, envelope_vertices, focus_target, fragment_envelopes, halo_instances, label_position, ResultsScene,
};
use crate::state::LoadedStructure;

/// Labels' stroke weight: as the measurement labels' bold.
const VALUE_LABEL_EDGE: f32 = 0.40;

impl App {
    /// The active structure's results scene, if it has results and a view.
    pub(crate) fn results_scene(&self, structure: &LoadedStructure) -> Option<ResultsScene> {
        let attachment = structure.apost.as_ref()?;
        let results = attachment.results.as_ref().ok()?;
        let view = attachment.view.as_ref()?;
        Some(build_scene(results, &structure.molecule, view, &self.color_ranges))
    }

    pub(crate) fn bond_scales(&self, structure: &LoadedStructure) -> Option<Vec<f32>> {
        let attachment = structure.apost.as_ref()?;
        let view = attachment.view.as_ref()?;
        if !view.bond_thickness {
            return None;
        }
        bond_scales(attachment.results.as_ref().ok()?, &structure.molecule)
    }

    /// Rebuilds the envelopes and halos (and the bonds, for their
    /// thickness) when anything they depend on changed: the active
    /// structure, what is shown, the color ranges, the atom size, the
    /// hidden atoms. Waits while a slider is being dragged.
    pub(crate) fn update_results_display(&mut self, ctx: &egui::Context) {
        let signature = self.active_structure.and_then(|i| self.structures.get(i)).map(|s| {
            let mut hidden: Vec<usize> = s.hidden_atoms.iter().copied().collect();
            hidden.sort_unstable();
            let view = s.apost.as_ref().and_then(|a| a.view.clone());
            format!("{:?}|{view:?}|{}|{hidden:?}|{:?}", self.active_structure, self.material.atom_scale, self.color_ranges)
        });
        let signature = signature.unwrap_or_default();
        if signature == self.results_signature {
            return;
        }
        let scale_changed = !self.results_signature.is_empty() && ctx.dragged_id().is_some();
        if scale_changed {
            return;
        }
        self.results_signature = signature;
        self.rebuild_results_display();
        self.rebuild_geometry();
    }

    fn rebuild_results_display(&mut self) {
        let atom_scale = self.material.atom_scale;
        let mut vertices = Vec::new();
        let mut ranges = Vec::new();
        let mut halos = Vec::new();
        if let Some(active) = self.active_structure {
            let scene = self.structures.get(active).and_then(|s| self.results_scene(s));
            let structure = &mut self.structures[active];
            if let (Some(scene), Some(attachment)) = (scene, structure.apost.as_mut()) {
                if let Ok(results) = &attachment.results {
                    if !scene.envelopes.is_empty() {
                        let mut hidden: Vec<usize> = structure.hidden_atoms.iter().copied().collect();
                        hidden.sort_unstable();
                        let key = (atom_scale.to_bits(), hidden);
                        if attachment.envelope_cache.as_ref().map(|(k, _)| k) != Some(&key) {
                            let meshes = fragment_envelopes(results, &structure.molecule, atom_scale, &structure.hidden_atoms);
                            attachment.envelope_cache = Some((key, meshes));
                        }
                        let meshes = &attachment.envelope_cache.as_ref().unwrap().1;
                        (vertices, ranges) = envelope_vertices(&scene, meshes);
                    }
                    halos = halo_instances(&scene, &structure.molecule, atom_scale, &structure.hidden_atoms);

                }
            }
        }
        self.with_resources(|resources| {
            resources.update_envelopes(&self.render_state.device, &vertices, &ranges);
            resources.update_halos(&self.render_state.device, &halos);
        });
    }

    /// Value labels for the active structure, sized for a view
    /// `viewport_height_px` tall, drawn over everything.
    pub(crate) fn value_label_instances(&self, camera: &OrbitCamera, viewport_height_px: f32) -> Vec<GlyphInstance> {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return Vec::new() };
        let Some(scene) = self.results_scene(structure) else { return Vec::new() };
        let (Some(attachment), Some(view)) = (structure.apost.as_ref(), structure.apost.as_ref().and_then(|a| a.view.as_ref())) else {
            return Vec::new();
        };
        let Ok(results) = &attachment.results else { return Vec::new() };
        let (ink, halo) = self.label_colors();
        let eye = camera.eye();
        let mut glyphs = Vec::new();
        for label in &scene.labels {
            let on_atom = matches!(label.anchor, crate::results_view::LabelAnchor::Atom(_));
            if let crate::results_view::LabelAnchor::Atom(a) = label.anchor {
                if structure.hidden_atoms.contains(&a) {
                    continue;
                }
            }
            let Some(position) = label_position(label.anchor, results, &structure.molecule, self.material.atom_scale, eye) else { continue };
            let px = if on_atom { view.label_size * 0.72 } else { view.label_size };
            let scale = glyph_scale_for_font_size(px, camera.world_units_per_pixel((position - eye).length(), viewport_height_px));
            let color = if label.color == [0.0; 3] { ink } else { label.color };
            let mut instances = layout_label(&self.glyph_atlas, &label.text, position, scale, color, VALUE_LABEL_EDGE, Halo { color: halo, width: 0.2 });
            instances.iter_mut().for_each(|g| g.on_top = 1.0);
            glyphs.extend(instances);
        }
        glyphs
    }

    /// Label ink and halo for the current background: dark text with a
    /// background-colored outline on light backgrounds, and the reverse.
    fn label_colors(&self) -> ([f32; 3], [f32; 3]) {
        let [r, g, b] = self.material.background;
        let light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5;
        if light { ([0.09, 0.1, 0.12], [r, g, b]) } else { ([0.95, 0.95, 0.96], [r, g, b]) }
    }

    pub(crate) fn results_legend(&self) -> Option<Legend> {
        let structure = self.active_structure.and_then(|i| self.structures.get(i))?;
        self.results_scene(structure)?.legend
    }

    /// The legend drawn into an exported image's pixels.
    pub(crate) fn draw_legend_into(&self, pixels: &mut [u8], width: u32, height: u32) {
        if let Some(legend) = self.results_legend() {
            let ink = self.label_colors().0.map(|c| (c * 255.0) as u8);
            apost3dview_render::draw_legend(pixels, width, height, &legend, ink);
        }
    }

    /// The legend in the live view, bottom left, the same as in exports.
    pub(crate) fn paint_legend(&self, painter: &egui::Painter, rect: Rect) {
        let Some(legend) = self.results_legend() else { return };
        let ink = self.label_colors().0;
        let ink = crate::util::rgb_to_color32(ink);
        let (bar_w, bar_h) = (240.0, 11.0);
        let origin = Pos2::new(rect.left() + 24.0, rect.bottom() - 52.0);
        let mut mesh = egui::Mesh::default();
        let steps = 48;
        for i in 0..steps {
            let (u0, u1) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
            let c = legend.scale.bar_color((u0 + u1) / 2.0);
            let color = crate::util::rgb_to_color32(c);
            mesh.add_colored_rect(Rect::from_min_max(Pos2::new(origin.x + u0 * bar_w, origin.y), Pos2::new(origin.x + u1 * bar_w, origin.y + bar_h)), color);
        }
        painter.add(egui::Shape::mesh(mesh));
        let font = egui::FontId::proportional(13.0);
        painter.text(Pos2::new(origin.x, origin.y - 4.0), egui::Align2::LEFT_BOTTOM, &legend.title, egui::FontId::proportional(14.0), ink);
        let (lo, mid, hi) = legend.scale.bar_labels(legend.decimals);
        let y = origin.y + bar_h + 3.0;
        painter.text(Pos2::new(origin.x, y), egui::Align2::LEFT_TOP, lo, font.clone(), ink);
        if let Some(mid) = mid {
            painter.text(Pos2::new(origin.x + bar_w / 2.0, y), egui::Align2::CENTER_TOP, mid, font.clone(), ink);
        }
        painter.text(Pos2::new(origin.x + bar_w, y), egui::Align2::RIGHT_TOP, hi, font, ink);
    }

    /// When a pair quantity is shown, a click on an atom focuses on it (or
    /// on its fragment) instead of selecting it. Returns whether the click
    /// was used that way.
    pub(crate) fn focus_on_clicked_atom(&mut self, atom: usize) -> bool {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return false };
        let Some(attachment) = structure.apost.as_mut() else { return false };
        let Ok(results) = &attachment.results else { return false };
        let Some(view) = attachment.view.as_mut() else { return false };
        match focus_target(results, view) {
            Some(true) => match results.fragments().iter().position(|f| f.atoms.contains(&(atom + 1))) {
                Some(fragment) => {
                    view.focus = fragment;
                    true
                }
                None => false,
            },
            Some(false) => {
                view.focus = atom;
                true
            }
            None => false,
        }
    }

    /// The values of every atom-pair quantity for two atoms (0-based), as
    /// (entry, value) — for the two-atom selection.
    pub(crate) fn pair_values(&self, a: usize, b: usize) -> Vec<(apost3dview_core::IndexEntry, f64)> {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return Vec::new() };
        let Some(Ok(results)) = structure.apost.as_ref().map(|a| &a.results) else { return Vec::new() };
        results
            .entries(apost3dview_core::QuantityKind::AtomMatrix)
            .filter_map(|entry| match results.quantity(entry) {
                Ok(Quantity::Matrix(m)) => m.get(a, b).map(|v| (entry.clone(), v)),
                _ => None,
            })
            .collect()
    }

    /// Shows `what` on the active structure's molecule: "nothing",
    /// "fragments", or a quantity's index id. False when there are no
    /// results or no such quantity. (Tests use it; so could scripting.)
    pub fn show_on_molecule(&mut self, what: &str) -> bool {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return false };
        let Some(attachment) = structure.apost.as_mut() else { return false };
        let Ok(results) = &attachment.results else { return false };
        let Some(view) = attachment.view.as_mut() else { return false };
        view.shown = match what {
            "nothing" => crate::results_view::Shown::Nothing,
            "fragments" => crate::results_view::Shown::Fragments,
            id if results.entry(id).is_some() => crate::results_view::Shown::Quantity(id.to_string()),
            _ => return false,
        };
        view.focus = 0;
        true
    }

    /// The legend's title for what is shown now, if anything has one.
    pub fn legend_title(&self) -> Option<String> {
        self.results_legend().map(|l| l.title)
    }

    /// The legend's end values as printed (left, right), if shown.
    pub fn legend_bounds(&self) -> Option<(String, String)> {
        self.results_legend().map(|l| {
            let (lo, _, hi) = l.scale.bar_labels(4);
            (lo, hi)
        })
    }
}
