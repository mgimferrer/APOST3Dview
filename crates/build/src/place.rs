//! Adding atoms, groups, ligands and molecules: where they go.
//!
//! A group (or ligand) has one atom that bonds to the molecule, and the
//! direction from that atom toward the atom it bonds to. It goes either in
//! place of a hydrogen (along the old bond), or on an atom's free direction
//! (the empty corner of its electron domains: neighbours and lone pairs),
//! at the typical length of the new bond (Clean's table, so Clean agrees).
//! It is then turned about the new bond to the turn with the most room
//! (every 10°, then every 1° near the best), which also makes it staggered.
//! A molecule (or a group) placed in empty space goes where it is put; a
//! group placed there gets a hydrogen on its open bond.
//!
//! Everything is computed in `f64` and is repeatable: the same placement
//! gives the same coordinates.

use apost3dview_core::{element_data, perceive_bonds, Bond, Molecule};
use glam::{DQuat, DVec3, Vec3};

use crate::clean::{bond_lengths, bond_target, is_metal, valence_electrons};

/// What a fragment is, which decides what happens in empty space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentKind {
    /// Bonds through one atom; in empty space it gets a hydrogen there.
    Group,
    /// Bonds through a donor atom; a molecule of its own in empty space.
    Ligand,
    /// A whole molecule: always placed as a separate piece.
    Molecule,
}

/// How a fragment bonds to a molecule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attachment {
    /// The fragment's atom that bonds (0-based).
    pub atom: usize,
    /// Unit vector from that atom toward the atom it bonds to.
    pub toward: Vec3,
    /// Order of the new bond (its typical length comes from it).
    pub order: f64,
}

/// Something that can be added: its atoms (angstrom, any frame) and how
/// it bonds.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    pub kind: FragmentKind,
    pub atomic_numbers: Vec<u32>,
    pub positions: Vec<Vec3>,
    pub attach: Option<Attachment>,
}

/// Where a fragment goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Site {
    /// In place of this atom (a hydrogen: bonded to exactly one atom).
    Replace(usize),
    /// Bonded to this atom, on its free direction.
    On(usize),
    /// As a separate piece, centred at this point.
    Free(Vec3),
}

/// The viewer's directions (world, unit vectors), for the choices geometry
/// leaves open: the first group on a lone atom goes to the right, a group
/// on a flat atom goes out of its plane toward the viewer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub right: Vec3,
    pub toward_viewer: Vec3,
}

impl Default for View {
    fn default() -> Self {
        Self { right: Vec3::X, toward_viewer: Vec3::Z }
    }
}

/// A molecule after an addition (or a change of hydrogens), with where
/// every old atom and bond went (`None`: removed, e.g. the replaced
/// hydrogen) and the atoms added. New bonds come after all old ones.
#[derive(Clone)]
pub struct Placement {
    pub molecule: Molecule,
    pub atom_map: Vec<Option<usize>>,
    pub bond_map: Vec<Option<usize>>,
    pub added: Vec<usize>,
}

impl Placement {
    /// Nothing changed.
    fn unchanged(molecule: &Molecule) -> Self {
        Self {
            molecule: molecule.clone(),
            atom_map: (0..molecule.atomic_numbers.len()).map(Some).collect(),
            bond_map: (0..molecule.bonds.len()).map(Some).collect(),
            added: Vec::new(),
        }
    }
}

/// The usual number of bonds of a main-group element (H 1, B 3, C 4, N 3,
/// O 2, F 1, Si 4, P 3, S 2, ...); `None` for metals, 0 for noble gases.
pub fn usual_valence(z: u32) -> Option<u32> {
    if is_metal(z) {
        return None;
    }
    if z == 1 {
        return Some(1);
    }
    let e = valence_electrons(z);
    Some(if e >= 8 || z == 2 { 0 } else if e <= 4 { e } else { 8 - e })
}

/// The typical length of a bond of `order` between `a` and `b` (Clean's
/// table; single when the pair has no such order).
pub fn bond_length(a: u32, b: u32, order: f64) -> f64 {
    let lengths = bond_lengths(a, b);
    lengths.iter().find(|(o, _)| (*o - order).abs() < 1e-9).unwrap_or(&lengths[0]).1
}

