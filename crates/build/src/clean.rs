//! Clean: a rough geometry made tidy with simple harmonic potentials, not a
//! force field (no atom types, every element), from the current geometry
//! (so it settles into the nearest tidy shape) in milliseconds.
//!
//! Terms (angstrom, arbitrary energy units):
//! - bonds (the bonds given, i.e. drawn): harmonic to a typical length for
//!   the two elements, of the bond order closest to the current distance
//!   (single, aromatic 1.5, double, triple);
//! - angles at main-group atoms: harmonic in cos θ to the VSEPR angle of the
//!   atom's electron domains (neighbours + lone pairs); at metals, kept as
//!   they are, or snapped to the nearest ideal polyhedron angle;
//! - flat centres (3 neighbours, no lone pair): the signed volume of the
//!   centre and its neighbours pulled to zero;
//! - twist about single bonds between 4-domain atoms: a weak 3-fold term
//!   toward staggered;
//! - atoms neither bonded nor sharing a neighbour: a soft repulsion below
//!   0.8 times the sum of their van der Waals radii.
//!
//! Minimized with L-BFGS; held atoms don't move; a point group, if given,
//! is kept exactly (gradient and positions symmetrized every step).

use apost3dview_core::element_data;
use glam::{DMat3, DVec3, Vec3};

const K_BOND: f64 = 100.0;
const K_ANGLE: f64 = 40.0;
const K_FLAT: f64 = 20.0;
const K_TWIST: f64 = 0.3;
const K_REPEL: f64 = 20.0;
const K_PI: f64 = 3.0;

/// What Clean may and may not do.
#[derive(Debug, Clone, Default)]
pub struct CleanOptions {
    /// Atoms that don't move.
    pub held: Vec<usize>,
    /// At metals, bond lengths from covalent radii and angles snapped to
    /// the nearest ideal polyhedron angle (otherwise both kept as they are).
    pub ideal_metals: bool,
    /// The weak preference for staggered single bonds.
    pub twist: bool,
    /// A point group to keep exactly: its operations about `center` and,
    /// for each, where every atom goes.
    pub symmetry: Option<KeptSymmetry>,
}

/// A point group's operations, as the atoms permute under them.
#[derive(Debug, Clone)]
pub struct KeptSymmetry {
    pub center: DVec3,
    pub operations: Vec<DMat3>,
    pub permutations: Vec<Vec<usize>>,
}

impl KeptSymmetry {
    /// Positions averaged over the group: exactly symmetric.
    fn positions(&self, p: &[DVec3]) -> Vec<DVec3> {
        let n = self.operations.len() as f64;
        (0..p.len())
            .map(|i| {
                self.center
                    + self.operations.iter().zip(&self.permutations).map(|(m, perm)| m.transpose() * (p[perm[i]] - self.center)).sum::<DVec3>() / n
            })
            .collect()
    }

    /// A gradient averaged over the group (as vectors, no center).
    fn gradient(&self, g: &[DVec3]) -> Vec<DVec3> {
        let n = self.operations.len() as f64;
        (0..g.len()).map(|i| self.operations.iter().zip(&self.permutations).map(|(m, perm)| m.transpose() * g[perm[i]]).sum::<DVec3>() / n).collect()
    }
}

/// The result of a clean.
#[derive(Debug, Clone)]
pub struct Cleaned {
    pub positions: Vec<Vec3>,
    /// The largest move of an atom, angstrom.
    pub largest_move: f64,
    pub steps: usize,
}

/// Transition metals, lanthanides, actinides and the s-block metals: VSEPR
/// doesn't hold, so their angles aren't driven to VSEPR values.
pub fn is_metal(z: u32) -> bool {
    matches!(z, 3 | 4 | 11 | 12 | 19..=31 | 37..=50 | 55..=84 | 87..=118)
}

/// Valence electrons of a main-group element (its group's last digit).
pub(crate) fn valence_electrons(z: u32) -> u32 {
    match z {
        1 => 1,
        2 => 2,
        _ => {
            // Position in its period, then the group.
            let period_start = [3, 11, 19, 37, 55, 87].iter().rev().find(|&&s| z >= s).copied().unwrap_or(3);
            let k = z - period_start; // 0-based column
            let span = match period_start {
                3 | 11 => 8,
                19 | 37 => 18,
                _ => 32,
            };
            let from_end = span - 1 - k; // 0 = noble gas
            if from_end < 6 { 8 - from_end } else { (k + 1).min(2) }
        }
    }
}

