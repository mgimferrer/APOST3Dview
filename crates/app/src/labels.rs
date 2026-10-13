//! 3D text labels (atom labels, measurement labels): layout helpers and
//! the export-side label builder.

use apost3dview_core::{element_data, measure};
use apost3dview_render::{glyph_scale_for_font_size, glyph_scale_for_world_size, layout_label, GlyphAtlas, GlyphInstance, OrbitCamera};
use glam::Vec3;

use crate::app::App;
use crate::state::AtomLabelMode;
use crate::util::{color32_to_rgb, format_measurement, measurement_anchor};

/// SDF edge-threshold constants controlling stroke weight (see
/// `GlyphInstance::edge_bias`) — lower reads as thicker, since more of the
/// distance field counts as "inside" the glyph. Sampling the field once at
/// a shifted threshold gives a uniformly thicker, still crisply
/// antialiased stroke; the previous approach (stamping several offset
/// copies of the glyph) visibly pixelated once labels could be zoomed in
/// this close, since each copy's edge lands on a different sub-pixel and
/// their overlap dithers instead of blending.
pub(crate) const EDGE_BIAS_NORMAL: f32 = 0.5;
pub(crate) const EDGE_BIAS_BOLD: f32 = 0.38;
/// Atom labels have no weight toggle — they're always rendered noticeably
/// thicker than plain body text so a single digit reads clearly against a
/// CPK-colored sphere.
pub(crate) const EDGE_BIAS_ATOM_LABEL: f32 = 0.30;

/// Lays out `text` as 3D glyph instances anchored at `anchor`, appending to
/// `instances`.
pub(crate) fn push_label(instances: &mut Vec<GlyphInstance>, atlas: &GlyphAtlas, text: &str, anchor: Vec3, scale: f32, color: [f32; 3], edge_bias: f32) {
    instances.extend(layout_label(atlas, text, anchor, scale, color, edge_bias));
}

impl App {
    /// Builds the same atom/measurement label geometry the live view
    /// does (see the central-panel label-building block), sized for
    /// `target_height_px` — the export's own output height, not whatever
    /// the on-screen viewport happens to be, so measurement labels (which
    /// hold a constant *apparent* size relative to the render they're in)
    /// come out proportioned exactly like the live view once the export
    /// is viewed at its own native resolution. Deliberately a separate,
    /// simplified copy rather than sharing code with the live-view block:
    /// that block also does interactive drag hit-testing (needs `ui`,
    /// mutates measurement state), which a one-shot export has no use for.
    pub(crate) fn build_export_label_instances(&self, active: usize, camera: &OrbitCamera, target_height_px: f32) -> Vec<GlyphInstance> {
        let mut label_instances: Vec<GlyphInstance> = Vec::new();
        let structure = &self.structures[active];

        if self.atom_label_mode != AtomLabelMode::None {
            let color = color32_to_rgb(self.atom_label_style.text_color);
            for (index, (&z, &position)) in structure.molecule.atomic_numbers.iter().zip(&structure.molecule.positions).enumerate() {
                if structure.hidden_atoms.contains(&index) {
                    continue;
                }
                let radius = element_data(z).vdw_radius * self.material.atom_scale;
                let scale = glyph_scale_for_world_size(radius * self.atom_label_style.relative_size);
                let text = match self.atom_label_mode {
                    AtomLabelMode::Number => format!("{}", index + 1),
                    AtomLabelMode::Type => element_data(z).symbol.to_string(),
                    AtomLabelMode::NumberType => format!("{}{}", element_data(z).symbol, index + 1),
                    AtomLabelMode::None => unreachable!(),
                };
                let to_camera = (camera.eye() - position).normalize_or_zero();
                let label_anchor = position + to_camera * (radius * 1.15 + 0.02);
                push_label(&mut label_instances, &self.glyph_atlas, &text, label_anchor, scale, color, EDGE_BIAS_ATOM_LABEL);
            }
        }

        let (camera_right, camera_up) = camera.screen_basis();
        let measurement_color = color32_to_rgb(self.measurement_style.text_color);
        for measurement in &structure.measurements {
            let anchor = measurement_anchor(&structure.molecule, measurement.kind);
            let distance = (anchor - camera.eye()).length();
            let world_per_pixel = camera.world_units_per_pixel(distance, target_height_px);
            let world_offset = camera_right * (measurement.label_offset.x * world_per_pixel)
                - camera_up * (measurement.label_offset.y * world_per_pixel);
            let final_anchor = anchor + world_offset;

            let value = measure(&structure.molecule, measurement.kind);
            let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
            let scale = glyph_scale_for_font_size(self.measurement_style.font_size, world_per_pixel);
            let edge_bias = if self.measurement_style.bold { EDGE_BIAS_BOLD } else { EDGE_BIAS_NORMAL };
            push_label(&mut label_instances, &self.glyph_atlas, &text, final_anchor, scale, measurement_color, edge_bias);
        }

        label_instances
    }
}