/// Unit directions of `steric` electron domains around an atom, the first
/// along +Z: linear, trigonal, tetrahedral, then octahedral-like.
fn ideal_directions(steric: usize) -> Vec<DVec3> {
    let polar = |theta: f64, phi: f64| {
        let (t, p) = (theta.to_radians(), phi.to_radians());
        DVec3::new(t.sin() * p.cos(), t.sin() * p.sin(), t.cos())
    };
    match steric {
        0 | 1 => vec![DVec3::Z],
        2 => vec![DVec3::Z, -DVec3::Z],
        3 => vec![DVec3::Z, polar(120.0, 0.0), polar(120.0, 180.0)],
        4 => vec![DVec3::Z, polar(109.471_220_634, 0.0), polar(109.471_220_634, 120.0), polar(109.471_220_634, 240.0)],
        _ => vec![DVec3::Z, DVec3::X, DVec3::Y, -DVec3::X, -DVec3::Y, -DVec3::Z],
    }
}

/// An element with its hydrogens, bonding through the element: C gives
/// CH3, N gives NH2, O gives OH, F gives F, B gives BH2. A metal or a noble
/// gas is the atom alone.
pub fn element_fragment(z: u32) -> Fragment {
    let valence = usual_valence(z).unwrap_or(1).max(1);
    let hydrogens = valence as usize - 1;
    let lone = if is_metal(z) || z == 1 { 0 } else { valence_electrons(z).saturating_sub(valence) as usize / 2 };
    let directions = ideal_directions(1 + hydrogens + lone);
    let length = bond_length(z, 1, 1.0);
    let mut atomic_numbers = vec![z];
    let mut positions = vec![Vec3::ZERO];
    for d in directions.iter().skip(1).take(hydrogens) {
        atomic_numbers.push(1);
        positions.push((*d * length).as_vec3());
    }
    Fragment { kind: FragmentKind::Group, atomic_numbers, positions, attach: Some(Attachment { atom: 0, toward: Vec3::Z, order: 1.0 }) }
}

/// Neighbour lists from bond pairs.
fn neighbor_lists(n: usize, bonds: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut neighbors = vec![Vec::new(); n];
    for &(a, b) in bonds {
        if a < n && b < n && a != b && !neighbors[a].contains(&b) {
            neighbors[a].push(b);
            neighbors[b].push(a);
        }
    }
    neighbors
}

/// A unit vector perpendicular to `u`, as close as possible to `prefer`
/// (then to the other fallbacks).
fn perpendicular(u: DVec3, prefer: &[DVec3]) -> DVec3 {
    for p in prefer.iter().chain([DVec3::X, DVec3::Y, DVec3::Z].iter()) {
        let v = *p - u * p.dot(u);
        if v.length() > 1e-3 {
            return v.normalize();
        }
    }
    DVec3::X
}

/// Whether the bond a-b is in a ring (a and b still joined without it).
fn in_ring(neighbors: &[Vec<usize>], a: usize, b: usize) -> bool {
    let mut seen = vec![false; neighbors.len()];
    let mut stack = vec![a];
    seen[a] = true;
    while let Some(i) = stack.pop() {
        for &j in &neighbors[i] {
            if (i == a && j == b) || seen[j] {
                continue;
            }
            if j == b {
                return true;
            }
            seen[j] = true;
            stack.push(j);
        }
    }
    false
}

/// The order of the bond `atom`-`j` read from its length as Clean reads it,
/// aromatic (1.5) only in a ring.
fn bond_order(z: &[u32], p: &[DVec3], neighbors: &[Vec<usize>], atom: usize, element: u32, j: usize) -> f64 {
    let distance = p[atom].distance(p[j]);
    if in_ring(neighbors, atom, j) {
        return bond_target(element, z[j], distance).0;
    }
    bond_lengths(element, z[j])
        .into_iter()
        .filter(|(order, _)| (*order - 1.5).abs() > 1e-9)
        .min_by(|x, y| (x.1 - distance).abs().total_cmp(&(y.1 - distance).abs()))
        .map_or(1.0, |(order, _)| order)
}

/// The sum of bond orders from `atom` to `around` (non-metal neighbours
/// only), each read from the distance.
fn bond_order_sum(z: &[u32], p: &[DVec3], neighbors: &[Vec<usize>], around: &[usize], atom: usize, element: u32) -> f64 {
    around.iter().filter(|&&j| !is_metal(z[j])).map(|&j| bond_order(z, p, neighbors, atom, element, j)).sum()
}

