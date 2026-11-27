//! Clean on known molecules, distorted at random.

use super::*;
use apost3dview_core::perceive_bonds;

type Mol = Vec<(u32, [f64; 3])>;

fn ring(z: u32, n: usize, radius: f64, height: f64, offset: f64) -> Mol {
    (0..n)
        .map(|k| {
            let a = (offset + 360.0 * k as f64 / n as f64).to_radians();
            (z, [radius * a.cos(), radius * a.sin(), height])
        })
        .collect()
}

fn methane() -> Mol {
    let a = 0.629;
    vec![(6, [0.0; 3]), (1, [a, a, a]), (1, [a, -a, -a]), (1, [-a, a, -a]), (1, [-a, -a, a])]
}

fn water() -> Mol {
    vec![(8, [0.0, 0.0, 0.117]), (1, [0.757, 0.0, -0.469]), (1, [-0.757, 0.0, -0.469])]
}

fn ammonia() -> Mol {
    let mut m = vec![(7, [0.0, 0.0, 0.10])];
    m.extend(ring(1, 3, 0.94, -0.27, 0.0));
    m
}

fn ethane() -> Mol {
    let mut m = vec![(6, [0.0, 0.0, 0.765]), (6, [0.0, 0.0, -0.765])];
    m.extend(ring(1, 3, 1.02, 1.16, 0.0));
    m.extend(ring(1, 3, 1.02, -1.16, 60.0));
    m
}

fn benzene() -> Mol {
    let mut m = ring(6, 6, 1.39, 0.0, 0.0);
    m.extend(ring(1, 6, 2.47, 0.0, 0.0));
    m
}

fn sf6() -> Mol {
    let d = 1.56;
    vec![(16, [0.0; 3]), (9, [d, 0.0, 0.0]), (9, [-d, 0.0, 0.0]), (9, [0.0, d, 0.0]), (9, [0.0, -d, 0.0]), (9, [0.0, 0.0, d]), (9, [0.0, 0.0, -d])]
}

fn bf3() -> Mol {
    let mut m = vec![(5, [0.0; 3])];
    m.extend(ring(9, 3, 1.31, 0.0, 0.0));
    m
}

fn ferrocene() -> Mol {
    let mut m = vec![(26, [0.0; 3])];
    m.extend(ring(6, 5, 1.22, 1.66, 0.0));
    m.extend(ring(1, 5, 2.30, 1.70, 0.0));
    m.extend(ring(6, 5, 1.22, -1.66, 36.0));
    m.extend(ring(1, 5, 2.30, -1.70, 36.0));
    m
}

fn split(m: &Mol) -> (Vec<u32>, Vec<Vec3>) {
    (m.iter().map(|(z, _)| *z).collect(), m.iter().map(|(_, p)| DVec3::from_array(*p).as_vec3()).collect())
}

/// Every atom moved by up to `size` angstrom (fixed pseudo-random).
fn distorted(p: &[Vec3], size: f32) -> Vec<Vec3> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 2_000_001) as f32 / 1_000_000.0 - 1.0
    };
    p.iter().map(|v| *v + Vec3::new(next(), next(), next()) * (size / 3f32.sqrt())).collect()
}

fn bonds_of(z: &[u32], p: &[Vec3]) -> Vec<(usize, usize)> {
    perceive_bonds(z, p).iter().map(|b| (b.atom_a, b.atom_b)).collect()
}

fn angle(p: &[Vec3], a: usize, c: usize, b: usize) -> f64 {
    (p[a] - p[c]).as_dvec3().angle_between((p[b] - p[c]).as_dvec3()).to_degrees()
}

fn distance(p: &[Vec3], a: usize, b: usize) -> f64 {
    p[a].as_dvec3().distance(p[b].as_dvec3())
}

/// Cleans `m` distorted by `size`, its bonds those of the ideal molecule.
fn cleaned(m: &Mol, size: f32, options: &CleanOptions) -> (Vec<u32>, Vec<Vec3>) {
    let (z, ideal) = split(m);
    let bonds = bonds_of(&z, &ideal);
    let result = clean(&z, &distorted(&ideal, size), &bonds, options);
    (z, result.positions)
}

