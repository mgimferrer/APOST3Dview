//! The atomic weight functions of APOST-3D's real-space atoms (BECKE,
//! BECKE-RHO, TFVC), rebuilt from the results file's `atomic_weights` block
//! (schema 0.16): w_A(r) for every atom, and so the weight of a fragment,
//! w_F = sum of w_A over its atoms, anywhere in space.
//!
//! The EFOs APOST-3D writes to its EOS/GEOS orbital `.fchk` files are
//! basis-set fits of w_F times the EFO; the fit leaves small tails on the
//! other fragments, which multiplying by w_F once more removes.
//!
//! w_A(r) = P_A(r) / sum_B P_B(r), P_A(r) = prod_{B != A} s(nu_AB), with
//! s(nu) = (1 - f^k(nu)) / 2, f(x) = x (3 - x^2) / 2 applied k times
//! (`stiffness`), mu_AB = (|r - R_A| - |r - R_B|) / |R_A - R_B| and, with
//! chi = chi_AB (`radius_ratios[A][B]`):
//! - becke: nu = mu + a (1 - mu^2), a = (1 - chi^2) / (4 chi) limited to
//!   [-1/2, 1/2];
//! - tfvc: nu = (1 + mu - chi (1 - mu)) / (1 + mu + chi (1 - mu)).
//!
//! Source of truth: APOST3D `docs/source/developer/json-output.md`,
//! section `atomic_weights`.

use glam::DVec3;
use serde::Deserialize;

/// The `atomic_weights` block as written.
#[derive(Debug, Clone, Deserialize)]
pub struct AtomicWeights {
    /// "becke" (BECKE, BECKE-RHO) or "tfvc" (TFVC).
    pub profile: String,
    /// k, the number of iterations of the cell function.
    pub stiffness: u32,
    /// chi_AB, row A, column B; the diagonal is not used.
    pub radius_ratios: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    Becke,
    Tfvc,
}

/// A cell function s(nu_AB) below this fraction of the nearest atom's
/// P stands for zero: P_B is at most its smallest factor, so leaving such
/// atoms out changes no weight by more than (number of atoms) x this.
const NEGLIGIBLE: f64 = 1e-15;

/// The weight functions of a molecule's atoms, ready to evaluate.
#[derive(Debug, Clone)]
pub struct AtomCells {
    profile: Profile,
    stiffness: u32,
    /// Nuclear positions, bohr.
    centers: Vec<DVec3>,
    /// 1 / |R_A - R_B|, row-major n x n.
    inverse_distances: Vec<f64>,
    /// Per pair: the Becke shift a (limited), or chi for TFVC.
    parameters: Vec<f64>,
}

/// Scratch space for `AtomCells::atom_weights`, one per thread.
#[derive(Debug, Clone, Default)]
pub struct CellScratch {
    distances: Vec<f64>,
    cells: Vec<f64>,
}

impl AtomCells {
    /// Checks the block against the molecule (`coordinates_bohr`): one row
    /// and column per atom, positive finite ratios, a known profile.
    pub fn new(weights: &AtomicWeights, coordinates_bohr: &[[f64; 3]]) -> Result<Self, String> {
        let profile = match weights.profile.as_str() {
            "becke" => Profile::Becke,
            "tfvc" => Profile::Tfvc,
            other => return Err(format!("unknown atomic weight profile \"{other}\"")),
        };
        if weights.stiffness > 32 {
            return Err(format!("stiffness {} is out of range", weights.stiffness));
        }
        let n = coordinates_bohr.len();
        if weights.radius_ratios.len() != n || weights.radius_ratios.iter().any(|row| row.len() != n) {
            return Err(format!("the radius ratios are not a {n} x {n} matrix"));
        }
        let centers: Vec<DVec3> = coordinates_bohr.iter().map(|c| DVec3::from_array(*c)).collect();
        let mut inverse_distances = vec![0.0; n * n];
        let mut parameters = vec![0.0; n * n];
        for a in 0..n {
            for b in 0..n {
                if a == b {
                    continue;
                }
                let distance = centers[a].distance(centers[b]);
                if distance < 1e-8 {
                    return Err(format!("atoms {} and {} are at the same place", a + 1, b + 1));
                }
                let chi = weights.radius_ratios[a][b];
                if !(chi.is_finite() && chi > 0.0) {
                    return Err(format!("radius ratio of atoms {} and {} is {chi}", a + 1, b + 1));
                }
                inverse_distances[a * n + b] = 1.0 / distance;
                parameters[a * n + b] = match profile {
                    Profile::Becke => ((1.0 - chi * chi) / (4.0 * chi)).clamp(-0.5, 0.5),
                    Profile::Tfvc => chi,
                };
            }
        }
        Ok(Self { profile, stiffness: weights.stiffness, centers, inverse_distances, parameters })
    }

