//! The labels drawn over the molecule — atom names, measurements, APOST-3D
//! values — built the same way for the live view and for exports
//! (`App::scene_labels`), and placed so they never overlap
//! (`label_layout`).
//!
//! Since 2026-10-19 (item 4 of the improvement backlog) every label is
//! drawn on top, whole: a label is either shown complete or not at all,
//! never cut by a nearer atom. An atom's name or value is shown only when
//! a fair part of the atom is visible (`atom_visible_fraction`);
//! measurements always.

use apost3dview_core::{element_data, measure, MeasurementKind, Molecule};
use apost3dview_render::{
    atom_visible_fraction, glyph_scale_for_font_size, glyph_scale_for_world_size, label_extent, layout_label, GlyphInstance, Halo, OrbitCamera,
};
use egui::{Pos2, Rect, Vec2};
use glam::Vec3;

use crate::app::App;
use crate::label_layout::{place, Request};
use crate::results_view::{label_position, LabelAnchor, FOCUS_COLOR};
use crate::state::AtomLabelMode;
use crate::util::{color32_to_rgb, format_measurement, project_to_screen};

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
/// APOST-3D values: a little bolder than plain text, outlined.
pub(crate) const VALUE_LABEL_EDGE: f32 = 0.40;
/// The share of an atom that must be visible for its label to show.
const MIN_VISIBLE_FRACTION: f32 = 0.3;
/// A name above a value is this much smaller than the value.
const NAME_OVER_VALUE: f32 = 0.8;
/// An atom name may be this much wider than its atom before it shrinks,
/// and shrinks at most to this fraction of its size.
const NAME_WIDTH_PER_DIAMETER: f32 = 1.6;
const NAME_LEAST_SHRINK: f32 = 0.6;

/// Every label over the molecule, placed: the glyphs to draw, where each
/// measurement label landed (for dragging it), and every label's text and
/// screen box (for tests).
pub(crate) struct SceneLabels {
    pub(crate) glyphs: Vec<GlyphInstance>,
    pub(crate) measurement_hits: Vec<(usize, Rect)>,
    pub(crate) placed: Vec<(String, Rect)>,
}

/// One line of a label.
struct Line {
    text: String,
    /// World size per raster pixel (`layout_label`'s scale).
    scale: f32,
    color: [f32; 3],
    edge_bias: f32,
    halo: Halo,
}

/// A label before placement: where it is attached, its lines (top to
/// bottom), and how the placement may treat it.
struct Pending {
    anchor: Vec3,
    /// From the anchor's place on screen (pixels, y down): a
    /// measurement's default place beside its bond, plus the user's drag.
    shift: Vec2,
    lines: Vec<Line>,
    world_per_px: f32,
    order: f32,
    may_move: bool,
    required: bool,
    measurement: Option<usize>,
}