/// Pyykkö's double- and triple-bond covalent radii (angstrom) of the light
/// main-group elements that form multiple bonds.
fn multiple_radii(z: u32) -> Option<(f64, f64)> {
    Some(match z {
        5 => (0.78, 0.73),
        6 => (0.67, 0.60),
        7 => (0.60, 0.54),
        8 => (0.57, 0.53),
        14 => (1.07, 1.02),
        15 => (1.02, 0.94),
        16 => (0.94, 0.95),
        _ => return None,
    })
}

/// The typical lengths of the bond orders the pair can have: (order,
/// length). Single from Cordero's radii, aromatic by Pauling's rule (single
/// − 0.71 log10 1.5), double and triple from Pyykkö's radii.
pub(crate) fn bond_lengths(a: u32, b: u32) -> Vec<(f64, f64)> {
    let (ra, rb) = (f64::from(element_data(a).covalent_radius), f64::from(element_data(b).covalent_radius));
    let single = ra + rb;
    let mut out = vec![(1.0, single)];
    if let (Some((da, ta)), Some((db, tb))) = (multiple_radii(a), multiple_radii(b)) {
        out.push((1.5, single - 0.71 * 1.5f64.log10()));
        out.push((2.0, da + db));
        out.push((3.0, ta + tb));
    }
    out
}

/// A bond's order and target length: the order whose typical length is
/// closest to the current distance (what was drawn stays what it is).
pub(crate) fn bond_target(a: u32, b: u32, distance: f64) -> (f64, f64) {
    bond_lengths(a, b).into_iter().min_by(|x, y| (x.1 - distance).abs().total_cmp(&(y.1 - distance).abs())).unwrap()
}

/// Everything Clean pulls toward, worked out once from the starting
/// geometry.
pub(crate) struct Model {
    pub(crate) bonds: Vec<(usize, usize, f64)>,
    pub(crate) angles: Vec<(usize, usize, usize, f64)>,
    pub(crate) flat: Vec<[usize; 4]>,
    twists: Vec<[usize; 4]>,
    /// Across double and aromatic bonds between flat centres: 0 or 180.
    pi: Vec<[usize; 4]>,
    pub(crate) repel: Vec<(usize, usize, f64)>,
}

