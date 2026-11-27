//! Placements on small molecules whose result is known.

use super::*;
use crate::analyze;

fn molecule(atoms: &[(u32, [f32; 3])]) -> Molecule {
    Molecule::from_atoms(atoms.iter().map(|a| a.0).collect(), atoms.iter().map(|a| Vec3::from_array(a.1)).collect())
}

fn pairs(m: &Molecule) -> Vec<(usize, usize)> {
    m.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect()
}

fn methane() -> Molecule {
    let a = 0.629;
    molecule(&[(6, [0.0; 3]), (1, [a, a, a]), (1, [a, -a, -a]), (1, [-a, a, -a]), (1, [-a, -a, a])])
}

fn benzene() -> Molecule {
    let mut atoms = Vec::new();
    for (z, r) in [(6, 1.39f32), (1, 2.47)] {
        for k in 0..6 {
            let a = (60.0 * k as f32).to_radians();
            atoms.push((z, [r * a.cos(), r * a.sin(), 0.0]));
        }
    }
    molecule(&atoms)
}

fn group(m: &Molecule, tolerance: f64) -> String {
    analyze(&m.atomic_numbers, &m.positions).group_within(tolerance).label.clone()
}

fn angle(m: &Molecule, a: usize, c: usize, b: usize) -> f32 {
    (m.positions[a] - m.positions[c]).angle_between(m.positions[b] - m.positions[c]).to_degrees()
}

fn distance(m: &Molecule, a: usize, b: usize) -> f32 {
    m.positions[a].distance(m.positions[b])
}

/// No two atoms closer than 0.75 of their van der Waals sum unless bonded
/// or sharing a neighbour.
fn no_overlaps(m: &Molecule) {
    let n = m.atomic_numbers.len();
    let neighbors = neighbor_lists(n, &pairs(m));
    for a in 0..n {
        for b in (a + 1)..n {
            let near = neighbors[a].contains(&b) || neighbors[a].iter().any(|&c| neighbors[c].contains(&b));
            let contact = 0.75 * (element_data(m.atomic_numbers[a]).vdw_radius + element_data(m.atomic_numbers[b]).vdw_radius);
            assert!(near || distance(m, a, b) > contact, "atoms {a} and {b} overlap: {}", distance(m, a, b));
        }
    }
}

#[test]
fn an_element_comes_with_its_hydrogens() {
    for (z, hydrogens) in [(6, 3), (7, 2), (8, 1), (9, 0), (5, 2), (14, 3), (15, 2), (16, 1), (17, 0), (1, 0)] {
        let f = element_fragment(z);
        assert_eq!(f.atomic_numbers.len(), 1 + hydrogens, "element {z}");
        assert!(f.atomic_numbers[1..].iter().all(|&h| h == 1));
    }
    // CH3: tetrahedral about the open bond.
    let f = element_fragment(6);
    for k in 1..4 {
        let toward = f.attach.unwrap().toward;
        assert!((f.positions[k].angle_between(toward).to_degrees() - 109.47).abs() < 0.01);
        assert!((f.positions[k].length() - bond_length(6, 1, 1.0) as f32).abs() < 1e-5);
    }
    // A metal is the atom alone.
    assert_eq!(element_fragment(26).atomic_numbers, vec![26]);
}

#[test]
fn a_methyl_on_methane_gives_staggered_ethane() {
    let m = methane();
    let placed = place(&m, &pairs(&m), &element_fragment(6), Site::Replace(1), View::default()).unwrap();
    let e = &placed.molecule;
    assert_eq!(e.atomic_numbers.len(), 8);
    assert_eq!(placed.atom_map[1], None);
    assert_eq!(placed.added, vec![4, 5, 6, 7]);
    assert_eq!(e.bonds.len(), 7);
    assert!((distance(e, 0, 4) - 1.52).abs() < 0.01, "C-C {}", distance(e, 0, 4));
    assert_eq!(group(e, 0.03), "D3d");
    no_overlaps(e);
    // The same placement twice gives the same coordinates.
    let again = place(&m, &pairs(&m), &element_fragment(6), Site::Replace(1), View::default()).unwrap();
    assert_eq!(again.molecule.positions, e.positions);
}

