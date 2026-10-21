//! Automatic fragments for an APOST-3D input, from the bond graph: the
//! bonds the viewer draws (those made with "Create bond" included, hidden
//! ones left out), so a wrong result is corrected by fixing the bonds.
//!
//! Around chosen centers (a metal, or several): each center is a fragment
//! of its own, and the rest of the molecule falls apart into ligands, the
//! connected pieces of the bond graph once the centers are taken out.
//! Multi-hapto ligands (Cp, benzene, an η²-alkene) stay whole, since their
//! atoms are bonded to each other; an atom bridging two centers (μ-Cl,
//! μ-O, μ-H) comes out as a ligand of its own; counter-ions and solvent
//! molecules come out as their own pieces. Atoms are 0-based here.

/// The connected pieces of the graph of `n_atoms` atoms and `bonds`,
/// leaving out the atoms in `removed`; each piece sorted, pieces ordered
/// by their first atom.
fn pieces(n_atoms: usize, bonds: &[(usize, usize)], removed: &[usize]) -> Vec<Vec<usize>> {
    let mut neighbors = vec![Vec::new(); n_atoms];
    for &(a, b) in bonds {
        if a < n_atoms && b < n_atoms && !removed.contains(&a) && !removed.contains(&b) {
            neighbors[a].push(b);
            neighbors[b].push(a);
        }
    }
    let mut seen = vec![false; n_atoms];
    let mut pieces = Vec::new();
    for start in 0..n_atoms {
        if seen[start] || removed.contains(&start) {
            continue;
        }
        seen[start] = true;
        let mut piece = vec![start];
        let mut next = 0;
        while next < piece.len() {
            for &neighbor in &neighbors[piece[next]] {
                if !seen[neighbor] {
                    seen[neighbor] = true;
                    piece.push(neighbor);
                }
            }
            next += 1;
        }
        piece.sort_unstable();
        pieces.push(piece);
    }
    pieces
}

/// Fragments around `centers`: one per center, in atom order, then one per
/// connected piece of what remains (the ligands), ordered by their first
/// atom.
pub fn fragments_around(n_atoms: usize, bonds: &[(usize, usize)], centers: &[usize]) -> Vec<Vec<usize>> {
    let mut centers: Vec<usize> = centers.iter().copied().filter(|&c| c < n_atoms).collect();
    centers.sort_unstable();
    centers.dedup();
    let mut fragments: Vec<Vec<usize>> = centers.iter().map(|&c| vec![c]).collect();
    fragments.extend(pieces(n_atoms, bonds, &centers));
    fragments
}

/// One fragment per molecule: the connected pieces of the whole bond graph
/// (the two waters of a dimer, a complex and its counter-ion).
pub fn separate_molecules(n_atoms: usize, bonds: &[(usize, usize)]) -> Vec<Vec<usize>> {
    pieces(n_atoms, bonds, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_bound_to_a_metal_stays_one_ligand() {
        // Fe (0) bound to all five carbons of a Cp ring (1-5), and a CO (6, 7).
        let mut bonds: Vec<(usize, usize)> = (1..=5).map(|c| (0, c)).collect();
        bonds.extend([(1, 2), (2, 3), (3, 4), (4, 5), (5, 1), (0, 6), (6, 7)]);
        assert_eq!(fragments_around(8, &bonds, &[0]), vec![vec![0], vec![1, 2, 3, 4, 5], vec![6, 7]]);
    }

    #[test]
    fn a_bridging_atom_between_two_centers_is_its_own_ligand() {
        // Two metals (0, 3) bridged by a chloride (1), each with a terminal
        // chloride (2, 4), and a counter-ion (5) bound to nothing.
        let bonds = [(0, 1), (1, 3), (0, 2), (3, 4), (0, 3)];
        assert_eq!(fragments_around(6, &bonds, &[3, 0]), vec![vec![0], vec![3], vec![1], vec![2], vec![4], vec![5]]);
    }

    #[test]
    fn molecules_are_the_connected_pieces() {
        let bonds = [(0, 1), (0, 2), (3, 4), (3, 5)];
        assert_eq!(separate_molecules(6, &bonds), vec![vec![0, 1, 2], vec![3, 4, 5]]);
        assert_eq!(fragments_around(3, &[(0, 1), (0, 2)], &[]), vec![vec![0, 1, 2]]);
    }
}