/// The direction (unit, from `atom`) a new bond of `order` takes: the empty
/// corner of the atom's electron domains (neighbours, the new bond and its
/// lone pairs). With one neighbour the new bond is anti to that
/// neighbour's other bonds (zigzag chains); a flat atom gets its plane's
/// normal, toward the viewer.
pub fn free_direction(atomic_numbers: &[u32], positions: &[Vec3], bonds: &[(usize, usize)], atom: usize, order: f64, view: View) -> Vec3 {
    let p: Vec<DVec3> = positions.iter().map(|v| v.as_dvec3()).collect();
    let neighbors = neighbor_lists(p.len(), bonds);
    let z = atomic_numbers;
    let mine = &neighbors[atom];
    let toward = view.toward_viewer.as_dvec3();
    let right = view.right.as_dvec3();
    let units: Vec<DVec3> = mine.iter().map(|&j| (p[j] - p[atom]).normalize_or_zero()).filter(|u| *u != DVec3::ZERO).collect();
    if units.is_empty() {
        return view.right;
    }
    let metal = is_metal(z[atom]);
    // Electron domains once the atom is complete: its neighbours, the new
    // bond, the bonds its valence still asks for (one domain each: the
    // hydrogens to come) and its lone pairs.
    let (later, lone) = if metal {
        (0, 0)
    } else {
        let orders = bond_order_sum(z, &p, &neighbors, mine, atom, z[atom]);
        let later = (f64::from(usual_valence(z[atom]).unwrap_or(0)) - orders - order + 0.25).floor().max(0.0);
        let lone = ((f64::from(valence_electrons(z[atom])) - orders - order - later).max(0.0) / 2.0).floor();
        (later as usize, lone as usize)
    };
    let steric = units.len() + 1 + later + lone;
    let sum: DVec3 = units.iter().copied().sum();
    let direction = if units.len() == 1 {
        let u = units[0];
        let theta: f64 = match steric {
            0..=2 => 180.0,
            3 => 120.0,
            4 => 109.471_220_634,
            _ => 90.0,
        };
        // Anti to the neighbour's other bonds.
        let neighbor = mine[0];
        let anti: Vec<DVec3> = neighbors[neighbor]
            .iter()
            .filter(|&&j| j != atom)
            .map(|&j| p[j] - p[neighbor])
            .map(|r| -(r - u * r.dot(u)))
            .filter(|v| v.length() > 1e-3)
            .take(1)
            .collect();
        let side = perpendicular(u, &[anti.first().copied().unwrap_or(DVec3::ZERO), toward, right]);
        u * theta.to_radians().cos() + side * theta.to_radians().sin()
    } else {
        let normal = plane_normal(&units, toward, right);
        if !metal && units.len() == 2 && steric == 4 && sum.length() > 0.1 {
            // Two of a tetrahedron's corners are taken: the new bond takes
            // one of the other two (the one toward the viewer).
            let half = 54.735_610_317_f64.to_radians();
            -sum.normalize() * half.cos() + normal * half.sin()
        } else if sum.length() > 0.25 {
            -sum.normalize()
        } else {
            normal
        }
    };
    direction.normalize().as_vec3()
}

/// The normal of the plane the unit vectors lie in (or closest to), toward
/// the viewer.
fn plane_normal(units: &[DVec3], toward: DVec3, right: DVec3) -> DVec3 {
    let mut m = DVec3::ZERO;
    for i in 0..units.len() {
        for j in (i + 1)..units.len() {
            let mut c = units[i].cross(units[j]);
            if m.dot(c) < 0.0 {
                c = -c;
            }
            m += c;
        }
    }
    let m = if m.length() < 1e-6 { perpendicular(units[0], &[toward, right]) } else { m.normalize() };
    if m.dot(toward) < 0.0 { -m } else { m }
}

/// The rotation turning unit vector `from` onto unit vector `to` (exact
/// also for tiny and opposite turns).
fn rotation_between(from: DVec3, to: DVec3) -> DQuat {
    let axis = from.cross(to);
    let angle = axis.length().atan2(from.dot(to));
    if axis.length() < 1e-12 {
        if from.dot(to) > 0.0 {
            return DQuat::IDENTITY;
        }
        return DQuat::from_axis_angle(perpendicular(from, &[]), std::f64::consts::PI);
    }
    DQuat::from_axis_angle(axis.normalize(), angle)
}