#[test]
fn phenyl_on_benzene_gives_twisted_biphenyl() {
    let b = benzene();
    // Benzene without H6: a phenyl bonding through C0.
    let ring: Vec<usize> = (0..12).filter(|&k| k != 6).collect();
    let phenyl = group_from_selection(&b, &pairs(&b), &ring).unwrap();
    assert_eq!(phenyl.attach.unwrap().atom, 0);
    let placed = place(&b, &pairs(&b), &phenyl, Site::Replace(9), View::default()).unwrap();
    let m = &placed.molecule;
    assert_eq!(m.atomic_numbers.len(), 22);
    let ipso = placed.added[0];
    let host = placed.atom_map[3].unwrap();
    assert!((distance(m, ipso, host) - 1.52).abs() < 0.01);
    no_overlaps(m);
    // The rings are turned against each other (ortho hydrogens clash flat).
    let normal = |a: usize, b: usize, c: usize| (m.positions[b] - m.positions[a]).cross(m.positions[c] - m.positions[a]).normalize();
    let twist = normal(0, 1, 2).angle_between(normal(placed.added[1], placed.added[2], placed.added[3])).to_degrees();
    let twist = twist.min(180.0 - twist);
    assert!(twist > 30.0, "rings nearly flat: {twist}");
}

#[test]
fn a_free_direction_completes_the_shape() {
    // H on a lone C: four of them give methane.
    let c = molecule(&[(6, [0.0; 3])]);
    let ch4 = add_hydrogens(&c, &[], &[], View::default());
    assert_eq!(ch4.molecule.atomic_numbers.len(), 5);
    assert_eq!(group(&ch4.molecule, 0.01), "Td");
    // Ethylene skeleton: flat, 120 degrees.
    let cc = molecule(&[(6, [0.0; 3]), (6, [1.34, 0.0, 0.0])]);
    let c2h4 = add_hydrogens(&cc, &pairs(&cc), &[], View::default()).molecule;
    assert_eq!(c2h4.atomic_numbers.len(), 6);
    assert_eq!(group(&c2h4, 0.01), "D2h");
    // Water: a third hydrogen on O goes to a tetrahedral corner.
    let water = molecule(&[(8, [0.0, 0.0, 0.117]), (1, [0.757, 0.0, -0.469]), (1, [-0.757, 0.0, -0.469])]);
    let h3o = place(&water, &pairs(&water), &element_fragment(1), Site::On(0), View::default()).unwrap().molecule;
    for (a, b) in [(1, 3), (2, 3)] {
        assert!((angle(&h3o, a, 0, b) - 109.5).abs() < 8.0, "HOH {}", angle(&h3o, a, 0, b));
    }
    // Square-pyramidal Fe(CO)5 + CO on Fe: octahedral.
    let mut atoms = vec![(26, [0.0f32; 3])];
    for d in [[1.0f32, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]] {
        let v = Vec3::from_array(d);
        atoms.push((6, (v * 1.82).to_array()));
        atoms.push((8, (v * 2.97).to_array()));
    }
    let fe = molecule(&atoms);
    let co = Fragment {
        kind: FragmentKind::Ligand,
        atomic_numbers: vec![6, 8],
        positions: vec![Vec3::ZERO, Vec3::new(0.0, 0.0, 1.15)],
        attach: Some(Attachment { atom: 0, toward: Vec3::new(0.0, 0.0, -1.0), order: 1.0 }),
    };
    let placed = place(&fe, &pairs(&fe), &co, Site::On(0), View::default()).unwrap().molecule;
    let c = placed.atomic_numbers.len() - 2;
    assert!(placed.positions[c].normalize().dot(Vec3::NEG_Z) > 0.999, "CO at {:?}", placed.positions[c]);
}

