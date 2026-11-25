//! Which atoms move when a distance, angle or dihedral is set: the "side"
//! of an atom is everything still connected to it once the bond to its
//! neighbor in the measurement is cut. Sides follow the bonds given (the
//! app passes the bonds drawn), so hiding a bond cuts there and creating
//! one joins.

/// The bonds of a molecule as neighbor lists.
pub struct Graph {
    neighbors: Vec<Vec<usize>>,
}

impl Graph {
    /// `n` atoms joined by `bonds` (0-based pairs; out-of-range ones are
    /// left out).
    pub fn new(n: usize, bonds: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut neighbors = vec![Vec::new(); n];
        for (a, b) in bonds {
            if a < n && b < n && a != b {
                neighbors[a].push(b);
                neighbors[b].push(a);
            }
        }
        for list in &mut neighbors {
            list.sort_unstable();
            list.dedup();
        }
        Self { neighbors }
    }

    /// The atoms bonded to `atom`, directly or through others (its piece:
    /// a ligand, a solvent molecule), with `atom`, sorted.
    pub fn piece(&self, atom: usize) -> Vec<usize> {
        let n = self.neighbors.len();
        if atom >= n {
            return Vec::new();
        }
        let mut seen = vec![false; n];
        seen[atom] = true;
        let mut stack = vec![atom];
        while let Some(u) = stack.pop() {
            for &v in &self.neighbors[u] {
                if !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        (0..n).filter(|&i| seen[i]).collect()
    }

    /// The atoms on `from`'s side of `other`: `from` and everything
    /// reachable from it without passing through `other`, sorted. `None`
    /// when `other` is reached anyway (the two are in one ring, or joined
    /// by another path), since then no side can move without the other.
    /// A direct bond between the two is the one cut; without one, `from`'s
    /// whole molecule is its side unless it contains `other`.
    pub fn side(&self, other: usize, from: usize) -> Option<Vec<usize>> {
        let n = self.neighbors.len();
        if from >= n || other >= n || from == other {
            return None;
        }
        let mut seen = vec![false; n];
        seen[from] = true;
        let mut stack = vec![from];
        while let Some(u) = stack.pop() {
            for &v in &self.neighbors[u] {
                if v == other {
                    if u == from {
                        continue;
                    }
                    return None;
                }
                if !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        Some((0..n).filter(|&i| seen[i]).collect())
    }
}

/// Which end of a measurement moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The last atom picked and its side.
    Last,
    /// The first atom picked and its side (the other half of "both sides").
    First,
}

/// The atoms that move when the measurement on `atoms` (2 to 4, in the
/// order picked) is changed by moving one end with its side, or `None`
/// when that side can't be separated or would carry the other end along
/// (which would leave the value unchanged).
///
/// The bond cut is the one at that end: distance a-b and angle a-b-c the
/// last bond (or first, for `Which::First`), dihedral a-b-c-d the middle
/// bond b-c, so the whole group behind c (or b) turns.
pub fn moving_atoms(graph: &Graph, atoms: &[usize], which: Which) -> Option<Vec<usize>> {
    let (keep, start, must_hold, must_not_hold): (usize, usize, &[usize], &[usize]) = match (atoms, which) {
        ([a, b], Which::Last) => (*a, *b, &[], &[]),
        ([a, b], Which::First) => (*b, *a, &[], &[]),
        ([a, b, c], Which::Last) => (*b, *c, &[], std::slice::from_ref(a)),
        ([a, b, c], Which::First) => (*b, *a, &[], std::slice::from_ref(c)),
        ([a, b, c, d], Which::Last) => (*b, *c, std::slice::from_ref(d), std::slice::from_ref(a)),
        ([a, b, c, d], Which::First) => (*c, *b, std::slice::from_ref(a), std::slice::from_ref(d)),
        _ => return None,
    };
    let side = graph.side(keep, start)?;
    let holds = |atom: &usize| side.binary_search(atom).is_ok();
    (must_hold.iter().all(holds) && !must_not_hold.iter().any(holds)).then_some(side)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Butane-like chain 0-1-2-3 with hydrogens 4 (on 0) and 5 (on 3).
    fn chain() -> Graph {
        Graph::new(6, [(0, 1), (1, 2), (2, 3), (0, 4), (3, 5)])
    }

    #[test]
    fn a_side_is_what_stays_connected_past_the_cut() {
        let g = chain();
        assert_eq!(g.side(1, 2), Some(vec![2, 3, 5]));
        assert_eq!(g.side(2, 1), Some(vec![0, 1, 4]));
        // Not bonded: the whole molecule, unless it holds the other atom.
        assert_eq!(g.side(0, 3), None);
    }

    #[test]
    fn a_piece_is_everything_bonded_together() {
        let g = Graph::new(6, [(0, 1), (0, 2), (3, 4), (3, 5)]);
        assert_eq!(g.piece(4), vec![3, 4, 5]);
        assert_eq!(g.piece(0), vec![0, 1, 2]);
        assert_eq!(g.piece(9), Vec::<usize>::new());
    }

    #[test]
    fn rings_have_no_sides() {
        // Cyclobutane 0-1-2-3-0 with a methyl 4 on atom 2.
        let g = Graph::new(5, [(0, 1), (1, 2), (2, 3), (3, 0), (2, 4)]);
        assert_eq!(g.side(1, 2), None);
        // A bond out of the ring has sides.
        assert_eq!(g.side(2, 4), Some(vec![4]));
        assert_eq!(g.side(4, 2), Some(vec![0, 1, 2, 3]));
    }

    #[test]
    fn separate_molecules_move_whole() {
        // Two waters: 0(O)-1,2 and 3(O)-4,5.
        let g = Graph::new(6, [(0, 1), (0, 2), (3, 4), (3, 5)]);
        assert_eq!(moving_atoms(&g, &[0, 3], Which::Last), Some(vec![3, 4, 5]));
        assert_eq!(moving_atoms(&g, &[0, 3], Which::First), Some(vec![0, 1, 2]));
    }

    #[test]
    fn a_dihedral_turns_the_group_behind_the_middle_bond() {
        let g = chain();
        assert_eq!(moving_atoms(&g, &[0, 1, 2, 3], Which::Last), Some(vec![2, 3, 5]));
        assert_eq!(moving_atoms(&g, &[0, 1, 2, 3], Which::First), Some(vec![0, 1, 4]));
        // The angle 0-1-2: the last bond 1-2 is cut.
        assert_eq!(moving_atoms(&g, &[0, 1, 2], Which::Last), Some(vec![2, 3, 5]));
        assert_eq!(moving_atoms(&g, &[0, 1, 2], Which::First), Some(vec![0, 4]));
    }

    #[test]
    fn a_side_carrying_the_other_end_is_refused() {
        // Angle 4-0-1 where 4 is not bonded to 0 but to 1 (so 1's side,
        // cut at 0-1, holds 4 and the angle could not change).
        let g = Graph::new(5, [(0, 1), (1, 4), (1, 2)]);
        assert_eq!(moving_atoms(&g, &[4, 0, 1], Which::Last), None);
    }
}
