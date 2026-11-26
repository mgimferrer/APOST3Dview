//! Rebuild on wrecked drawings: good molecules flattened, scrambled or
//! crushed, then rebuilt from their bonds.

use std::collections::BTreeSet;

use super::*;
use apost3dview_core::{element_data, perceive_bonds, Molecule};

type Mol = (Vec<u32>, Vec<Vec3>);

fn ring(z: u32, n: usize, radius: f32, height: f32, offset: f32) -> Vec<(u32, Vec3)> {
    (0..n)
        .map(|k| {
            let a = (offset + 360.0 * k as f32 / n as f32).to_radians();
            (z, Vec3::new(radius * a.cos(), radius * a.sin(), height))
        })
        .collect()
}

fn unzip(atoms: Vec<(u32, Vec3)>) -> Mol {
    atoms.into_iter().unzip()
}

fn benzene() -> Mol {
    let mut m = ring(6, 6, 1.39, 0.0, 0.0);
    m.extend(ring(1, 6, 2.47, 0.0, 0.0));
    unzip(m)
}

fn cyclohexane() -> Mol {
    let mut m = Vec::new();
    for k in 0..6 {
        let a = (60.0 * k as f32).to_radians();
        let up = if k % 2 == 0 { 1.0 } else { -1.0 };
        m.push((6, Vec3::new(1.45 * a.cos(), 1.45 * a.sin(), 0.25 * up)));
        m.push((1, Vec3::new(1.45 * a.cos(), 1.45 * a.sin(), (0.25 + 1.09) * up)));
        m.push((1, Vec3::new(2.47 * a.cos(), 2.47 * a.sin(), -0.10 * up)));
    }
    unzip(m)
}

fn hexane() -> Mol {
    // Zigzag carbons, two hydrogens on each (three at the ends).
    let mut m = Vec::new();
    for k in 0..6 {
        let c = Vec3::new(1.27 * k as f32, if k % 2 == 0 { 0.0 } else { 0.85 }, 0.0);
        m.push((6, c));
        let away = if k % 2 == 0 { -1.0 } else { 1.0 };
        m.push((1, c + Vec3::new(0.0, 0.5 * away, 0.95)));
        m.push((1, c + Vec3::new(0.0, 0.5 * away, -0.95)));
        if k == 0 || k == 5 {
            m.push((1, c + Vec3::new(if k == 0 { -1.0 } else { 1.0 }, 0.3 * away, 0.0)));
        }
    }
    unzip(m)
}

fn chfclbr() -> Mol {
    let a = 0.629;
    unzip(vec![
        (6, Vec3::ZERO),
        (1, Vec3::new(a, a, a)),
        (9, Vec3::new(-0.78, -0.78, 0.78)),
        (17, Vec3::new(-1.02, 1.02, -1.02)),
        (35, Vec3::new(1.12, -1.12, -1.12)),
    ])
}

fn example(job: &str, file: &str) -> Mol {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(job).join(file);
    let m = Molecule::from_fchk(&path).unwrap();
    (m.atomic_numbers, m.positions)
}

fn bond_set(z: &[u32], p: &[Vec3]) -> BTreeSet<(usize, usize)> {
    perceive_bonds(z, p).iter().map(|b| (b.atom_a.min(b.atom_b), b.atom_a.max(b.atom_b))).collect()
}

fn noise(seed: u64) -> impl FnMut() -> f32 {
    let mut state = seed;
    move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 2_000_001) as f32 / 1_000_000.0 - 1.0
    }
}

/// The ways students wreck a drawing.
fn wrecks(p: &[Vec3]) -> Vec<(&'static str, Vec<Vec3>)> {
    let mut r = noise(7);
    let flat = p.iter().map(|v| Vec3::new(v.x, v.y, 0.0) + Vec3::new(r(), r(), 0.0) * 0.15).collect();
    let mut r = noise(11);
    let scrambled = p.iter().map(|v| *v + Vec3::new(r(), r(), r()) * (0.8 / 3f32.sqrt())).collect();
    let mut r = noise(13);
    let crushed = p.iter().map(|v| *v * 0.5 + Vec3::new(r(), r(), r()) * 0.05).collect();
    vec![("flattened", flat), ("scrambled", scrambled), ("crushed", crushed)]
}