/// How crowded the placed atoms are against the rest: a soft r⁻⁶ sum.
fn crowding(placed: &[(u32, DVec3)], rest: &[(u32, DVec3)]) -> f64 {
    let mut total = 0.0;
    for (zi, pi) in placed {
        for (zj, pj) in rest {
            let sigma = 0.5 * f64::from(element_data(*zi).vdw_radius + element_data(*zj).vdw_radius);
            let d2 = pi.distance_squared(*pj).max(1e-6);
            total += (sigma * sigma / d2).powi(3);
        }
    }
    total
}

/// Adds `fragment` to `molecule` at `site`. `bonds` are the bonds drawn
/// (they decide neighbours and the free direction); every bond of the
/// molecule is kept except those of a replaced atom.
pub fn place(molecule: &Molecule, bonds: &[(usize, usize)], fragment: &Fragment, site: Site, view: View) -> Result<Placement, String> {
    let n = molecule.atomic_numbers.len();
    if fragment.atomic_numbers.is_empty() || fragment.atomic_numbers.len() != fragment.positions.len() {
        return Err("This group has no atoms.".into());
    }
    let z = &molecule.atomic_numbers;
    let neighbors = neighbor_lists(n, bonds);
    let frag: Vec<DVec3> = fragment.positions.iter().map(|v| v.as_dvec3()).collect();
    let mut add_z = fragment.atomic_numbers.clone();
    let (removed, placed, host): (Option<usize>, Vec<DVec3>, Option<(usize, usize)>) = match (site, fragment.attach, fragment.kind) {
        (Site::Free(point), attach, kind) => {
            let mut atoms = frag.clone();
            if let (Some(a), FragmentKind::Group) = (attach, kind) {
                // An open bond in empty space gets a hydrogen.
                let length = bond_length(add_z[a.atom], 1, 1.0);
                atoms.push(frag[a.atom] + a.toward.as_dvec3().normalize() * length);
                add_z.push(1);
            }
            let centre = atoms.iter().copied().sum::<DVec3>() / atoms.len() as f64;
            (None, atoms.iter().map(|q| *q - centre + point.as_dvec3()).collect(), None)
        }
        (_, None, _) | (_, _, FragmentKind::Molecule) => return Err("A whole molecule is added in empty space, as a separate piece.".into()),
        (Site::Replace(h), Some(attach), _) | (Site::On(h), Some(attach), _) => {
            if h >= n {
                return Err("That atom isn't in the structure.".into());
            }
            let p = |i: usize| molecule.positions[i].as_dvec3();
            let (host, removed, direction) = match site {
                Site::Replace(_) => {
                    if neighbors[h].len() != 1 {
                        return Err("Only an atom bonded to exactly one other can be replaced.".into());
                    }
                    let host = neighbors[h][0];
                    (host, Some(h), (p(h) - p(host)).normalize())
                }
                _ => (h, None, free_direction(z, &molecule.positions, bonds, h, attach.order, view).as_dvec3()),
            };
            let length = bond_length(z[host], add_z[attach.atom], attach.order);
            let anchor = p(host) + direction * length;
            let lined_up = rotation_between(attach.toward.as_dvec3().normalize(), -direction);
            let relative: Vec<DVec3> = frag.iter().map(|q| lined_up * (*q - frag[attach.atom])).collect();
            let rest: Vec<(u32, DVec3)> = (0..n).filter(|&i| Some(i) != removed).map(|i| (z[i], p(i))).collect();
            let turned = |degrees: f64| -> Vec<DVec3> {
                let turn = DQuat::from_axis_angle(direction, degrees.to_radians());
                relative.iter().map(|q| anchor + turn * *q).collect()
            };
            let score = |degrees: f64| {
                let atoms = turned(degrees);
                let moving: Vec<(u32, DVec3)> = atoms.iter().enumerate().filter(|(k, _)| *k != attach.atom).map(|(k, q)| (add_z[k], *q)).collect();
                crowding(&moving, &rest)
            };
            let mut best = (0.0, score(0.0));
            for step in 1..36 {
                let degrees = f64::from(step) * 10.0;
                let s = score(degrees);
                if s < best.1 - 1e-12 {
                    best = (degrees, s);
                }
            }
            let coarse = best.0;
            for step in -9..=9 {
                let degrees = coarse + f64::from(step);
                let s = score(degrees);
                if s < best.1 - 1e-12 {
                    best = (degrees, s);
                }
            }
            (removed, turned(best.0), Some((host, attach.atom)))
        }
    };
    // Old atoms (minus a replaced one), then the new ones.
    let mut atom_map = vec![None; n];
    let mut atomic_numbers = Vec::with_capacity(n + add_z.len());
    let mut positions = Vec::with_capacity(n + add_z.len());
    for i in 0..n {
        if Some(i) != removed {
            atom_map[i] = Some(atomic_numbers.len());
            atomic_numbers.push(z[i]);
            positions.push(molecule.positions[i]);
        }
    }
    let first_new = atomic_numbers.len();
    for (k, q) in placed.iter().enumerate() {
        atomic_numbers.push(add_z[k]);
        positions.push(q.as_vec3());
    }
    let mut bond_map = vec![None; molecule.bonds.len()];
    let mut new_bonds = Vec::new();
    for (k, b) in molecule.bonds.iter().enumerate() {
        if let (Some(a), Some(c)) = (atom_map[b.atom_a], atom_map[b.atom_b]) {
            bond_map[k] = Some(new_bonds.len());
            new_bonds.push(Bond { atom_a: a, atom_b: c });
        }
    }
    let own: Vec<Vec3> = placed.iter().map(|q| q.as_vec3()).collect();
    for b in perceive_bonds(&add_z, &own) {
        new_bonds.push(Bond { atom_a: first_new + b.atom_a, atom_b: first_new + b.atom_b });
    }
    if let Some((host, attach)) = host {
        new_bonds.push(Bond { atom_a: atom_map[host].expect("the host stays"), atom_b: first_new + attach });
    }
    let added = (first_new..atomic_numbers.len()).collect();
    Ok(Placement { molecule: Molecule { atomic_numbers, positions, bonds: new_bonds }, atom_map, bond_map, added })
}

