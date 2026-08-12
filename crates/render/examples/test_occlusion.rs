use apost3dview_core::Molecule;
use apost3dview_render::{is_atom_visible, OrbitCamera};
use glam::Vec3;
use std::collections::HashSet;

fn main() {
    // Synthetic: two atoms directly in line with the camera, one behind
    // the other. The far one should be occluded.
    let molecule = Molecule::from_atoms(vec![6, 6], vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -3.0)]);
    let camera_eye = Vec3::new(0.0, 0.0, 10.0);
    let empty = HashSet::new();
    let near_visible = is_atom_visible(&molecule, camera_eye, 0, 0.5, 0.15, &empty, &empty);
    let far_visible = is_atom_visible(&molecule, camera_eye, 1, 0.5, 0.15, &empty, &empty);
    println!("synthetic: near atom visible={near_visible} (expect true), far atom visible={far_visible} (expect false)");
    assert!(near_visible, "near atom should be visible");
    assert!(!far_visible, "far atom should be occluded by the near one");

    // Real molecule, default camera framing: count how many of the 83
    // atoms are visible — should be a meaningful subset, not all-or-none.
    let path = std::env::args().nth(1).expect("usage: test_occlusion <path.fchk>");
    let real = Molecule::from_fchk(std::path::Path::new(&path)).expect("parse failed");
    let mut camera = OrbitCamera::default();
    let (center, radius) = real.bounding_sphere();
    camera.frame_bounds(center, radius);
    let hidden = HashSet::new();
    let visible_count = (0..real.positions.len())
        .filter(|&i| is_atom_visible(&real, camera.eye(), i, 0.20, 0.15, &hidden, &hidden))
        .count();
    println!("real molecule: {visible_count} / {} atoms visible from default camera", real.positions.len());
    assert!(visible_count > 0 && visible_count < real.positions.len(), "expected a meaningful subset, not all-or-none");
    println!("ALL CHECKS PASSED");
}
