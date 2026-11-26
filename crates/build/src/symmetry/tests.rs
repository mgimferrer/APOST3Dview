//! Known molecules, built with their ideal symmetry, then shaken.

use super::*;

/// A molecule: (atomic number, x, y, z) in angstrom.
type Mol = Vec<(u32, [f64; 3])>;

fn ring(z: u32, n: usize, radius: f64, height: f64, offset_degrees: f64) -> Mol {
    (0..n)
        .map(|k| {
            let a = (offset_degrees + 360.0 * k as f64 / n as f64).to_radians();
            (z, [radius * a.cos(), radius * a.sin(), height])
        })
        .collect()
}

fn water() -> Mol {
    vec![(8, [0.0, 0.0, 0.117]), (1, [0.757, 0.0, -0.469]), (1, [-0.757, 0.0, -0.469])]
}

fn ammonia() -> Mol {
    let mut m = vec![(7, [0.0, 0.0, 0.10])];
    m.extend(ring(1, 3, 0.94, -0.27, 0.0));
    m
}

fn methane() -> Mol {
    let a = 0.629;
    vec![(6, [0.0; 3]), (1, [a, a, a]), (1, [a, -a, -a]), (1, [-a, a, -a]), (1, [-a, -a, a])]
}

fn ethane(staggered: bool) -> Mol {
    let mut m = vec![(6, [0.0, 0.0, 0.765]), (6, [0.0, 0.0, -0.765])];
    m.extend(ring(1, 3, 1.02, 1.16, 0.0));
    m.extend(ring(1, 3, 1.02, -1.16, if staggered { 60.0 } else { 0.0 }));
    m
}

fn benzene() -> Mol {
    let mut m = ring(6, 6, 1.39, 0.0, 0.0);
    m.extend(ring(1, 6, 2.47, 0.0, 0.0));
    m
}

fn ferrocene(staggered: bool) -> Mol {
    let mut m = vec![(26, [0.0; 3])];
    let turn = if staggered { 36.0 } else { 0.0 };
    m.extend(ring(6, 5, 1.22, 1.66, 0.0));
    m.extend(ring(1, 5, 2.30, 1.70, 0.0));
    m.extend(ring(6, 5, 1.22, -1.66, turn));
    m.extend(ring(1, 5, 2.30, -1.70, turn));
    m
}

fn sf6() -> Mol {
    let d = 1.56;
    vec![(16, [0.0; 3]), (9, [d, 0.0, 0.0]), (9, [-d, 0.0, 0.0]), (9, [0.0, d, 0.0]), (9, [0.0, -d, 0.0]), (9, [0.0, 0.0, d]), (9, [0.0, 0.0, -d])]
}

/// C60: the even (cyclic) permutations of (0, ±1, ±3φ), (±1, ±(2+φ), ±2φ),
/// (±φ, ±2, ±(2φ+1)), edge 2, scaled to 1.4 angstrom.
fn c60() -> Mol {
    let phi = (1.0 + 5f64.sqrt()) / 2.0;
    let mut m = Mol::new();
    let mut add = |v: [f64; 3]| {
        for shift in 0..3 {
            let p = [v[shift % 3], v[(shift + 1) % 3], v[(shift + 2) % 3]];
            for signs in 0..8 {
                let s = [if signs & 1 == 0 { 1.0 } else { -1.0 }, if signs & 2 == 0 { 1.0 } else { -1.0 }, if signs & 4 == 0 { 1.0 } else { -1.0 }];
                let q = [p[0] * s[0] * 0.7, p[1] * s[1] * 0.7, p[2] * s[2] * 0.7];
                if !m.iter().any(|(_, r)| (0..3).all(|k| (r[k] - q[k]).abs() < 1e-9)) {
                    m.push((6, q));
                }
            }
        }
    };
    add([0.0, 1.0, 3.0 * phi]);
    add([1.0, 2.0 + phi, 2.0 * phi]);
    add([phi, 2.0, 2.0 * phi + 1.0]);
    assert_eq!(m.len(), 60);
    m
}

fn allene() -> Mol {
    vec![
        (6, [0.0, 0.0, 0.0]),
        (6, [0.0, 0.0, 1.31]),
        (6, [0.0, 0.0, -1.31]),
        (1, [0.93, 0.0, 1.87]),
        (1, [-0.93, 0.0, 1.87]),
        (1, [0.0, 0.93, -1.87]),
        (1, [0.0, -0.93, -1.87]),
    ]
}

fn hydrogen_peroxide() -> Mol {
    let t = 55.5f64.to_radians();
    vec![(8, [0.725, 0.0, 0.0]), (8, [-0.725, 0.0, 0.0]), (1, [0.975, 0.9 * t.cos(), 0.9 * t.sin()]), (1, [-0.975, 0.9 * t.cos(), -0.9 * t.sin()])]
}

