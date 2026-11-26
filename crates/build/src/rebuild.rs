//! Rebuild: a sensible 3D structure from the bonds alone, for drawings too
//! poor for Clean (everything flat, atoms on top of each other, rings folded
//! through themselves). The coordinates are used only for the handedness of
//! stereocentres the drawing defines, and to keep the result where the
//! drawing was.
//!
//! Distance geometry, as RDKit and Open Babel turn 2D drawings into 3D:
//! target distances for bonded pairs (bond lengths) and for pairs two bonds
//! apart (from the VSEPR or ring angle between them), a contact lower bound
//! for every other pair; atoms placed at random and moved to match the
//! targets first, then the bounds too (so tangles unwind before atoms block
//! each other); the best of several starts kept, then cleaned.

use glam::{DVec3, Vec3};

use crate::clean::{build_model, clean, minimize, CleanOptions};

const K_CHIRAL: f64 = 5.0;

/// A structure rebuilt from its bonds: `bonds` are 0-based pairs,
/// `positions` the drawing, `starts` how many random starts to try.
pub fn rebuild(z: &[u32], positions: &[Vec3], bonds: &[(usize, usize)], starts: usize) -> Vec<Vec3> {
    let n = z.len();
    if n < 3 {
        return clean(z, positions, bonds, &CleanOptions { ideal_metals: true, twist: true, ..Default::default() }).positions;
    }
    // The drawing scaled so its bonds have their typical single-bond mean
    // length: bond types are then read from sensible distances (a crushed
    // drawing's 0.76 angstrom C-C isn't a triple bond).
    let raw: Vec<DVec3> = positions.iter().map(|v| v.as_dvec3()).collect();
    let single = |a: usize, b: usize| f64::from(apost3dview_core::element_data(z[a]).covalent_radius + apost3dview_core::element_data(z[b]).covalent_radius);
    let valid: Vec<(usize, usize)> = bonds.iter().copied().filter(|&(a, b)| a < n && b < n && a != b).collect();
    let drawn_mean = valid.iter().map(|&(a, b)| raw[a].distance(raw[b])).sum::<f64>() / valid.len().max(1) as f64;
    let single_mean = valid.iter().map(|&(a, b)| single(a, b)).sum::<f64>() / valid.len().max(1) as f64;
    let scale = if drawn_mean > 1e-6 { single_mean / drawn_mean } else { 1.0 };
    let center = raw.iter().copied().sum::<DVec3>() / n as f64;
    let drawing: Vec<DVec3> = raw.iter().map(|v| (*v - center) * scale).collect();
    let model = build_model(z, &drawing, bonds, &CleanOptions { ideal_metals: true, twist: true, ..Default::default() });
    // Exact targets: bonds, and pairs two bonds apart from their angle.
    let length = |a: usize, b: usize| model.bonds.iter().find(|&&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a)).map(|&(_, _, l)| l);
    let mut exact: Vec<(usize, usize, f64)> = model.bonds.clone();
    // Angles at a metal with more than six neighbours (η ligands) are taken
    // from the drawing: no targets from them.
    let crowded_metal = |c: usize| crate::clean::is_metal(z[c]) && bonds.iter().filter(|&&(a, b)| a == c || b == c).count() > 6;
    for &(a, c, b, theta) in &model.angles {
        if crowded_metal(c) {
            continue;
        }
        if let (Some(l1), Some(l2)) = (length(a, c), length(c, b)) {
            exact.push((a, b, (l1 * l1 + l2 * l2 - 2.0 * l1 * l2 * theta.cos()).max(0.0).sqrt()));
        }
    }
    // Handedness of stereocentres (four neighbours) where the drawing is
    // clearly three-dimensional at them.
    let mut neighbors = vec![Vec::new(); n];
    for &(a, b, _) in &model.bonds {
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    let chiral: Vec<([usize; 4], f64)> = (0..n)
        .filter(|&c| neighbors[c].len() == 4)
        .filter_map(|c| {
            let [a, b, d] = [neighbors[c][0], neighbors[c][1], neighbors[c][2]];
            let volume = (drawing[a] - drawing[c]).dot((drawing[b] - drawing[c]).cross(drawing[d] - drawing[c]));
            let scale = (drawing[a] - drawing[c]).length() * (drawing[b] - drawing[c]).length() * (drawing[d] - drawing[c]).length();
            (volume.abs() > 0.2 * scale).then_some(([c, a, b, d], volume.signum()))
        })
        .collect();
    // Stage 1 (shape) pulls only toward the targets; stage 2 adds the
    // contacts and the handedness.
    let stress = |x: &[DVec3], g: &mut [DVec3], bounds: bool| -> f64 {
        g.iter_mut().for_each(|v| *v = DVec3::ZERO);
        let mut e = 0.0;
        for &(a, b, d0) in &exact {
            let d = x[a] - x[b];
            let r = d.length().max(1e-9);
            e += (r - d0).powi(2);
            let f = d * (2.0 * (r - d0) / r);
            g[a] += f;
            g[b] -= f;
        }
        if bounds {
            for &(a, b, contact) in &model.repel {
                let d = x[a] - x[b];
                let r = d.length();
                if r < contact {
                    let r = r.max(1e-6);
                    e += (contact - r).powi(2);
                    let f = d * (-2.0 * (contact - r) / r);
                    g[a] += f;
                    g[b] -= f;
                }
            }
        }
        for &([c, a, b, d], sign) in chiral.iter().filter(|_| bounds) {
            let (u, v, w) = (x[a] - x[c], x[b] - x[c], x[d] - x[c]);
            let volume = u.dot(v.cross(w));
            let wrong = (-sign * volume).max(0.0);
            if wrong > 0.0 {
                e += K_CHIRAL * wrong * wrong;
                let f = -2.0 * K_CHIRAL * wrong * sign;
                let (gu, gv, gw) = (v.cross(w) * f, w.cross(u) * f, u.cross(v) * f);
                g[a] += gu;
                g[b] += gv;
                g[d] += gw;
                g[c] -= gu + gv + gw;
            }
        }
        e
    };
    let none = |_: &mut Vec<DVec3>| {};
    let same = |x: Vec<DVec3>| x;
    let side = 1.5 * (n as f64).cbrt() + 2.0;
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut random = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 1_000_001) as f64 / 1_000_000.0 - 0.5
    };
    // The first start is the (scaled) drawing itself: a crushed or
    // stretched drawing still says a lot. The others are random.
    let mut best: Option<(f64, Vec<DVec3>)> = None;
    for k in 0..starts.max(1) {
        let start: Vec<DVec3> = if k == 0 {
            drawing.iter().map(|v| *v + DVec3::new(random(), random(), random()) * 0.1).collect()
        } else {
            (0..n).map(|_| DVec3::new(random(), random(), random()) * side).collect()
        };
        let (shaped, _) = minimize(start, |x, g| stress(x, g, false), &none, &same, 2000);
        let (placed, _) = minimize(shaped, |x, g| stress(x, g, true), &none, &same, 2000);
        let mut scratch = vec![DVec3::ZERO; n];
        let score = stress(&placed, &mut scratch, true);
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, placed));
        }
    }
    let placed: Vec<Vec3> = best.unwrap().1.iter().map(|v| v.as_vec3()).collect();
    let cleaned = clean(z, &placed, bonds, &CleanOptions { ideal_metals: true, twist: true, ..Default::default() }).positions;
    // Turned and moved to lie where the drawing was (best rigid fit).
    match apost3dview_core::superpose(positions, &cleaned) {
        Some(fit) => cleaned.iter().map(|v| fit.rotation * *v + fit.translation).collect(),
        None => cleaned,
    }
}

#[cfg(test)]
mod tests;