#[test]
fn the_gradient_matches_finite_differences() {
    for m in [ethane(), benzene(), sf6(), ferrocene()] {
        let (z, ideal) = split(&m);
        let bonds = bonds_of(&z, &ideal);
        let p: Vec<DVec3> = distorted(&ideal, 0.3).iter().map(|v| v.as_dvec3()).collect();
        let model = build_model(&z, &p, &bonds, &CleanOptions { twist: true, ideal_metals: true, ..Default::default() });
        let mut g = vec![DVec3::ZERO; p.len()];
        energy(&model, &p, &mut g);
        let mut scratch = vec![DVec3::ZERO; p.len()];
        for i in 0..p.len() {
            for axis in 0..3 {
                let h = 1e-6;
                let (mut plus, mut minus) = (p.clone(), p.clone());
                plus[i][axis] += h;
                minus[i][axis] -= h;
                let numeric = (energy(&model, &plus, &mut scratch) - energy(&model, &minus, &mut scratch)) / (2.0 * h);
                assert!((numeric - g[i][axis]).abs() < 1e-4 * (1.0 + numeric.abs()), "atom {i} axis {axis}: {numeric} vs {}", g[i][axis]);
            }
        }
    }
}

#[test]
fn distorted_molecules_come_back_tidy() {
    let options = CleanOptions { twist: true, ..Default::default() };
    // Methane: C-H 1.07 (Cordero radii), tetrahedral.
    let (_, p) = cleaned(&methane(), 0.25, &options);
    for h in 1..5 {
        assert!((distance(&p, 0, h) - 1.07).abs() < 0.01, "C-H {}", distance(&p, 0, h));
        for k in (h + 1)..5 {
            assert!((angle(&p, h, 0, k) - 109.47).abs() < 1.5, "HCH {}", angle(&p, h, 0, k));
        }
    }
    // Water bent, ammonia pyramidal.
    let (_, p) = cleaned(&water(), 0.2, &options);
    assert!((angle(&p, 1, 0, 2) - 104.5).abs() < 1.5, "HOH {}", angle(&p, 1, 0, 2));
    let (_, p) = cleaned(&ammonia(), 0.2, &options);
    assert!((angle(&p, 1, 0, 2) - 107.0).abs() < 1.5, "HNH {}", angle(&p, 1, 0, 2));
    // BF3 flat and trigonal.
    let (_, p) = cleaned(&bf3(), 0.2, &options);
    assert!((angle(&p, 1, 0, 2) + angle(&p, 2, 0, 3) + angle(&p, 3, 0, 1) - 360.0).abs() < 1.0);
    // SF6 octahedral.
    let (_, p) = cleaned(&sf6(), 0.25, &options);
    for a in 1..7 {
        for b in (a + 1)..7 {
            let t = angle(&p, a, 0, b);
            assert!((t - 90.0).abs() < 1.5 || (t - 180.0).abs() < 1.5, "FSF {t}");
        }
    }
    // Benzene: 1.395 aromatic C-C, flat.
    let (_, p) = cleaned(&benzene(), 0.08, &options);
    for k in 0..6 {
        assert!((distance(&p, k, (k + 1) % 6) - 1.395).abs() < 0.015, "C-C {}", distance(&p, k, (k + 1) % 6));
    }
    let normal = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
    let off = p.iter().map(|v| (*v - p[0]).dot(normal).abs()).fold(0.0, f32::max);
    assert!(off < 0.02, "not flat: {off}");
    // Ethane staggered.
    let (_, p) = cleaned(&ethane(), 0.15, &options);
    let q = [p[2].as_dvec3(), p[0].as_dvec3(), p[1].as_dvec3(), p[5].as_dvec3()];
    let phi = dihedral(&q).to_degrees().abs();
    assert!(phi.rem_euclid(120.0).min(120.0 - phi.rem_euclid(120.0)) > 55.0, "eclipsed: {phi}");
}