pub(crate) fn build_model(z: &[u32], p: &[DVec3], bonds: &[(usize, usize)], options: &CleanOptions) -> Model {
    let n = z.len();
    let mut neighbors = vec![Vec::new(); n];
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    for &(a, b) in bonds {
        if a >= n || b >= n || a == b || neighbors[a].contains(&b) {
            continue;
        }
        neighbors[a].push(b);
        neighbors[b].push(a);
        pairs.push((a, b, bond_target(z[a], z[b], p[a].distance(p[b])).0));
    }
    // A ligand atom's metal is one of its electron domains when it bonds
    // to it on its own (σ: the C of CO or CN); not when a neighbour bonds to
    // the same metal too (η: a Cp ring on Fe).
    let sigma = |i: usize, m: usize| !neighbors[i].iter().any(|&j| j != m && !is_metal(z[j]) && neighbors[j].contains(&m));
    let organic: Vec<Vec<usize>> = (0..n)
        .map(|i| neighbors[i].iter().copied().filter(|&j| !is_metal(z[j]) || (!is_metal(z[i]) && sigma(i, j))).collect())
        .collect();
    let rings = small_rings(&organic);
    // Aromatic rings (5 or 6 atoms, each three-connected C, or N, O, S
    // with two or three neighbours, every bond drawn aromatic or double):
    // all their bonds 1.5, whatever the drawing's lengths.
    let mut aromatic = std::collections::HashSet::new();
    for ring in rings.iter().filter(|r| r.len() == 5 || r.len() == 6) {
        let atoms_ok = ring.iter().all(|&i| match z[i] {
            6 => organic[i].len() == 3,
            7 | 8 | 15 | 16 => (2..=3).contains(&organic[i].len()),
            _ => false,
        });
        let bond_ok = |a: usize, b: usize| pairs.iter().any(|&(x, y, o)| ((x, y) == (a, b) || (x, y) == (b, a)) && o >= 1.5);
        let ring_bonds: Vec<(usize, usize)> = (0..ring.len()).map(|k| (ring[k], ring[(k + 1) % ring.len()])).collect();
        if atoms_ok && ring_bonds.iter().filter(|&&(a, b)| bond_ok(a, b)).count() * 2 >= ring.len() {
            for (a, b) in ring_bonds {
                aromatic.insert((a.min(b), a.max(b)));
            }
        }
    }
    // No atom of the first two rows beyond its valence (C 4, N 3, O 2, a
    // halogen 1) plus one formal charge (the O of C≡O, the N of an
    // isocyanide, nitro or azide): a multiple bond that would put it further
    // is single (a crushed drawing makes short single bonds look triple).
    let valence = |i: usize| -> Option<f64> {
        let e = valence_electrons(z[i]);
        (z[i] <= 10 && !is_metal(z[i])).then(|| f64::from(e.min(8 - e)).clamp(1.0, 4.0))
    };
    for _ in 0..3 {
        let mut total = vec![0.0f64; n];
        for &(a, b, order) in &pairs {
            total[a] += order;
            total[b] += order;
        }
        for pair in pairs.iter_mut() {
            let (a, b, order) = *pair;
            let over = |i: usize| valence(i).is_some_and(|v| total[i] > v + 1.6);
            if order > 1.0 && !aromatic.contains(&(a.min(b), a.max(b))) && (over(a) || over(b)) {
                pair.2 = 1.0;
            }
        }
    }
    let mut orders = vec![0.0f64; n];
    let mut model_bonds = Vec::new();
    for &(a, b, order) in &pairs {
        let order = if aromatic.contains(&(a.min(b), a.max(b))) { 1.5 } else { order };
        // Bonds to metals keep their length unless ideal metals are asked
        // for (covalent radii are crude there: Fe-C 2.18 against 2.05 in
        // ferrocene).
        let metal_bond = is_metal(z[a]) || is_metal(z[b]);
        let length = if metal_bond && !options.ideal_metals {
            p[a].distance(p[b])
        } else {
            bond_lengths(z[a], z[b]).into_iter().find(|(o, _)| *o == order).map_or(p[a].distance(p[b]), |(_, l)| l)
        };
        if !is_metal(z[b]) || organic[a].contains(&b) {
            orders[a] += order;
        }
        if !is_metal(z[a]) || organic[b].contains(&a) {
            orders[b] += order;
        }
        model_bonds.push((a, b, length));
    }
    let domains = |i: usize| -> (usize, usize) {
        let lone = if is_metal(z[i]) { 0 } else { ((f64::from(valence_electrons(z[i])) - orders[i]).max(0.0) / 2.0).floor() as usize };
        let around = if is_metal(z[i]) { neighbors[i].len() } else { organic[i].len() };
        (around + lone, lone)
    };
    // The smallest ring holding the angle a-c-b, if it has 3, 4 or 5 atoms:
    // then the ring decides the angle.
    let ring_angle = |a: usize, c: usize, b: usize| -> Option<f64> {
        rings
            .iter()
            .filter(|r| r.len() <= 5 && r.contains(&a) && r.contains(&b) && r.contains(&c))
            .map(|r| r.len())
            .min()
            .map(|k| 180.0 * (k as f64 - 2.0) / k as f64)
    };
    let mut angles = Vec::new();
    let mut flat = Vec::new();
    for c in 0..n {
        let around = if is_metal(z[c]) { &neighbors[c] } else { &organic[c] };
        let k = around.len();
        if k < 2 {
            continue;
        }
        let (steric, lone) = domains(c);
        for x in 0..k {
            for y in (x + 1)..k {
                let (a, b) = (around[x], around[y]);
                let current = (p[a] - p[c]).angle_between(p[b] - p[c]).to_degrees();
                let nearest = |allowed: &[f64]| allowed.iter().copied().min_by(|u, v| (u - current).abs().total_cmp(&(v - current).abs())).unwrap();
                let target = if is_metal(z[c]) {
                    if !options.ideal_metals {
                        current
                    } else {
                        match k {
                            2 => 180.0,
                            3 => 120.0,
                            4 if current > 100.0 => 109.47,
                            4 => nearest(&[90.0, 180.0]),
                            5 => nearest(&[90.0, 120.0, 180.0]),
                            6 => nearest(&[90.0, 180.0]),
                            _ => current,
                        }
                    }
                } else if let Some(in_ring) = ring_angle(a, c, b) {
                    in_ring
                } else {
                    match steric {
                        2 => 180.0,
                        3 => 120.0,
                        // Lone pairs squeeze the bonds a little (NH3 107, H2O 104.5).
                        4 => 109.47 - 2.5 * lone as f64,
                        5 => nearest(&[90.0, 120.0, 180.0]),
                        6 => nearest(&[90.0, 180.0]),
                        _ => current,
                    }
                };
                angles.push((a, c, b, target.to_radians()));
            }
        }
        if k == 3 && lone == 0 && steric == 3 && !is_metal(z[c]) {
            flat.push([c, around[0], around[1], around[2]]);
        }
    }
    let mut pi = Vec::new();
    let flat_centres: std::collections::HashSet<usize> = flat.iter().map(|f| f[0]).collect();
    for &(b, c, order) in &pairs {
        let order = if aromatic.contains(&(b.min(c), b.max(c))) { 1.5 } else { order };
        if order < 1.5 || order > 2.0 || !flat_centres.contains(&b) || !flat_centres.contains(&c) {
            continue;
        }
        for &a in organic[b].iter().filter(|&&a| a != c) {
            for &d in organic[c].iter().filter(|&&d| d != b && d != a) {
                pi.push([a, b, c, d]);
            }
        }
    }
    let mut twists = Vec::new();
    if options.twist {
        for &(b, c, _) in &model_bonds {
            let single = bond_target(z[b], z[c], p[b].distance(p[c])).0 == 1.0;
            if !single || domains(b).0 != 4 || domains(c).0 != 4 || is_metal(z[b]) || is_metal(z[c]) {
                continue;
            }
            for &a in neighbors[b].iter().filter(|&&a| a != c) {
                for &d in neighbors[c].iter().filter(|&&d| d != b && d != a) {
                    twists.push([a, b, c, d]);
                }
            }
        }
    }
    // Pairs neither bonded nor sharing a neighbour.
    let mut close = vec![std::collections::HashSet::new(); n];
    for a in 0..n {
        for &b in &neighbors[a] {
            close[a].insert(b);
            for &c in &neighbors[b] {
                close[a].insert(c);
            }
        }
    }
    let mut repel = Vec::new();
    for a in 0..n {
        for b in (a + 1)..n {
            if !close[a].contains(&b) {
                let contact = 0.8 * f64::from(element_data(z[a]).vdw_radius + element_data(z[b]).vdw_radius);
                repel.push((a, b, contact));
            }
        }
    }
    Model { bonds: model_bonds, angles, flat, twists, pi, repel }
}

