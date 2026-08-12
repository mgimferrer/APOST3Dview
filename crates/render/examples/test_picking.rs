use std::env;
use std::path::PathBuf;

use apost3dview_core::Molecule;
use apost3dview_render::{pick_atom, pick_bond, ray_from_ndc, OrbitCamera};

fn main() {
    let path = env::args().nth(1).map(PathBuf::from).expect("usage: test_picking <path.fchk>");
    let molecule = Molecule::from_fchk(&path).expect("failed to parse fchk");

    let mut camera = OrbitCamera::default();
    let (center, radius) = molecule.bounding_sphere();
    camera.frame_bounds(center, radius);
    let aspect_ratio = 1.4;

    let view_proj = camera.view_projection_matrix(aspect_ratio);
    let hidden = std::collections::HashSet::new();

    let mut atom_pass = 0;
    let mut atom_fail = 0;
    for (index, &position) in molecule.positions.iter().enumerate() {
        let clip = view_proj * glam::Vec4::new(position.x, position.y, position.z, 1.0);
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        if !(-1.0..=1.0).contains(&ndc_x) || !(-1.0..=1.0).contains(&ndc_y) {
            continue; // off-screen at this framing, skip
        }

        let (origin, dir) = ray_from_ndc(&camera, aspect_ratio, ndc_x, ndc_y);
        let hit = pick_atom(&molecule, origin, dir, 0.35, &hidden);
        if hit == Some(index) {
            atom_pass += 1;
        } else {
            atom_fail += 1;
            println!("MISS atom {index}: expected Some({index}), got {hit:?}");
        }
    }
    println!("Atom picking: {atom_pass} pass, {atom_fail} fail (of {} atoms)", molecule.positions.len());

    let mut bond_pass = 0;
    let mut bond_fail = 0;
    for (index, bond) in molecule.bonds.iter().enumerate() {
        let midpoint = (molecule.positions[bond.atom_a] + molecule.positions[bond.atom_b]) * 0.5;
        let clip = view_proj * glam::Vec4::new(midpoint.x, midpoint.y, midpoint.z, 1.0);
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        if !(-1.0..=1.0).contains(&ndc_x) || !(-1.0..=1.0).contains(&ndc_y) {
            continue;
        }

        let (origin, dir) = ray_from_ndc(&camera, aspect_ratio, ndc_x, ndc_y);
        let hit = pick_bond(&molecule, origin, dir, 0.15, &hidden, &hidden);
        if hit == Some(index) {
            bond_pass += 1;
        } else {
            bond_fail += 1;
            println!("MISS bond {index}: expected Some({index}), got {hit:?}");
        }
    }
    println!("Bond picking: {bond_pass} pass, {bond_fail} fail (of {} bonds)", molecule.bonds.len());
}
