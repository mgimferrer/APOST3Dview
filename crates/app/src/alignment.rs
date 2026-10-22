//! Aligning structures on selected atoms: related molecules from separate
//! calculations (Be and Mg complexes, two oxidation states) usually come
//! out of their programs turned differently, so the same camera shows
//! them differently. A structure aligned onto another gets a placement:
//! the rotation and translation that best put its selected atoms onto the
//! other's, in the same order (`apost3dview_core::superpose`).
//!
//! Nothing in the structure moves: its files, orbitals and results keep
//! their coordinates. The placement only changes how it is seen: with
//! "Keep the view when switching structures", a view of one structure is
//! carried into the other's coordinates through both placements, so the
//! matched atoms come out on the same spot of the screen, lit the same way
//! (the camera's `world_up` carries the fill light's "up" along).

use apost3dview_core::superpose;
use apost3dview_render::OrbitCamera;
use glam::{Quat, Vec3};

use crate::app::App;

/// Where a structure's coordinates sit in the shared frame:
/// `shared = rotation * own + translation`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placement {
    pub(crate) rotation: Quat,
    pub(crate) translation: Vec3,
    /// What it was aligned onto, for the Molecule window: the other
    /// structure's name, how many atoms, and the RMSD left (Å).
    pub(crate) onto: String,
    pub(crate) atoms: usize,
    pub(crate) rmsd: f64,
}

/// A structure's rotation and translation into the shared frame
/// (identity when it isn't aligned).
pub(crate) fn motion(placement: Option<&Placement>) -> (Quat, Vec3) {
    placement.map_or((Quat::IDENTITY, Vec3::ZERO), |p| (p.rotation, p.translation))
}

/// `camera`, seeing a structure placed by `from`, as the same view of a
/// structure placed by `to`: the shared frame looks the same through both.
pub(crate) fn carry_camera(camera: &OrbitCamera, from: (Quat, Vec3), to: (Quat, Vec3)) -> OrbitCamera {
    let ((ra, ta), (rb, tb)) = (from, to);
    let mut carried = *camera;
    carried.orientation = (rb.inverse() * (ra * camera.orientation)).normalize();
    carried.target = rb.inverse() * (ra * camera.target + ta - tb);
    carried.world_up = up_in(to);
    carried
}

/// Whether two cameras show the same view, up to rounding.
pub(crate) fn same_view(a: &OrbitCamera, b: &OrbitCamera) -> bool {
    let scale = a.distance.max(1.0);
    a.orientation.dot(b.orientation).abs() > 1.0 - 1e-6 && (a.target - b.target).length() < 1e-4 * scale && (a.distance - b.distance).abs() < 1e-5 * scale
}

/// The shared "up" in the coordinates of a structure placed by `motion`.
pub(crate) fn up_in(motion: (Quat, Vec3)) -> Vec3 {
    if motion.0 == Quat::IDENTITY { Vec3::Y } else { (motion.0.inverse() * Vec3::Y).normalize() }
}

impl App {
    /// The structures the active one can be aligned onto: those with as
    /// many atoms selected (at least three).
    pub(crate) fn alignment_candidates(&self) -> Vec<usize> {
        let Some(active) = self.active_structure else { return Vec::new() };
        let n = self.structures[active].selected_atoms.len();
        if n < 3 {
            return Vec::new();
        }
        (0..self.structures.len()).filter(|&s| s != active && self.structures[s].selected_atoms.len() == n).collect()
    }

    /// Aligns the active structure onto structure `reference`: its
    /// selected atoms onto the reference's, in click order. Turns "Keep the
    /// view" on and shows it as the reference was seen.
    pub(crate) fn align_active_onto(&mut self, reference: usize) {
        let Some(active) = self.active_structure else { return };
        if reference == active || reference >= self.structures.len() {
            return;
        }
        let (target, moving) = (&self.structures[reference], &self.structures[active]);
        let reference_motion = motion(target.placement.as_ref());
        let shared: Vec<Vec3> = target.selected_atoms.iter().map(|&a| reference_motion.0 * target.molecule.positions[a] + reference_motion.1).collect();
        let own: Vec<Vec3> = moving.selected_atoms.iter().map(|&a| moving.molecule.positions[a]).collect();
        let Some(fit) = superpose(&shared, &own) else {
            self.show_warning("Select at least three atoms here, and as many in the other structure, in the same order.");
            return;
        };
        let reference_view = target.view.unwrap_or_else(|| self.view_for(reference));
        let (name, onto) = (moving.label.clone(), target.label.clone());
        let placement = Placement { rotation: fit.rotation, translation: fit.translation, onto: onto.clone(), atoms: own.len(), rmsd: fit.rmsd };
        let new_motion = (placement.rotation, placement.translation);
        self.structures[active].placement = Some(placement);
        self.keep_view = true;
        self.camera = carry_camera(&reference_view, reference_motion, new_motion);
        self.show_status(format!("{name} aligned onto {onto} on {} atoms (RMSD {:.3} \u{c5}). Both are now seen the same way.", own.len(), fit.rmsd));
    }

    /// Undoes the active structure's alignment. The live camera is in the
    /// structure's own coordinates already, so the picture stays (only the
    /// fill light's "up" goes back to the structure's own +Y).
    pub(crate) fn undo_alignment(&mut self) {
        let Some(active) = self.active_structure else { return };
        if self.structures[active].placement.take().is_some() {
            self.camera.world_up = Vec3::Y;
        }
    }

    /// Where the active structure's atoms are on a `width` x `height`
    /// picture of the current view (pixels, top left origin) — for tests.
    pub fn atom_screen_positions(&self, width: f32, height: f32) -> Vec<[f32; 2]> {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return Vec::new() };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        structure
            .molecule
            .positions
            .iter()
            .filter_map(|p| crate::util::project_to_screen(&self.camera, width / height, rect, *p).map(|q| [q.x, q.y]))
            .collect()
    }
}