    pub fn n_atoms(&self) -> usize {
        self.centers.len()
    }

    /// s(nu_AB) at a point at distances `da` from A and `db` from B.
    #[inline]
    fn cell(&self, a: usize, b: usize, da: f64, db: f64) -> f64 {
        let pair = a * self.centers.len() + b;
        let mu = (da - db) * self.inverse_distances[pair];
        let p = self.parameters[pair];
        let mut nu = match self.profile {
            Profile::Becke => mu + p * (1.0 - mu * mu),
            Profile::Tfvc => {
                let (plus, minus) = (1.0 + mu, p * (1.0 - mu));
                (plus - minus) / (plus + minus)
            }
        };
        for _ in 0..self.stiffness {
            nu *= 1.5 - 0.5 * nu * nu;
        }
        0.5 * (1.0 - nu)
    }

    /// P_A: the product of A's cell functions with every other atom,
    /// given up (as 0) once it falls to `floor`.
    fn cell_product(&self, a: usize, distances: &[f64], floor: f64) -> f64 {
        let mut product = 1.0;
        for b in 0..self.centers.len() {
            if b != a {
                product *= self.cell(a, b, distances[a], distances[b]);
                if product <= floor {
                    return 0.0;
                }
            }
        }
        product
    }

    /// w_A at `point` (bohr) for every atom A, into `weights`.
    ///
    /// The nearest atom's P is computed first; another atom whose P can't
    /// reach `NEGLIGIBLE` times it is left out, which on a large molecule
    /// skips most of the n^2 cell functions at any point.
    pub fn atom_weights(&self, point: DVec3, scratch: &mut CellScratch, weights: &mut [f64]) {
        let n = self.centers.len();
        scratch.distances.clear();
        scratch.distances.extend(self.centers.iter().map(|c| c.distance(point)));
        scratch.cells.clear();
        scratch.cells.resize(n, 0.0);
        let distances = &scratch.distances;
        let nearest = (0..n).min_by(|&a, &b| distances[a].total_cmp(&distances[b])).unwrap_or(0);
        let own = self.cell_product(nearest, distances, 0.0);
        let floor = own * NEGLIGIBLE;
        scratch.cells[nearest] = own;
        for a in (0..n).filter(|&a| a != nearest) {
            // P_A is at most its factor with the nearest atom.
            if self.cell(a, nearest, distances[a], distances[nearest]) > floor {
                scratch.cells[a] = self.cell_product(a, distances, floor);
            }
        }
        let mut total: f64 = scratch.cells.iter().sum();
        if total <= 0.0 {
            // Every P vanished (possible only with extreme ratios): all of
            // them in full, and the nearest atom if that is still nothing.
            for a in 0..n {
                scratch.cells[a] = self.cell_product(a, distances, 0.0);
            }
            total = scratch.cells.iter().sum();
            if total <= 0.0 {
                scratch.cells.fill(0.0);
                scratch.cells[nearest] = 1.0;
                total = 1.0;
            }
        }
        for (w, p) in weights.iter_mut().zip(&scratch.cells) {
            *w = p / total;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(profile: &str, chi: Vec<Vec<f64>>) -> AtomicWeights {
        AtomicWeights { profile: profile.to_string(), stiffness: 4, radius_ratios: chi }
    }

    fn evaluate(cells: &AtomCells, point: DVec3) -> Vec<f64> {
        let mut out = vec![0.0; cells.n_atoms()];
        cells.atom_weights(point, &mut CellScratch::default(), &mut out);
        out
    }

    #[test]
    fn weights_sum_to_one_and_are_one_at_a_nucleus() {
        let coordinates = [[0.0, 0.0, 0.0], [2.1, 0.0, 0.0], [-0.7, 1.9, 0.3], [0.4, -1.2, 1.8]];
        let chi = vec![vec![1.0, 0.6, 1.3, 0.9], vec![1.0 / 0.6, 1.0, 1.1, 1.0], vec![1.0 / 1.3, 1.0 / 1.1, 1.0, 0.8], vec![1.0 / 0.9, 1.0, 1.25, 1.0]];
        for profile in ["becke", "tfvc"] {
            let cells = AtomCells::new(&weights(profile, chi.clone()), &coordinates).unwrap();
            for point in [DVec3::new(0.3, 0.2, -0.1), DVec3::new(1.0, 1.0, 1.0), DVec3::new(-5.0, 3.0, 8.0)] {
                let w = evaluate(&cells, point);
                assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-14, "{profile}: {w:?}");
                assert!(w.iter().all(|&x| (0.0..=1.0).contains(&x)));
            }
            for (a, c) in coordinates.iter().enumerate() {
                let w = evaluate(&cells, DVec3::from_array(*c));
                assert!((w[a] - 1.0).abs() < 1e-12, "{profile}, atom {a}: {w:?}");
            }
        }
    }

    /// With equal radii both profiles reduce to Becke's original cells:
    /// nu = mu, and the midpoint of two atoms is shared half and half.
    #[test]
    fn equal_radii_give_the_same_cells_in_both_profiles() {
        let coordinates = [[0.0, 0.0, 0.0], [1.4, 0.0, 0.0], [0.5, 1.7, 0.0]];
        let ones = vec![vec![1.0; 3]; 3];
        let becke = AtomCells::new(&weights("becke", ones.clone()), &coordinates).unwrap();
        let tfvc = AtomCells::new(&weights("tfvc", ones), &coordinates).unwrap();
        for point in [DVec3::new(0.7, 0.0, 0.0), DVec3::new(0.2, 0.9, -0.4), DVec3::new(3.0, -1.0, 2.0)] {
            let (a, b) = (evaluate(&becke, point), evaluate(&tfvc, point));
            for (x, y) in a.iter().zip(&b) {
                assert!((x - y).abs() < 1e-15);
            }
        }
        let two = AtomCells::new(&weights("tfvc", vec![vec![1.0; 2]; 2]), &coordinates[..2]).unwrap();
        let w = evaluate(&two, DVec3::new(0.7, 0.4, -0.3));
        assert!((w[0] - 0.5).abs() < 1e-15 && (w[1] - 0.5).abs() < 1e-15, "{w:?}");
    }

    #[test]
    fn a_block_that_does_not_fit_the_molecule_is_refused() {
        let coordinates = [[0.0, 0.0, 0.0], [1.4, 0.0, 0.0]];
        assert!(AtomCells::new(&weights("tfvc", vec![vec![1.0; 3]; 3]), &coordinates).is_err());
        assert!(AtomCells::new(&weights("hirshfeld", vec![vec![1.0; 2]; 2]), &coordinates).is_err());
        assert!(AtomCells::new(&weights("becke", vec![vec![1.0, 0.0], vec![1.0, 1.0]]), &coordinates).is_err());
    }
}