/// Why a rebuilt structure isn't sensible, if it isn't.
fn problems(z: &[u32], bonds: &BTreeSet<(usize, usize)>, p: &[Vec3]) -> Vec<String> {
    let mut out = Vec::new();
    let found = bond_set(z, p);
    if &found != bonds {
        let missing: Vec<_> = bonds.difference(&found).collect();
        let extra: Vec<_> = found.difference(bonds).collect();
        out.push(format!("bonds differ: missing {missing:?}, extra {extra:?}"));
    }
    let mut neighbors = vec![BTreeSet::new(); z.len()];
    for &(a, b) in bonds {
        neighbors[a].insert(b);
        neighbors[b].insert(a);
    }
    for a in 0..z.len() {
        for b in (a + 1)..z.len() {
            let near = neighbors[a].contains(&b) || neighbors[a].iter().any(|c| neighbors[*c].contains(&b));
            let contact = 0.75 * (element_data(z[a]).vdw_radius + element_data(z[b]).vdw_radius);
            if !near && p[a].distance(p[b]) < contact {
                out.push(format!("atoms {a} and {b} overlap ({:.2} A)", p[a].distance(p[b])));
            }
        }
    }
    out
}

#[test]
fn wrecked_drawings_are_rebuilt() {
    let molecules: Vec<(&str, Mol)> = vec![
        ("benzene", benzene()),
        ("cyclohexane", cyclohexane()),
        ("hexane", hexane()),
        ("CHFClBr", chfclbr()),
        ("CH3F", example("CH3F-GEOS", "CH3F-GEOS.fchk")),
        ("[Fe(CN)5NO]3-", example("FeCN5NO3--UBLYP", "FeCN5NO3--UBLYP.fchk")),
        ("[Fe(CO)6]2+", example("FeCO6-EOS", "FeCO6-EOS.fchk")),
        ("ferrocene", example("FeCp2-EOS-OSLO", "FeCp2-EOS-OSLO.fchk")),
    ];
    let mut failures = Vec::new();
    let mut tried = 0;
    for (name, (z, good)) in &molecules {
        let bonds = bond_set(z, good);
        let pairs: Vec<(usize, usize)> = bonds.iter().copied().collect();
        for (how, wreck) in wrecks(good) {
            tried += 1;
            let rebuilt = rebuild(z, &wreck, &pairs, 16);
            let issues = problems(z, &bonds, &rebuilt);
            eprintln!("{name:>14} {how:>10}: {}", if issues.is_empty() { "ok".to_string() } else { issues.join("; ") });
            if !issues.is_empty() {
                failures.push(format!("{name} {how}"));
            }
        }
    }
    eprintln!("rebuilt {} of {tried}", tried - failures.len());
    assert!(failures.is_empty(), "not sensible: {failures:?}");
}

#[test]
fn the_drawn_handedness_is_kept() {
    let (z, good) = chfclbr();
    let bonds: Vec<(usize, usize)> = bond_set(&z, &good).into_iter().collect();
    let volume = |p: &[Vec3]| (p[1] - p[0]).dot((p[2] - p[0]).cross(p[3] - p[0]));
    // Drawn roughly (0.3 A off) but clearly one hand, and its mirror image.
    let mut r = noise(5);
    let rough: Vec<Vec3> = good.iter().map(|v| *v + Vec3::new(r(), r(), r()) * 0.17).collect();
    let mirror: Vec<Vec3> = rough.iter().map(|v| Vec3::new(-v.x, v.y, v.z)).collect();
    for drawing in [rough, mirror] {
        let rebuilt = rebuild(&z, &drawing, &bonds, 8);
        assert_eq!(volume(&rebuilt).signum(), volume(&drawing).signum());
    }
}