/// The rings of up to 8 atoms: for every bond, the shortest path between
/// its two atoms that doesn't use it (each ring once, as an atom list).
fn small_rings(neighbors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut rings: Vec<Vec<usize>> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for a in 0..neighbors.len() {
        for &b in neighbors[a].iter().filter(|&&b| b > a) {
            // Breadth first from b to a, not through the bond a-b.
            let mut previous = vec![usize::MAX; neighbors.len()];
            previous[b] = b;
            let mut queue = std::collections::VecDeque::from([b]);
            while let Some(u) = queue.pop_front() {
                if u == a {
                    break;
                }
                for &v in &neighbors[u] {
                    if previous[v] == usize::MAX && !(u == b && v == a) {
                        previous[v] = u;
                        queue.push_back(v);
                    }
                }
            }
            if previous[a] == usize::MAX {
                continue;
            }
            let mut ring = vec![a];
            let mut u = previous[a];
            while u != b {
                ring.push(u);
                u = previous[u];
            }
            ring.push(b);
            if ring.len() <= 8 {
                let mut key = ring.clone();
                key.sort_unstable();
                if seen.insert(key) {
                    rings.push(ring);
                }
            }
        }
    }
    rings
}

/// The energy and its gradient.
fn energy(model: &Model, p: &[DVec3], gradient: &mut [DVec3]) -> f64 {
    gradient.iter_mut().for_each(|g| *g = DVec3::ZERO);
    let mut e = 0.0;
    for &(a, b, r0) in &model.bonds {
        let d = p[a] - p[b];
        let r = d.length().max(1e-9);
        e += K_BOND * (r - r0).powi(2);
        let g = d * (2.0 * K_BOND * (r - r0) / r);
        gradient[a] += g;
        gradient[b] -= g;
    }
    for &(a, c, b, theta0) in &model.angles {
        let (u, v) = (p[a] - p[c], p[b] - p[c]);
        let (lu, lv) = (u.length().max(1e-9), v.length().max(1e-9));
        let cos = u.dot(v) / (lu * lv);
        let diff = cos - theta0.cos();
        e += K_ANGLE * diff * diff;
        let f = 2.0 * K_ANGLE * diff;
        // d cos / du = v/(|u||v|) − cos u/|u|²
        let du = (v / (lu * lv) - u * (cos / (lu * lu))) * f;
        let dv = (u / (lu * lv) - v * (cos / (lv * lv))) * f;
        gradient[a] += du;
        gradient[b] += dv;
        gradient[c] -= du + dv;
    }
    for &[c, a, b, d] in &model.flat {
        let (x, y, w) = (p[a] - p[c], p[b] - p[c], p[d] - p[c]);
        let volume = x.dot(y.cross(w));
        e += K_FLAT * volume * volume;
        let f = 2.0 * K_FLAT * volume;
        let (gx, gy, gw) = (y.cross(w) * f, w.cross(x) * f, x.cross(y) * f);
        gradient[a] += gx;
        gradient[b] += gy;
        gradient[d] += gw;
        gradient[c] -= gx + gy + gw;
    }
    // Twists: 1 + cos 3φ (staggered, lowest at 60 and 180 degrees) and
    // 1 − cos 2φ (conjugated, lowest at 0 and 180). Gradient by central
    // differences (few terms, all small).
    let twist_terms = model.twists.iter().map(|t| (t, K_TWIST, 3.0, 1.0)).chain(model.pi.iter().map(|t| (t, K_PI, 2.0, -1.0)));
    for (t, k_term, fold, sign) in twist_terms {
        let value = |q: &[DVec3; 4]| 1.0 + sign * (fold * dihedral(q)).cos();
        let q = [p[t[0]], p[t[1]], p[t[2]], p[t[3]]];
        e += k_term * value(&q);
        for (k, &atom) in t.iter().enumerate() {
            for axis in 0..3 {
                let h = 1e-5;
                let (mut plus, mut minus) = (q, q);
                plus[k][axis] += h;
                minus[k][axis] -= h;
                gradient[atom][axis] += k_term * (value(&plus) - value(&minus)) / (2.0 * h);
            }
        }
    }
    for &(a, b, contact) in &model.repel {
        let d = p[a] - p[b];
        let r = d.length();
        if r < contact {
            let r = r.max(1e-6);
            e += K_REPEL * (contact - r).powi(2);
            let g = d * (-2.0 * K_REPEL * (contact - r) / r);
            gradient[a] += g;
            gradient[b] -= g;
        }
    }
    e
}

