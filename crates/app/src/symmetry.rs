//! Point groups of the structures shown: worked out whenever the active
//! structure's geometry changes (`apost3dview_build::analyze`: ~10 ms for
//! 100 atoms; cubic and icosahedral molecules up to a second, once), shown in
//! the Molecule window for everyone, and offered in the Build window to
//! symmetrize, with the chosen group's elements drawn in the view.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use apost3dview_build::{Analysis, Candidate, Element, PROGRAM_TOLERANCE};
use apost3dview_core::Molecule;
use egui::{Color32, Pos2, Stroke};

use crate::app::App;

/// How close a higher group must be to be named next to the group.
pub(crate) const NEARBY_TOLERANCE: f64 = 0.1;

/// The symmetry Standard orientation follows: the highest group this close.
pub(crate) const ORIENTATION_TOLERANCE: f64 = 0.01;

/// A structure's point group: the analysis, and the geometry it was made
/// for.
#[derive(Default)]
pub(crate) struct SymmetryState {
    result: Option<(u64, Arc<Analysis>)>,
}

/// A fingerprint of the atoms and their positions.
pub(crate) fn geometry_key(molecule: &Molecule) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    molecule.atomic_numbers.hash(&mut hasher);
    for p in &molecule.positions {
        p.to_array().map(f32::to_bits).hash(&mut hasher);
    }
    hasher.finish()
}

