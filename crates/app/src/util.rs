//! Small helpers shared by several windows.

use apost3dview_core::{LengthUnit, MeasurementKind, Molecule};
use apost3dview_render::OrbitCamera;
use egui::Color32;
use glam::{Vec3, Vec4};

pub(crate) fn toggle_selected(list: &mut Vec<usize>, index: usize) {
    if let Some(pos) = list.iter().position(|&i| i == index) {
        list.remove(pos);
    } else {
        list.push(index);
    }
}

pub(crate) fn find_bond_between(molecule: &Molecule, a: usize, b: usize) -> Option<usize> {
    molecule.bonds.iter().position(|bond| (bond.atom_a == a && bond.atom_b == b) || (bond.atom_a == b && bond.atom_b == a))
}

pub(crate) fn format_measurement(kind: MeasurementKind, value: f32, unit: LengthUnit, decimals: usize) -> String {
    match kind {
        MeasurementKind::Distance(..) => {
            let converted = unit.from_angstrom(value as f64);
            let unit_label = match unit {
                LengthUnit::Angstrom => "Å",
                LengthUnit::Bohr => "a.u.",
            };
            format!("{converted:.decimals$} {unit_label}")
        }
        MeasurementKind::Angle(..) | MeasurementKind::Dihedral(..) => format!("{value:.decimals$}\u{b0}"),
    }
}

/// "C1-N3-O5" style tag for the measurement list, so entries with
/// identical values (a common occurrence — symmetric structures, several
/// similar bonds) are still distinguishable at a glance.
pub(crate) fn format_measurement_atoms(molecule: &Molecule, kind: MeasurementKind) -> String {
    kind.atoms()
        .iter()
        .map(|&i| crate::names::atom_name(&molecule.atomic_numbers, i))
        .collect::<Vec<_>>()
        .join("-")
}

/// Projects a world-space point through the camera into screen pixels
/// within `rect`. `None` if it's behind the camera.
pub(crate) fn project_to_screen(camera: &OrbitCamera, aspect_ratio: f32, rect: egui::Rect, world_pos: Vec3) -> Option<egui::Pos2> {
    let clip = camera.view_projection_matrix(aspect_ratio) * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
    if clip.w <= 1e-4 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    let x = rect.left() + (ndc_x * 0.5 + 0.5) * rect.width();
    let y = rect.top() + (1.0 - (ndc_y * 0.5 + 0.5)) * rect.height();
    Some(egui::pos2(x, y))
}

pub(crate) fn color32_to_rgb(color: Color32) -> [f32; 3] {
    [color.r() as f32 / 255.0, color.g() as f32 / 255.0, color.b() as f32 / 255.0]
}

/// A path's file name for messages and labels ("" if it has none).
pub(crate) fn file_display_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// "Bond order" from "bond order".
pub(crate) fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// An atom's drawn radius: its van der Waals radius times the atom size.
pub(crate) fn drawn_radius(atomic_number: u32, atom_scale: f32) -> f32 {
    apost3dview_core::element_data(atomic_number).vdw_radius * atom_scale
}

/// The reverse: an sRGB color in [0, 1] as an egui color.
pub(crate) fn rgb_to_color32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

/// A small square of `color`, `size` points wide, in the current row.
pub(crate) fn swatch(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

/// Whether two molecules describe the same geometry (same atoms, same
/// positions within a small tolerance) — used to decide whether switching
/// the active structure should reframe the camera. `.cube` files sharing
/// one `.xyz`/`.fchk` geometry (several orbitals of one molecule) are the
/// motivating case: switching between them should feel like changing a
/// setting, not opening a different structure.
pub(crate) fn molecules_share_geometry(a: &Molecule, b: &Molecule) -> bool {
    if a.atomic_numbers != b.atomic_numbers {
        return false;
    }
    a.positions.iter().zip(&b.positions).all(|(pa, pb)| pa.distance(*pb) < 1e-3)
}