fn dihedral(q: &[DVec3; 4]) -> f64 {
    let (b1, b2, b3) = (q[1] - q[0], q[2] - q[1], q[3] - q[2]);
    (b2.length() * b1.dot(b2.cross(b3))).atan2(b1.cross(b2).dot(b2.cross(b3)))
}

/// L-BFGS with a backtracking line search, from `x`: `eval` gives the
/// energy and fills the gradient, `project` limits the gradient (held atoms,
/// symmetry), `place` limits the positions (symmetry). No atom moves more
/// than 0.3 angstrom in one step.
pub(crate) fn minimize(
    mut x: Vec<DVec3>,
    mut eval: impl FnMut(&[DVec3], &mut [DVec3]) -> f64,
    project: &dyn Fn(&mut Vec<DVec3>),
    place: &dyn Fn(Vec<DVec3>) -> Vec<DVec3>,
    max_steps: usize,
) -> (Vec<DVec3>, usize) {
    let n = x.len();
    let mut g = vec![DVec3::ZERO; n];
    let mut e = eval(&x, &mut g);
    project(&mut g);
    let memory = 8;
    let mut history: Vec<(Vec<DVec3>, Vec<DVec3>, f64)> = Vec::new();
    let dot = |a: &[DVec3], b: &[DVec3]| a.iter().zip(b).map(|(u, v)| u.dot(*v)).sum::<f64>();
    let mut steps = 0;
    for _ in 0..max_steps {
        let gnorm = dot(&g, &g).sqrt();
        if gnorm < 1e-5 {
            break;
        }
        // Two-loop recursion.
        let mut q = g.clone();
        let mut alphas = Vec::with_capacity(history.len());
        for (s, y, rho) in history.iter().rev() {
            let a = rho * dot(s, &q);
            q.iter_mut().zip(y).for_each(|(qi, yi)| *qi -= *yi * a);
            alphas.push(a);
        }
        if let Some((s, y, _)) = history.last() {
            let gamma = dot(s, y) / dot(y, y).max(1e-30);
            q.iter_mut().for_each(|qi| *qi *= gamma);
        } else {
            let scale = 0.1 / gnorm.max(1e-12);
            q.iter_mut().for_each(|qi| *qi *= scale);
        }
        for ((s, y, rho), a) in history.iter().zip(alphas.iter().rev()) {
            let b = rho * dot(y, &q);
            q.iter_mut().zip(s).for_each(|(qi, si)| *qi += *si * (a - b));
        }
        let mut direction: Vec<DVec3> = q.iter().map(|v| -*v).collect();
        project(&mut direction);
        if dot(&direction, &g) >= 0.0 {
            direction = g.iter().map(|v| -*v * (0.1 / gnorm)).collect();
            history.clear();
        }
        let longest = direction.iter().map(|v| v.length()).fold(0.0, f64::max);
        let mut t = if longest > 0.3 { 0.3 / longest } else { 1.0 };
        let slope = dot(&direction, &g);
        let mut accepted = None;
        for _ in 0..30 {
            let trial = place(x.iter().zip(&direction).map(|(xi, di)| *xi + *di * t).collect());
            let mut trial_g = vec![DVec3::ZERO; n];
            let trial_e = eval(&trial, &mut trial_g);
            if trial_e <= e + 1e-4 * t * slope {
                project(&mut trial_g);
                accepted = Some((trial, trial_e, trial_g));
                break;
            }
            t *= 0.5;
        }
        let Some((new_x, new_e, new_g)) = accepted else { break };
        let s: Vec<DVec3> = new_x.iter().zip(&x).map(|(a, b)| *a - *b).collect();
        let y: Vec<DVec3> = new_g.iter().zip(&g).map(|(a, b)| *a - *b).collect();
        let sy = dot(&s, &y);
        if sy > 1e-12 {
            history.push((s, y, 1.0 / sy));
            if history.len() > memory {
                history.remove(0);
            }
        }
        steps += 1;
        let converged = (e - new_e).abs() < 1e-10;
        x = new_x;
        e = new_e;
        g = new_g;
        if converged {
            break;
        }
    }
    (x, steps)
}