#[test]
fn changing_an_element_fits_its_hydrogens() {
    // CH4: C to N gives NH3, to O gives H2O, back to C from NH3 gives CH4.
    let m = methane();
    let nh3 = change_element_adjusting(&m, &pairs(&m), 0, 7, true, View::default());
    assert_eq!(nh3.molecule.atomic_numbers.iter().filter(|&&z| z == 1).count(), 3);
    let water = change_element_adjusting(&m, &pairs(&m), 0, 8, true, View::default());
    assert_eq!(water.molecule.atomic_numbers.iter().filter(|&&z| z == 1).count(), 2);
    let back = change_element_adjusting(&nh3.molecule, &pairs(&nh3.molecule), 0, 6, true, View::default());
    assert_eq!(back.molecule.atomic_numbers.iter().filter(|&&z| z == 1).count(), 4);
    // Without adjusting, only the element.
    let plain = change_element_adjusting(&m, &pairs(&m), 0, 7, false, View::default());
    assert_eq!(plain.molecule.atomic_numbers, vec![7, 1, 1, 1, 1]);
    // Benzene C to N gives pyridine: its H goes (aromatic bonds count 1.5).
    let b = benzene();
    let pyridine = change_element_adjusting(&b, &pairs(&b), 0, 7, true, View::default());
    assert_eq!(pyridine.molecule.atomic_numbers.len(), 11);
    assert_eq!(pyridine.atom_map[6], None);
}

#[test]
fn hydrogens_complete_every_atom() {
    // Heavy atoms of acetic acid without hydrogens: C C O O.
    let skeleton = molecule(&[(6, [0.0, 0.0, 0.0]), (6, [1.52, 0.0, 0.0]), (8, [2.12, 1.05, 0.0]), (8, [2.20, -1.15, 0.0])]);
    let full = add_hydrogens(&skeleton, &pairs(&skeleton), &[], View::default()).molecule;
    // C=O at 1.21 is double: CH3 + COOH, four hydrogens.
    assert_eq!(full.atomic_numbers.iter().filter(|&&z| z == 1).count(), 4);
    no_overlaps(&full);
    // Already complete: nothing added.
    let m = methane();
    assert_eq!(add_hydrogens(&m, &pairs(&m), &[], View::default()).added.len(), 0);
}

#[test]
fn a_selection_bonded_by_one_bond_is_a_group() {
    let m = methane();
    let ethane = place(&m, &pairs(&m), &element_fragment(6), Site::Replace(1), View::default()).unwrap().molecule;
    let methyl: Vec<usize> = vec![4, 5, 6, 7];
    let f = group_from_selection(&ethane, &pairs(&ethane), &methyl).unwrap();
    assert_eq!(f.atomic_numbers, vec![6, 1, 1, 1]);
    let attach = f.attach.unwrap();
    assert_eq!(attach.atom, 0);
    assert_eq!(attach.order, 1.0);
    // Two bonds leaving: refused.
    assert!(group_from_selection(&ethane, &pairs(&ethane), &[0, 1]).is_err());
    // A molecule can't go on an atom.
    let whole = Fragment { kind: FragmentKind::Molecule, atomic_numbers: vec![6], positions: vec![Vec3::ZERO], attach: None };
    assert!(place(&m, &pairs(&m), &whole, Site::On(0), View::default()).is_err());
    // In empty space a group gets a hydrogen on its open bond: CH3 -> CH4.
    let free = place(&m, &pairs(&m), &element_fragment(6), Site::Free(Vec3::new(5.0, 0.0, 0.0)), View::default()).unwrap().molecule;
    assert_eq!(free.atomic_numbers.len(), 10);
    assert_eq!(free.bonds.len(), 8);
}