/// Hydrogens an atom should have for its usual valence, given its other
/// bonds (orders read from the distances, aromatic 1.5); `None` for metals.
fn wanted_hydrogens(z: &[u32], p: &[DVec3], neighbors: &[Vec<usize>], atom: usize, element: u32) -> Option<usize> {
    let valence = usual_valence(element)?;
    let heavy: Vec<usize> = neighbors[atom].iter().copied().filter(|&j| z[j] != 1 || neighbors[j].len() != 1).collect();
    let orders = bond_order_sum(z, p, neighbors, &heavy, atom, element);
    Some((f64::from(valence) - orders + 0.25).floor().max(0.0) as usize)
}

/// The hydrogens bonded to `atom` alone (terminal ones).
fn terminal_hydrogens(z: &[u32], neighbors: &[Vec<usize>], atom: usize) -> Vec<usize> {
    neighbors[atom].iter().copied().filter(|&j| z[j] == 1 && neighbors[j].len() == 1).collect()
}

/// Composes two placements done one after the other.
fn then(first: Placement, second: Placement) -> Placement {
    let atom_map = first.atom_map.iter().map(|a| a.and_then(|a| second.atom_map[a])).collect();
    let bond_map = first.bond_map.iter().map(|b| b.and_then(|b| second.bond_map[b])).collect();
    let mut added: Vec<usize> = first.added.iter().filter_map(|&a| second.atom_map[a]).collect();
    added.extend(second.added);
    Placement { molecule: second.molecule, atom_map, bond_map, added }
}

/// The bonds of a placement's molecule as pairs: the old drawn bonds
/// carried over, and every new bond.
fn carried_bonds(placement: &Placement, drawn: &[(usize, usize)], old_bonds: &[Bond]) -> Vec<(usize, usize)> {
    let drawn: std::collections::HashSet<(usize, usize)> = drawn.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
    let carried = placement.bond_map.iter().filter_map(|b| *b).count();
    let mut out = Vec::new();
    for (k, b) in old_bonds.iter().enumerate() {
        if drawn.contains(&(b.atom_a.min(b.atom_b), b.atom_a.max(b.atom_b)))
            && let Some(nb) = placement.bond_map[k]
        {
            let bond = &placement.molecule.bonds[nb];
            out.push((bond.atom_a, bond.atom_b));
        }
    }
    out.extend(placement.molecule.bonds[carried..].iter().map(|b| (b.atom_a, b.atom_b)));
    out
}