/// The geometry cleaned (see the module's description). `bonds` are the
/// bonds drawn, as 0-based pairs.
pub fn clean(z: &[u32], positions: &[Vec3], bonds: &[(usize, usize)], options: &CleanOptions) -> Cleaned {
    let start: Vec<DVec3> = positions.iter().map(|v| v.as_dvec3()).collect();
    let n = start.len();
    if n == 0 {
        return Cleaned { positions: Vec::new(), largest_move: 0.0, steps: 0 };
    }
    let model = build_model(z, &start, bonds, options);
    let mut held = vec![false; n];
    for &h in &options.held {
        if h < n {
            held[h] = true;
        }
    }
    let project = |g: &mut Vec<DVec3>| {
        if let Some(s) = &options.symmetry {
            *g = s.gradient(g);
        }
        for (i, gi) in g.iter_mut().enumerate() {
            if held[i] {
                *gi = DVec3::ZERO;
            }
        }
    };
    let x0 = match &options.symmetry {
        Some(s) => s.positions(&start),
        None => start.clone(),
    };
    let symmetrize = |x: Vec<DVec3>| match &options.symmetry {
        Some(s) => s.positions(&x),
        None => x,
    };
    let (x, steps) = minimize(x0, |x, g| energy(&model, x, g), &project, &symmetrize, 1000);
    let largest_move = x.iter().zip(&start).map(|(a, b)| a.distance(*b)).fold(0.0, f64::max);
    Cleaned { positions: x.iter().map(|v| v.as_vec3()).collect(), largest_move, steps }
}

#[cfg(test)]
mod tests;