impl App {
    /// Every label over the molecule of structure `active`, seen by
    /// `camera` in a view of `viewport` (pixels: the live viewport, or
    /// `0, 0, width, height` for an export), placed so none overlaps, as
    /// glyphs drawn on top. Also where each measurement label landed on
    /// screen, for dragging it in the live view.
    pub(crate) fn scene_labels(&self, active: usize, camera: &OrbitCamera, viewport: Rect) -> SceneLabels {
        let structure = &self.structures[active];
        let molecule = &structure.molecule;
        let aspect = viewport.width() / viewport.height().max(1.0);
        let eye = camera.eye();
        let (right, up) = camera.screen_basis();
        let world_per_px = |p: Vec3| camera.world_units_per_pixel((p - eye).length(), viewport.height());
        let project = |p: Vec3| project_to_screen(camera, aspect, viewport, p);
        // An atom counts as visible when a fair part of it can be seen
        // (a hydrogen in front of a carbon's center doesn't hide it).
        let visible = |a: usize| {
            atom_visible_fraction(molecule, eye, a, self.material.atom_scale, self.material.bond_radius, &structure.hidden_atoms, &structure.hidden_bonds)
                >= MIN_VISIBLE_FRACTION
        };
        let (ink, background) = self.label_colors();
        let mut pending: Vec<Pending> = Vec::new();

        // Measurements: beside what they measure, wherever it is.
        let measurement_color = color32_to_rgb(self.measurement_style.text_color);
        let molecule_center = project(molecule.bounding_sphere().0).unwrap_or(viewport.center());
        for (index, measurement) in structure.measurements.iter().enumerate() {
            let anchor = measurement_anchor(molecule, measurement.kind);
            let wpp = world_per_px(anchor);
            let px = self.measurement_style.font_size;
            let text = format_measurement(measurement.kind, measure(molecule, measurement.kind), self.coordinate_unit, self.measurement_style.decimal_places);
            let scale = glyph_scale_for_font_size(px, wpp);
            let (width, height) = label_extent(&self.glyph_atlas, &text, scale);
            let size = Vec2::new(width, height) / wpp;
            // Clear of the bond (or the vertex atom) by a few pixels.
            let clearance = match measurement.kind {
                MeasurementKind::Angle(_, v, _) => crate::util::drawn_radius(molecule.atomic_numbers[v], self.material.atom_scale) / wpp,
                _ => self.material.bond_radius / wpp,
            } + 4.0;
            let shift = measurement_shift(measurement.kind, molecule, &project, molecule_center, size, clearance) + measurement.label_offset;
            pending.push(Pending {
                anchor,
                shift,
                lines: vec![Line {
                    text,
                    scale,
                    color: measurement_color,
                    edge_bias: if self.measurement_style.bold { EDGE_BIAS_BOLD } else { EDGE_BIAS_NORMAL },
                    halo: Halo { color: background, width: 0.2 },
                }],
                world_per_px: wpp,
                order: 0.0,
                may_move: false,
                required: true,
                measurement: Some(index),
            });
        }

        // APOST-3D values, with the atom's or fragment's name above when
        // asked for; they then replace the atom names below.
        let mut values_on_atoms = false;
        if let (Some(scene), Some(attachment)) = (self.results_scene(structure), structure.apost.as_ref())
            && let (Ok(results), Some(view)) = (&attachment.results, attachment.view.as_ref())
        {
            values_on_atoms = scene.labels.iter().any(|l| matches!(l.anchor, LabelAnchor::Atom(_)));
            for label in &scene.labels {
                let on_atom = match label.anchor {
                    LabelAnchor::Atom(a) if structure.hidden_atoms.contains(&a) || !visible(a) => continue,
                    LabelAnchor::Atom(_) => true,
                    LabelAnchor::Fragment(_) => false,
                };
                let Some(position) = label_position(label.anchor, results, molecule, self.material.atom_scale, eye) else { continue };
                let wpp = world_per_px(position);
                let px = if on_atom { view.label_size * 0.72 } else { view.label_size };
                let focus = label.color == FOCUS_COLOR;
                let color = if label.color == [0.0; 3] { ink } else { label.color };
                let line = |text: String, px: f32, color: [f32; 3]| Line {
                    text,
                    scale: glyph_scale_for_font_size(px, wpp),
                    color,
                    edge_bias: VALUE_LABEL_EDGE,
                    halo: Halo { color: background, width: 0.2 },
                };
                let mut lines = Vec::new();
                if view.names && !focus {
                    let name = match label.anchor {
                        LabelAnchor::Atom(a) => self.atom_name_text(molecule, a, true),
                        LabelAnchor::Fragment(f) => results.fragments().get(f).map(|f| crate::names::fragment_name(f, &molecule.atomic_numbers)).unwrap_or_default(),
                    };
                    lines.push(line(name, px * NAME_OVER_VALUE, ink));
                }
                lines.push(line(label.text.clone(), px, color));
                pending.push(Pending {
                    anchor: position,
                    shift: Vec2::ZERO,
                    lines,
                    world_per_px: wpp,
                    // The focused one first, then the nearest.
                    order: if focus { -1.0 } else { (position - eye).length() },
                    may_move: true,
                    required: false,
                    measurement: None,
                });
            }
        }

        // Atom names, sized with their atom, shown whole on visible atoms
        // (with values on the atoms, the name goes with the value, or not
        // at all).
        if self.atom_label_mode != AtomLabelMode::None && !values_on_atoms {
            let color = color32_to_rgb(self.atom_label_style.text_color);
            for (index, (&z, &position)) in molecule.atomic_numbers.iter().zip(&molecule.positions).enumerate() {
                if structure.hidden_atoms.contains(&index) || !visible(index) {
                    continue;
                }
                let radius = crate::util::drawn_radius(z, self.material.atom_scale);
                let text = self.atom_name_text(molecule, index, false);
                let mut scale = glyph_scale_for_world_size(radius * self.atom_label_style.relative_size);
                // Shrunk a little to fit a small atom (a hydrogen with a
                // two-digit number).
                let (width, _) = label_extent(&self.glyph_atlas, &text, scale);
                let room = 2.0 * radius * NAME_WIDTH_PER_DIAMETER;
                if width > room {
                    scale *= (room / width).max(NAME_LEAST_SHRINK);
                }
                let anchor = position + (eye - position).normalize_or_zero() * (radius * 1.15 + 0.02);
                pending.push(Pending {
                    anchor,
                    shift: Vec2::ZERO,
                    lines: vec![Line { text, scale, color, edge_bias: EDGE_BIAS_ATOM_LABEL, halo: Halo::NONE }],
                    world_per_px: world_per_px(anchor),
                    order: (anchor - eye).length(),
                    may_move: false,
                    required: false,
                    measurement: None,
                });
            }
        }

        // Placement on screen, then glyphs at the placed spots.
        let sizes: Vec<Vec2> = pending
            .iter()
            .map(|p| {
                p.lines.iter().fold(Vec2::ZERO, |size, line| {
                    let (w, h) = label_extent(&self.glyph_atlas, &line.text, line.scale);
                    Vec2::new(size.x.max(w / p.world_per_px), size.y + h / p.world_per_px)
                })
            })
            .collect();
        let requests: Vec<Option<Request>> = pending
            .iter()
            .zip(&sizes)
            .map(|(p, &size)| {
                project(p.anchor).map(|center| Request { center: center + p.shift, size, order: p.order, may_move: p.may_move, required: p.required })
            })
            .collect();
        let shown: Vec<usize> = (0..pending.len()).filter(|&i| requests[i].is_some()).collect();
        let placed = place(&shown.iter().map(|&i| requests[i].unwrap()).collect::<Vec<_>>());

        let mut glyphs = Vec::new();
        let mut hits = Vec::new();
        let mut placed_boxes = Vec::new();
        for (&i, offset) in shown.iter().zip(placed) {
            let Some(offset) = offset else { continue };
            let (p, request) = (&pending[i], requests[i].unwrap());
            let shift = p.shift + offset;
            let center = p.anchor + right * (shift.x * p.world_per_px) - up * (shift.y * p.world_per_px);
            let mut y = -sizes[i].y / 2.0;
            for line in &p.lines {
                let (_, h) = label_extent(&self.glyph_atlas, &line.text, line.scale);
                let h = h / p.world_per_px;
                let line_center = center - up * ((y + h / 2.0) * p.world_per_px);
                y += h;
                let mut instances = layout_label(&self.glyph_atlas, &line.text, line_center, line.scale, line.color, line.edge_bias, line.halo);
                instances.iter_mut().for_each(|g| g.on_top = 1.0);
                glyphs.extend(instances);
            }
            let rect = Rect::from_center_size(request.center + offset, sizes[i]);
            if let Some(index) = p.measurement {
                hits.push((index, rect));
            }
            placed_boxes.push((p.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"), rect));
        }
        SceneLabels { glyphs, measurement_hits: hits, placed: placed_boxes }
    }

    /// The labels an export of `width` x `height` shows, each as its text
    /// (lines joined by newlines) and its box on the image — for tests.
    pub fn placed_labels(&self, width: f32, height: f32) -> Vec<(String, Rect)> {
        let Some(active) = self.active_structure else { return Vec::new() };
        self.scene_labels(active, &self.camera, Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))).placed
    }