impl App {
    /// Works out the active structure's point group when its geometry
    /// changed (not while atoms are dragged: at the end of the drag).
    pub(crate) fn update_symmetry(&mut self) {
        if self.build.drag.is_some() {
            return;
        }
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get_mut(i)) else { return };
        let key = geometry_key(&structure.molecule);
        if structure.symmetry.result.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let analysis = apost3dview_build::analyze(&structure.molecule.atomic_numbers, &structure.molecule.positions);
        structure.symmetry.result = Some((key, Arc::new(analysis)));
    }

    /// The analysis of structure `index` as it is now, once it is done.
    pub(crate) fn symmetry(&self, index: usize) -> Option<Arc<Analysis>> {
        let structure = self.structures.get(index)?;
        let key = geometry_key(&structure.molecule);
        structure.symmetry.result.as_ref().filter(|(k, _)| *k == key).map(|(_, a)| Arc::clone(a))
    }

    /// "C2v", or "C1 (C2v within 0.012 Å)", or "…" while it is worked out.
    pub(crate) fn point_group_text(&self, index: usize) -> String {
        let Some(analysis) = self.symmetry(index) else { return "\u{2026}".to_string() };
        let group = analysis.group_within(PROGRAM_TOLERANCE);
        match analysis.highest_above(group, NEARBY_TOLERANCE) {
            Some(near) => format!("{} ({} within {})", group.label, near.label, self.length_text(near.displacement)),
            None => group.label.clone(),
        }
    }

    /// A length in the distance unit, "0.012 Å", with enough decimals that
    /// a small move doesn't read as zero ("0.0004 Å").
    pub(crate) fn length_text(&self, angstrom: f64) -> String {
        let value = self.coordinate_unit.from_angstrom(angstrom);
        let decimals = if value <= 0.0 { 3 } else { ((-value.log10()).floor() as i32 + 1).clamp(3, 6) as usize };
        let unit = if self.coordinate_unit == apost3dview_core::LengthUnit::Angstrom { "\u{c5}" } else { "bohr" };
        format!("{value:.decimals$} {unit}")
    }

    /// Symmetrizes the active structure to `candidate` (an edit: copy, undo).
    pub(crate) fn symmetrize_to(&mut self, candidate: &Candidate) {
        let positions = candidate.symmetrized().to_vec();
        let label = candidate.label.clone();
        let moved = candidate.displacement;
        if self.edit(move |s| {
            if s.molecule.positions.len() != positions.len() {
                return Err("The structure changed: find its symmetry again.".into());
            }
            Ok(crate::build::Edited::Positions(positions))
        }) {
            let moved = self.length_text(moved);
            self.show_status(format!("Symmetrized to {label}: atoms moved up to {moved}"));
        }
    }

    /// Standard orientation (an edit): the center of nuclear charge at the
    /// origin, the main axis along z, as Gaussian does. The camera turns
    /// with the molecule, so the picture doesn't jump.
    pub(crate) fn standard_orientation(&mut self) {
        let Some(active) = self.active_structure else { return };
        let Some(analysis) = self.symmetry(active) else { return };
        // The symmetry it really has: the highest group within 0.01 Å (a
        // geometry from a calculation is rarely exact to 1e-4).
        let exact = analysis.group_within(PROGRAM_TOLERANCE);
        let group = analysis.highest_above(exact, ORIENTATION_TOLERANCE).unwrap_or(exact).clone();
        let before = self.structures[active].molecule.positions.clone();
        let z = self.structures[active].molecule.atomic_numbers.clone();
        let (rotation, center) = apost3dview_build::standard_frame(&z, &before, &group);
        let positions = apost3dview_build::standard_orientation(&z, &before, &group);
        if self.edit(move |_| Ok(crate::build::Edited::Positions(positions))) {
            // new = R (old - c): the camera (and the fill light's "up", fixed
            // in the coordinates) go the same way.
            let r = glam::DQuat::from_mat3(&rotation);
            let camera = &mut self.camera;
            camera.target = (rotation * (camera.target.as_dvec3() - center)).as_vec3();
            camera.orientation = (r * camera.orientation.as_dquat()).normalize().as_quat();
            camera.world_up = (rotation * camera.world_up.as_dvec3()).normalize().as_vec3();
            self.show_status(format!("Standard orientation for {}: main axis along z", group.label));
        }
    }

    /// Draws the elements of `candidate` over the view (`rect`): axes as
    /// labeled lines through the center, planes as translucent discs, the
    /// inversion center as a dot.
    pub(crate) fn paint_symmetry_elements(&self, painter: &egui::Painter, rect: egui::Rect, center: glam::DVec3, candidate: &Candidate) {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return };
        let (_, radius) = structure.molecule.bounding_sphere();
        let reach = f64::from(radius) * 1.05 + 0.3;
        let aspect = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };
        let screen = |p: glam::DVec3| crate::util::project_to_screen(&self.camera, aspect, rect, p.as_vec3());
        let axis_color = Color32::from_rgb(40, 130, 200);
        let plane_color = Color32::from_rgba_unmultiplied(40, 130, 200, 22);
        let plane_edge = Color32::from_rgba_unmultiplied(40, 130, 200, 170);
        let font = egui::FontId::proportional(15.0);
        for element in &candidate.elements {
            match element {
                Element::Plane { normal } => {
                    let u = normal.any_orthonormal_vector();
                    let v = normal.cross(u);
                    let points: Vec<Pos2> = (0..48)
                        .filter_map(|k| {
                            let a = std::f64::consts::TAU * f64::from(k) / 48.0;
                            screen(center + (u * a.cos() + v * a.sin()) * reach)
                        })
                        .collect();
                    if points.len() == 48 {
                        painter.add(egui::Shape::convex_polygon(points, plane_color, Stroke::new(1.5, plane_edge)));
                    }
                }
                Element::Axis { direction, order } => {
                    if let (Some(a), Some(b)) = (screen(center - *direction * reach), screen(center + *direction * reach)) {
                        painter.line_segment([a, b], Stroke::new(2.0, axis_color));
                        let name = if *order == 0 { "C\u{221e}".to_string() } else { format!("C{order}") };
                        // At the end away from the window column (on the right).
                        let end = if a.x < b.x { a } else { b };
                        painter.text(end + egui::vec2(-4.0, -4.0), egui::Align2::RIGHT_BOTTOM, name, font.clone(), axis_color);
                    }
                }
                Element::Inversion => {
                    if let Some(c) = screen(center) {
                        painter.circle_filled(c, 4.0, axis_color);
                        painter.text(c + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, "i", font.clone(), axis_color);
                    }
                }
            }
        }
    }
}