fn chfclbr() -> Mol {
    let a = 0.629;
    vec![(6, [0.0; 3]), (1, [a, a, a]), (9, [-0.78, -0.78, 0.78]), (17, [-1.02, 1.02, -1.02]), (35, [1.12, -1.12, -1.12])]
}

fn boric_acid() -> Mol {
    let mut m = vec![(5, [0.0; 3])];
    m.extend(ring(8, 3, 1.37, 0.0, 0.0));
    m.extend(ring(1, 3, 1.95, 0.0, 28.0));
    m
}

fn s8() -> Mol {
    (0..8)
        .map(|k| {
            let a = (45.0 * k as f64).to_radians();
            (16, [2.0 * a.cos(), 2.0 * a.sin(), if k % 2 == 0 { 0.48 } else { -0.48 }])
        })
        .collect()
}

fn cyclohexane() -> Mol {
    let mut m = Mol::new();
    for k in 0..6 {
        let a = (60.0 * k as f64).to_radians();
        let up = if k % 2 == 0 { 1.0 } else { -1.0 };
        m.push((6, [1.45 * a.cos(), 1.45 * a.sin(), 0.25 * up]));
        m.push((1, [1.45 * a.cos(), 1.45 * a.sin(), (0.25 + 1.09) * up]));
        m.push((1, [2.47 * a.cos(), 2.47 * a.sin(), -0.10 * up]));
    }
    m
}

fn nitrogen() -> Mol {
    vec![(7, [0.0, 0.0, 0.55]), (7, [0.0, 0.0, -0.55])]
}

fn hcn() -> Mol {
    vec![(1, [0.0, 0.0, -1.59]), (6, [0.0, 0.0, -0.53]), (7, [0.0, 0.0, 0.62])]
}

/// Every known molecule with its group.
fn known() -> Vec<(&'static str, Mol, &'static str)> {
    vec![
        ("water", water(), "C2v"),
        ("ammonia", ammonia(), "C3v"),
        ("methane", methane(), "Td"),
        ("staggered ethane", ethane(true), "D3d"),
        ("eclipsed ethane", ethane(false), "D3h"),
        ("benzene", benzene(), "D6h"),
        ("eclipsed ferrocene", ferrocene(false), "D5h"),
        ("staggered ferrocene", ferrocene(true), "D5d"),
        ("SF6", sf6(), "Oh"),
        ("C60", c60(), "Ih"),
        ("allene", allene(), "D2d"),
        ("hydrogen peroxide", hydrogen_peroxide(), "C2"),
        ("CHFClBr", chfclbr(), "C1"),
        ("boric acid", boric_acid(), "C3h"),
        ("S8", s8(), "D4d"),
        ("cyclohexane chair", cyclohexane(), "D3d"),
        ("N2", nitrogen(), "D∞h"),
        ("HCN", hcn(), "C∞v"),
    ]
}

fn split(m: &Mol) -> (Vec<u32>, Vec<Vec3>) {
    (m.iter().map(|(z, _)| *z).collect(), m.iter().map(|(_, p)| DVec3::from_array(*p).as_vec3()).collect())
}

/// Every atom moved by up to `size` angstrom (a fixed pseudo-random walk).
fn shaken(m: &Mol, size: f64) -> Mol {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 2_000_001) as f64 / 1_000_000.0 - 1.0
    };
    m.iter().map(|(z, p)| (*z, [p[0] + size * next() / 3f64.sqrt(), p[1] + size * next() / 3f64.sqrt(), p[2] + size * next() / 3f64.sqrt()])).collect()
}

#[test]
fn known_molecules_have_their_groups() {
    for (name, m, group) in known() {
        let (z, p) = split(&m);
        let analysis = analyze(&z, &p);
        let found = analysis.group_within(PROGRAM_TOLERANCE);
        assert_eq!(found.label, group, "{name}: {:?}", analysis.candidates.iter().map(|c| (c.label.clone(), c.displacement)).collect::<Vec<_>>());
    }
}