    /// An atom's name as the Molecule window's Labels setting asks
    /// ("12", "C", "C12"); with `fallback`, "C12" when that setting is
    /// None (a name above a value always has one).
    fn atom_name_text(&self, molecule: &Molecule, index: usize, fallback: bool) -> String {
        let z = molecule.atomic_numbers[index];
        match self.atom_label_mode {
            AtomLabelMode::Number => format!("{}", index + 1),
            AtomLabelMode::Type => element_data(z).symbol.to_string(),
            AtomLabelMode::NumberType => crate::names::atom_name(&molecule.atomic_numbers, index),
            AtomLabelMode::None if fallback => crate::names::atom_name(&molecule.atomic_numbers, index),
            AtomLabelMode::None => String::new(),
        }
    }

    /// The labels of an export (or a GIF frame) of `width` x `height`.
    pub(crate) fn build_export_label_instances(&self, active: usize, camera: &OrbitCamera, width: f32, height: f32) -> Vec<GlyphInstance> {
        self.scene_labels(active, camera, Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))).glyphs
    }
}

/// Where a measurement is attached: a distance's bond middle, an angle's
/// vertex, a dihedral's central bond middle.
fn measurement_anchor(molecule: &Molecule, kind: MeasurementKind) -> Vec3 {
    let p = |i: usize| molecule.positions[i];
    match kind {
        MeasurementKind::Distance(a, b) | MeasurementKind::Dihedral(_, a, b, _) => (p(a) + p(b)) / 2.0,
        MeasurementKind::Angle(_, v, _) => p(v),
    }
}