/// Gives `atoms` (all atoms when empty) the hydrogens their usual valence
/// asks for, each on a free direction. Metals and hydrogens are left alone.
pub fn add_hydrogens(molecule: &Molecule, bonds: &[(usize, usize)], atoms: &[usize], view: View) -> Placement {
    let mut result = Placement::unchanged(molecule);
    let mut drawn = bonds.to_vec();
    let targets: Vec<usize> = if atoms.is_empty() { (0..molecule.atomic_numbers.len()).collect() } else { atoms.to_vec() };
    let hydrogen = element_fragment(1);
    for &original in &targets {
        let Some(Some(atom)) = result.atom_map.get(original).copied() else { continue };
        let element = result.molecule.atomic_numbers[atom];
        if element == 1 {
            continue;
        }
        loop {
            let z = &result.molecule.atomic_numbers;
            let p: Vec<DVec3> = result.molecule.positions.iter().map(|v| v.as_dvec3()).collect();
            let neighbors = neighbor_lists(z.len(), &drawn);
            let Some(wanted) = wanted_hydrogens(z, &p, &neighbors, atom, element) else { break };
            if terminal_hydrogens(z, &neighbors, atom).len() >= wanted {
                break;
            }
            let before = result.molecule.bonds.clone();
            let Ok(step) = place(&result.molecule, &drawn, &hydrogen, Site::On(atom), view) else { break };
            drawn = carried_bonds(&step, &drawn, &before);
            result = then(result, step);
        }
    }
    result
}

/// Changes `atom` to element `atomic_number`; with `adjust_hydrogens`,
/// adds or removes its hydrogens to fit the new element's usual valence (C
/// to N on a CH3 gives NH2).
pub fn change_element_adjusting(molecule: &Molecule, bonds: &[(usize, usize)], atom: usize, atomic_number: u32, adjust_hydrogens: bool, view: View) -> Placement {
    let mut changed = molecule.clone();
    changed.atomic_numbers[atom] = atomic_number;
    let mut result = Placement::unchanged(&changed);
    if !adjust_hydrogens || atomic_number == 1 {
        return result;
    }
    let p: Vec<DVec3> = changed.positions.iter().map(|v| v.as_dvec3()).collect();
    let neighbors = neighbor_lists(p.len(), bonds);
    let Some(wanted) = wanted_hydrogens(&changed.atomic_numbers, &p, &neighbors, atom, atomic_number) else { return result };
    let hydrogens = terminal_hydrogens(&changed.atomic_numbers, &neighbors, atom);
    if hydrogens.len() > wanted {
        let extra: Vec<usize> = hydrogens[wanted..].to_vec();
        let deleted = crate::delete_atoms(&changed, &extra);
        result = Placement { molecule: deleted.molecule, atom_map: deleted.atom_map, bond_map: deleted.bond_map, added: Vec::new() };
        return result;
    }
    let more = add_hydrogens(&changed, bonds, &[atom], view);
    result = then(result, more);
    result
}

/// The atoms of the selection as a group: exactly one drawn bond must leave
/// `selection`, and its end inside is the attachment atom.
pub fn group_from_selection(molecule: &Molecule, bonds: &[(usize, usize)], selection: &[usize]) -> Result<Fragment, String> {
    let inside: std::collections::HashSet<usize> = selection.iter().copied().collect();
    let leaving: Vec<(usize, usize)> = bonds
        .iter()
        .filter_map(|&(a, b)| match (inside.contains(&a), inside.contains(&b)) {
            (true, false) => Some((a, b)),
            (false, true) => Some((b, a)),
            _ => None,
        })
        .collect();
    let (attach, outside) = match leaving.as_slice() {
        [one] => *one,
        [] => return Err("No bond leaves the selection: select a group that is bonded to the rest by one bond.".into()),
        _ => return Err(format!("{} bonds leave the selection: a group must be bonded to the rest by exactly one.", leaving.len())),
    };
    let atoms: Vec<usize> = selection.to_vec();
    let at = atoms.iter().position(|&a| a == attach).expect("the attachment atom is selected");
    let p = |i: usize| molecule.positions[i].as_dvec3();
    let order = bond_target(molecule.atomic_numbers[attach], molecule.atomic_numbers[outside], p(attach).distance(p(outside))).0;
    let order = if (order - 1.5).abs() < 1e-9 { 1.0 } else { order };
    Ok(Fragment {
        kind: FragmentKind::Group,
        atomic_numbers: atoms.iter().map(|&a| molecule.atomic_numbers[a]).collect(),
        positions: atoms.iter().map(|&a| (p(a) - p(attach)).as_vec3()).collect(),
        attach: Some(Attachment { atom: at, toward: (p(outside) - p(attach)).normalize().as_vec3(), order }),
    })
}

#[cfg(test)]
mod tests;