#[test]
fn shaken_molecules_are_c1_and_symmetrize_back() {
    for (name, m, group) in known() {
        if name == "N2" {
            continue; // two like atoms are always D∞h
        }
        let (z, p) = split(&shaken(&m, 0.02));
        let analysis = analyze(&z, &p);
        // Three atoms always lie in a plane: Cs stays exact.
        let lowest = if m.len() == 3 { "Cs" } else { "C1" };
        assert_eq!(analysis.group_within(PROGRAM_TOLERANCE).label, lowest, "{name}");
        let candidate = analysis
            .candidates
            .iter()
            .find(|c| c.label == group)
            .unwrap_or_else(|| panic!("{name}: no {group} among {:?}", analysis.candidates.iter().map(|c| (c.label.clone(), c.displacement)).collect::<Vec<_>>()));
        assert!(candidate.displacement < 0.03, "{name}: {group} needs {}", candidate.displacement);
        // Symmetrized: exactly that group, nobody moved more than said, the
        // centroid in place.
        let symmetric = candidate.symmetrized();
        let again = analyze(&z, symmetric);
        assert_eq!(again.group_within(PROGRAM_TOLERANCE).label, group, "{name} symmetrized");
        let moved = p.iter().zip(symmetric).map(|(a, b)| a.as_dvec3().distance(b.as_dvec3())).fold(0.0, f64::max);
        assert!((moved - candidate.displacement).abs() < 1e-5, "{name}: moved {moved} vs {}", candidate.displacement);
        let centroid = |v: &[Vec3]| v.iter().map(|x| x.as_dvec3()).sum::<DVec3>() / v.len() as f64;
        assert!(centroid(&p).distance(centroid(symmetric)) < 1e-5, "{name}: the centroid moved");
    }
}

#[test]
fn standard_orientation_puts_the_main_axis_along_z() {
    for (name, m, group) in known() {
        // Turned and moved away first.
        let turn = glam::DQuat::from_euler(glam::EulerRot::XYZ, 0.7, -1.1, 0.4);
        let moved: Mol = m.iter().map(|(z, p)| (*z, (turn * DVec3::from_array(*p) + DVec3::new(3.0, -2.0, 5.0)).to_array())).collect();
        let (z, p) = split(&moved);
        let analysis = analyze(&z, &p);
        let found = analysis.group_within(PROGRAM_TOLERANCE);
        let oriented = standard_orientation(&z, &p, found);
        let (rotation, _) = standard_frame(&z, &p, found);
        assert!((rotation.determinant() - 1.0).abs() < 1e-9, "{name}: not a rotation");
        // The same shape, the same group, the center of charge at the origin.
        let again = analyze(&z, &oriented);
        assert_eq!(again.group_within(PROGRAM_TOLERANCE).label, group, "{name}");
        let charge: f64 = z.iter().map(|&k| f64::from(k)).sum();
        let center = oriented.iter().zip(&z).map(|(v, &k)| v.as_dvec3() * f64::from(k)).sum::<DVec3>() / charge;
        assert!(center.length() < 1e-4, "{name}: center {center:?}");
        // The main axis (or the plane's normal) is z.
        // (Cubic and icosahedral groups and D2h have several equal axes.)
        if let Some(axis) = again.group_within(PROGRAM_TOLERANCE).main_axis()
            && !matches!(group, "Td" | "Oh" | "Ih" | "D2h" | "D2")
        {
            assert!(axis.z.abs() > 0.999, "{name}: main axis {axis:?}");
        }
    }
}

#[test]
fn several_candidates_of_one_group_are_told_apart() {
    // Water pulled a little off C2v: two different mirror planes give Cs.
    let mut m = water();
    m[1].1[2] += 0.02;
    let (z, p) = split(&m);
    let analysis = analyze(&z, &p);
    let cs: Vec<&Candidate> = analysis.candidates.iter().filter(|c| c.label == "Cs").collect();
    assert!(!cs.is_empty(), "{:?}", analysis.candidates.iter().map(|c| (c.label.clone(), c.displacement)).collect::<Vec<_>>());
    // The molecular plane is exact (Cs at 0), C2v needs a move.
    assert_eq!(analysis.group_within(PROGRAM_TOLERANCE).label, "Cs");
    let c2v = analysis.candidates.iter().find(|c| c.label == "C2v").expect("C2v listed");
    assert!(c2v.displacement > 0.005 && c2v.displacement < 0.02);
}


/// Writes the known molecules as .xyz into `APOST3DVIEW_SYMMETRY_XYZ` (for
/// the comparison with pymatgen in validation/symmetry/).
#[test]
#[ignore]
fn write_known_molecules() {
    let Some(dir) = std::env::var_os("APOST3DVIEW_SYMMETRY_XYZ") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let symbol = |z: u32| match z {
        1 => "H",
        5 => "B",
        6 => "C",
        7 => "N",
        8 => "O",
        9 => "F",
        16 => "S",
        17 => "Cl",
        26 => "Fe",
        35 => "Br",
        _ => panic!("no symbol for {z}"),
    };
    for (name, m, group) in known() {
        let mut text = format!("{}\n{name}, {group}\n", m.len());
        for (z, p) in &m {
            text.push_str(&format!("{:<2} {:16.10} {:16.10} {:16.10}\n", symbol(*z), p[0], p[1], p[2]));
        }
        std::fs::write(dir.join(format!("{}.xyz", name.replace(' ', "-"))), text).unwrap();
    }
}