/// A measurement label's place on screen from its anchor (pixels, y
/// down): beside a bond, on the side away from the molecule's center; for
/// an angle, inside it along the bisector, clear of the vertex atom. The
/// label (`size`) keeps `clearance` pixels from the bond's axis or the
/// vertex's center.
fn measurement_shift(
    kind: MeasurementKind,
    molecule: &Molecule,
    project: &dyn Fn(Vec3) -> Option<Pos2>,
    molecule_center: Pos2,
    size: Vec2,
    clearance: f32,
) -> Vec2 {
    // How far the label's center must be, along `direction`, for its box
    // to stay `clearance` away.
    let away = |direction: Vec2| direction * (direction.x.abs() * size.x / 2.0 + direction.y.abs() * size.y / 2.0 + clearance);
    let screen = |i: usize| project(molecule.positions[i]);
    let beside = |a: usize, b: usize| -> Option<Vec2> {
        let (pa, pb) = (screen(a)?, screen(b)?);
        let along = (pb - pa).normalized();
        let mut across = if along.length() > 0.5 { Vec2::new(-along.y, along.x) } else { Vec2::Y };
        let middle = pa + (pb - pa) / 2.0;
        if (middle - molecule_center).dot(across) < 0.0 {
            across = -across;
        }
        Some(away(across))
    };
    let shift = match kind {
        MeasurementKind::Distance(a, b) | MeasurementKind::Dihedral(_, a, b, _) => beside(a, b),
        MeasurementKind::Angle(a, v, b) => (|| {
            let (pa, pv, pb) = (screen(a)?, screen(v)?, screen(b)?);
            let bisector = (pa - pv).normalized() + (pb - pv).normalized();
            let direction = if bisector.length() > 0.1 { bisector.normalized() } else { Vec2::new(-(pa - pv).normalized().y, (pa - pv).normalized().x) };
            Some(away(direction))
        })(),
    };
    shift.unwrap_or(Vec2::ZERO)
}