#[test]
fn metals_keep_their_angles_unless_asked() {
    let (z, ideal) = split(&ferrocene());
    let bonds = bonds_of(&z, &ideal);
    let start = distorted(&ideal, 0.1);
    let kept = clean(&z, &start, &bonds, &CleanOptions::default());
    // Fe-C bonds and the angles at Fe kept as they were.
    for c in 1..6 {
        assert!((distance(&kept.positions, 0, c) - distance(&start, 0, c)).abs() < 0.03, "Fe-C {} from {}", distance(&kept.positions, 0, c), distance(&start, 0, c));
    }
    // With ideal metals, from covalent radii (Fe 1.42 + C 0.76).
    let ideal_fe = clean(&z, &start, &bonds, &CleanOptions { ideal_metals: true, ..Default::default() });
    for c in 1..6 {
        assert!((distance(&ideal_fe.positions, 0, c) - 2.18).abs() < 0.05, "ideal Fe-C {}", distance(&ideal_fe.positions, 0, c));
    }
    // Cp rings: aromatic C-C, flat.
    for ring in [1..6, 11..16] {
        let r: Vec<usize> = ring.collect();
        for k in 0..5 {
            assert!((distance(&kept.positions, r[k], r[(k + 1) % 5]) - 1.395).abs() < 0.03, "Cp C-C {}", distance(&kept.positions, r[k], r[(k + 1) % 5]));
        }
    }
    for a in 0..z.len() {
        for b in (a + 1)..z.len() {
            assert!(distance(&kept.positions, a, b) > 0.9, "{a} {b} overlap");
        }
    }
}

#[test]
fn a_symmetric_molecule_stays_exactly_symmetric() {
    let (z, ideal) = split(&ethane());
    let bonds = bonds_of(&z, &ideal);
    // Stretched along the C-C axis: still D3d, far from tidy.
    let stretched: Vec<Vec3> = ideal.iter().map(|v| Vec3::new(v.x, v.y, v.z * 1.3)).collect();
    let analysis = crate::analyze(&z, &stretched);
    let group = analysis.group_within(crate::PROGRAM_TOLERANCE);
    assert_eq!(group.label, "D3d");
    let keep = KeptSymmetry { center: analysis.center, operations: group.operations.clone(), permutations: group.permutations.clone() };
    let result = clean(&z, &stretched, &bonds, &CleanOptions { symmetry: Some(keep), twist: true, ..Default::default() });
    let after = crate::analyze(&z, &result.positions);
    assert_eq!(after.group_within(crate::PROGRAM_TOLERANCE).label, "D3d");
    assert!((distance(&result.positions, 0, 1) - 1.52).abs() < 0.01, "C-C {}", distance(&result.positions, 0, 1));
}

#[test]
fn held_atoms_stay_and_tidy_geometries_barely_move() {
    let (z, ideal) = split(&methane());
    let bonds = bonds_of(&z, &ideal);
    let start = distorted(&ideal, 0.25);
    let result = clean(&z, &start, &bonds, &CleanOptions { held: vec![0, 1], ..Default::default() });
    assert_eq!(result.positions[0], start[0]);
    assert_eq!(result.positions[1], start[1]);
    // Ideal (experimental) methane: only C-H 1.09 -> 1.07.
    let tidy = clean(&z, &ideal, &bonds, &CleanOptions::default());
    assert!(tidy.largest_move < 0.05, "moved {}", tidy.largest_move);
}

#[test]
fn carbonyls_and_isocyanides_keep_their_triple_bonds() {
    // The valence cap allows one formal charge: O of C≡O, N of R-N≡C.
    let mut m = vec![(26, [0.0; 3])];
    for d in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]] {
        let v = DVec3::from_array(d);
        m.push((6, (v * 1.82).to_array()));
        m.push((8, (v * 2.97).to_array()));
    }
    let (z, p) = split(&m);
    let bonds = bonds_of(&z, &p);
    let result = clean(&z, &distorted(&p, 0.05), &bonds, &CleanOptions::default());
    for c in (1..11).step_by(2) {
        assert!((distance(&result.positions, c, c + 1) - 1.13).abs() < 0.02, "C-O {}", distance(&result.positions, c, c + 1));
    }
    let methyl_isocyanide = vec![(6, [0.0, 0.0, 0.0]), (7, [0.0, 0.0, 1.16]), (6, [0.0, 0.0, 2.58]), (1, [1.03, 0.0, 2.94]), (1, [-0.51, 0.89, 2.94]), (1, [-0.51, -0.89, 2.94])];
    let (_, p) = cleaned(&methyl_isocyanide, 0.05, &CleanOptions::default());
    assert!((distance(&p, 0, 1) - 1.14).abs() < 0.02, "C≡N {}", distance(&p, 0, 1));
}
