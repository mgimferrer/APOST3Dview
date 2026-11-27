//! The built-in library: every entry reads, bonds sensibly, and agrees with
//! Clean.

use std::collections::HashSet;

use super::*;
use crate::place::{place, Site, View};
use apost3dview_core::{perceive_bonds, Molecule};

#[test]
fn own_groups_round_trip_through_xyz() {
    let entry = library().iter().find(|e| e.key == "tert-butyl").unwrap();
    let text = group_to_xyz(entry);
    let back = group_from_xyz("my tBu", &text).unwrap();
    assert_eq!(back.label, "my tBu");
    assert_eq!(back.tab, Tab::Mine);
    assert_eq!(back.fragment.atomic_numbers, entry.fragment.atomic_numbers);
    let (a, b) = (back.fragment.attach.unwrap(), entry.fragment.attach.unwrap());
    assert_eq!(a.atom, b.atom);
    assert!(a.toward.distance(b.toward) < 1e-4);
    for (p, q) in back.fragment.positions.iter().zip(&entry.fragment.positions) {
        assert!(p.distance(*q) < 1e-4);
    }
    assert!(group_from_xyz("plain", "1\nwater\nO 0 0 0\n").is_err());
}

/// Bonds of a fragment with a hydrogen on its open bond (so Clean sees a
/// whole molecule).
fn capped(entry: &LibraryEntry) -> Molecule {
    let f = &entry.fragment;
    let mut z = f.atomic_numbers.clone();
    let mut p = f.positions.clone();
    if let (Some(a), crate::FragmentKind::Group) = (f.attach, f.kind) {
        z.push(1);
        p.push(p[a.atom] + a.toward * crate::bond_length(z[a.atom], 1, a.order) as f32);
    }
    Molecule::from_atoms(z, p)
}

/// The closest approach of two atoms neither bonded nor sharing a
/// neighbour, as a fraction of their van der Waals sum.
fn closest_contact(m: &Molecule) -> f32 {
    let n = m.atomic_numbers.len();
    let mut neighbors = vec![HashSet::new(); n];
    for b in &m.bonds {
        neighbors[b.atom_a].insert(b.atom_b);
        neighbors[b.atom_b].insert(b.atom_a);
    }
    let mut worst = f32::INFINITY;
    for a in 0..n {
        for b in (a + 1)..n {
            if neighbors[a].contains(&b) || neighbors[a].iter().any(|c| neighbors[*c].contains(&b)) {
                continue;
            }
            let sum = apost3dview_core::element_data(m.atomic_numbers[a]).vdw_radius + apost3dview_core::element_data(m.atomic_numbers[b]).vdw_radius;
            worst = worst.min(m.positions[a].distance(m.positions[b]) / sum);
        }
    }
    worst
}

/// How much Clean changes the entry's bond lengths and bond angles (not
/// its turns about bonds, which Clean's staggering may change freely).
fn clean_changes(m: &Molecule) -> (f32, f32) {
    let bonds: Vec<(usize, usize)> = m.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect();
    let after = crate::clean(&m.atomic_numbers, &m.positions, &bonds, &crate::CleanOptions { twist: true, ..Default::default() }).positions;
    let mut lengths = 0.0f32;
    for &(a, b) in &bonds {
        lengths = lengths.max((m.positions[a].distance(m.positions[b]) - after[a].distance(after[b])).abs());
    }
    let mut angles = 0.0f32;
    for &(a, c) in &bonds {
        for &(x, y) in &bonds {
            let b = if x == c && y != a { y } else if y == c && x != a { x } else { continue };
            let angle = |p: &[Vec3]| (p[a] - p[c]).angle_between(p[b] - p[c]).to_degrees();
            angles = angles.max((angle(&m.positions) - angle(&after)).abs());
        }
    }
    (lengths, angles)
}

#[test]
fn every_entry_is_sound() {
    let mut keys = HashSet::new();
    let mut problems = Vec::new();
    for entry in library() {
        assert!(keys.insert(entry.key.clone()), "{} twice", entry.key);
        let m = capped(entry);
        let bonds: Vec<(usize, usize)> = m.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect();
        let (lengths, angles) = clean_changes(&m);
        let contact = closest_contact(&m);
        let pieces = crate::Graph::new(m.atomic_numbers.len(), bonds.iter().copied()).piece(0).len();
        eprintln!("{:<26} {:>4} atoms  Clean changes bonds {:.3} A, angles {:>5.1} deg  closest contact {:.2}", entry.key, m.atomic_numbers.len(), lengths, angles, contact);
        if pieces != m.atomic_numbers.len() {
            problems.push(format!("{}: not one piece", entry.key));
        }
        if lengths > LENGTHS || angles > ANGLES {
            problems.push(format!("{}: Clean changes bonds by {lengths:.3} A, angles by {angles:.1} deg", entry.key));
        }
        if contact < 0.6 {
            problems.push(format!("{}: atoms at {contact:.2} of their contact distance", entry.key));
        }
        // Groups go on methane's hydrogen, ligands on a lone metal atom,
        // without touching anything they shouldn't.
        let (host, site) = match entry.fragment.kind {
            crate::FragmentKind::Group => (
                Molecule::from_atoms(vec![6, 1, 1, 1, 1], vec![Vec3::ZERO, Vec3::splat(0.629), Vec3::new(0.629, -0.629, -0.629), Vec3::new(-0.629, 0.629, -0.629), Vec3::new(-0.629, -0.629, 0.629)]),
                Site::Replace(1),
            ),
            crate::FragmentKind::Ligand => (Molecule::from_atoms(vec![26], vec![Vec3::ZERO]), Site::On(0)),
            crate::FragmentKind::Molecule => continue,
        };
        let pairs: Vec<(usize, usize)> = host.bonds.iter().map(|b| (b.atom_a, b.atom_b)).collect();
        let placed = place(&host, &pairs, &entry.fragment, site, View::default()).unwrap().molecule;
        let found = perceive_bonds(&placed.atomic_numbers, &placed.positions).len();
        if found != placed.bonds.len() {
            problems.push(format!("{}: placed, {} bonds drawn but {found} by distance", entry.key, placed.bonds.len()));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// How far an entry may be from what Clean makes of it: Clean's simple
/// rules and the sources differ most where the sources know better (B-O-C
/// in a boronic ester is near 120 degrees, VSEPR says 104.5; crowded
/// P(tBu)3 opens its angles), by up to 0.17 A and 25 degrees.
const LENGTHS: f32 = 0.2;
const ANGLES: f32 = 26.0;
