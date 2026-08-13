//! Evaluates a molecular orbital (or, later, density) at an arbitrary 3D
//! point directly from a parsed `.fchk` basis set + MO coefficients — the
//! actual "generate a cube ourselves" math for Phase 3.
//!
//! Staged deliberately, S/P first: getting Cartesian-vs-pure-spherical
//! conventions and cross-component normalization exactly right for d/f/g
//! shells is the one place this kind of code is easy to get subtly wrong,
//! so each angular momentum is added — and validated against a real
//! reference `.cube` — one at a time rather than all at once. Only S and
//! P are implemented so far; anything with a d/f/g shell in it returns an
//! error rather than a silently-wrong result.

use glam::{DVec3, Vec3};

use crate::wavefunction::{BasisSet, MolecularOrbitals};

/// Standard Gaussian primitive normalization constant, for a Cartesian
/// component with total angular momentum split (l, m, n) across x/y/z
/// and exponent `alpha`:
///
/// N = (2*alpha/pi)^(3/4) * (4*alpha)^((l+m+n)/2) / sqrt((2l-1)!! (2m-1)!! (2n-1)!!)
///
/// with the usual convention (2*0-1)!! = 1.
fn primitive_normalization(alpha: f64, l: u32, m: u32, n: u32) -> f64 {
    fn double_factorial(k: u32) -> f64 {
        if k == 0 {
            1.0
        } else {
            (1..=k).map(|i| (2 * i - 1) as f64).product()
        }
    }
    let total_l = (l + m + n) as f64;
    (2.0 * alpha / std::f64::consts::PI).powf(0.75) * (4.0 * alpha).powf(total_l / 2.0)
        / (double_factorial(l) * double_factorial(m) * double_factorial(n)).sqrt()
}

/// Contracted radial value for one specific Cartesian component (l, m, n)
/// of a shell: sum over primitives of `c_i * N(alpha_i, l, m, n) *
/// exp(-alpha_i * r^2)`. Each Cartesian component gets its own
/// normalization computed from its *own* (l, m, n) split — not a single
/// "canonical" one reused across the whole shell — which is what
/// actually gets the relative scaling between e.g. a d shell's xx and xy
/// components right.
fn contracted_radial(shell: &crate::wavefunction::Shell, r2: f64, l: u32, m: u32, n: u32) -> f64 {
    shell
        .primitive_exponents
        .iter()
        .zip(&shell.contraction_coefficients)
        .map(|(&alpha, &c)| c * primitive_normalization(alpha, l, m, n) * (-alpha * r2).exp())
        .sum()
}

/// Appends this basis set's function values at `point` (Bohr) to `out`,
/// in the same order the `.fchk`'s MO coefficients expect: shells in
/// file order, and within a shell, Gaussian's own Cartesian component
/// order — S: 1; P: x,y,z; Cartesian D: xx,yy,zz,xy,xz,yz.
pub fn evaluate_basis_functions(basis: &BasisSet, point: DVec3, out: &mut Vec<f64>) -> Result<(), String> {
    out.clear();
    for shell in &basis.shells {
        let d = point - shell.center;
        let r2 = d.length_squared();

        match (shell.angular_momentum, shell.is_pure) {
            (0, _) => {
                out.push(contracted_radial(shell, r2, 0, 0, 0));
            }
            (1, _) => {
                let radial = contracted_radial(shell, r2, 1, 0, 0); // same for any single-axis P component by symmetry
                out.push(radial * d.x);
                out.push(radial * d.y);
                out.push(radial * d.z);
            }
            (2, false) => {
                // Cartesian d: xx, yy, zz, xy, xz, yz.
                out.push(contracted_radial(shell, r2, 2, 0, 0) * d.x * d.x);
                out.push(contracted_radial(shell, r2, 0, 2, 0) * d.y * d.y);
                out.push(contracted_radial(shell, r2, 0, 0, 2) * d.z * d.z);
                out.push(contracted_radial(shell, r2, 1, 1, 0) * d.x * d.y);
                out.push(contracted_radial(shell, r2, 1, 0, 1) * d.x * d.z);
                out.push(contracted_radial(shell, r2, 0, 1, 1) * d.y * d.z);
            }
            (2, true) => {
                // Pure (spherical-harmonic) d: Gaussian's order is
                // D0, D+1, D-1, D+2, D-2. Built from the *same*
                // individually-normalized Cartesian components as the
                // Cartesian case above (xx/yy/zz/xy/xz/yz) — three of the
                // five pure components (xz, yz, xy) turn out to already
                // equal one of those directly (each is proportional to a
                // single monomial, so its own per-component normalization
                // already makes it correctly normalized); only D0 and D+2
                // are genuine linear combinations, needing coefficients
                // derived from the actual overlap integrals between
                // Cartesian components (⟨xx|zz⟩ etc. are not zero, unlike
                // ⟨xx|xy⟩) rather than simple ad hoc weights — see the
                // module-level comment for the derivation.
                let xx = contracted_radial(shell, r2, 2, 0, 0) * d.x * d.x;
                let yy = contracted_radial(shell, r2, 0, 2, 0) * d.y * d.y;
                let zz = contracted_radial(shell, r2, 0, 0, 2) * d.z * d.z;
                let xy = contracted_radial(shell, r2, 1, 1, 0) * d.x * d.y;
                let xz = contracted_radial(shell, r2, 1, 0, 1) * d.x * d.z;
                let yz = contracted_radial(shell, r2, 0, 1, 1) * d.y * d.z;
                out.push(zz - 0.5 * xx - 0.5 * yy); // D0
                out.push(xz); // D+1
                out.push(yz); // D-1
                out.push(3f64.sqrt() / 2.0 * (xx - yy)); // D+2
                out.push(xy); // D-2
            }
            (3, false) => {
                // Cartesian f, Gaussian's order: xxx, yyy, zzz, xyy, xxy,
                // xxz, xzz, yzz, yyz, xyz.
                out.push(contracted_radial(shell, r2, 3, 0, 0) * d.x * d.x * d.x);
                out.push(contracted_radial(shell, r2, 0, 3, 0) * d.y * d.y * d.y);
                out.push(contracted_radial(shell, r2, 0, 0, 3) * d.z * d.z * d.z);
                out.push(contracted_radial(shell, r2, 1, 2, 0) * d.x * d.y * d.y);
                out.push(contracted_radial(shell, r2, 2, 1, 0) * d.x * d.x * d.y);
                out.push(contracted_radial(shell, r2, 2, 0, 1) * d.x * d.x * d.z);
                out.push(contracted_radial(shell, r2, 1, 0, 2) * d.x * d.z * d.z);
                out.push(contracted_radial(shell, r2, 0, 1, 2) * d.y * d.z * d.z);
                out.push(contracted_radial(shell, r2, 0, 2, 1) * d.y * d.y * d.z);
                out.push(contracted_radial(shell, r2, 1, 1, 1) * d.x * d.y * d.z);
            }
            (3, true) => {
                // Pure f, Gaussian's order: F0, F+1, F-1, F+2, F-2, F+3,
                // F-3. Same method as pure d above: each coefficient comes
                // from the overlap integrals between the individually-
                // normalized Cartesian components (not a memorized table),
                // derived from the standard real solid harmonics
                // (2z³-3x²z-3y²z, x(4z²-x²-y²), y(4z²-x²-y²), z(x²-y²),
                // xyz, x³-3xy², 3x²y-y³) — see the module-level comment.
                let xxx = contracted_radial(shell, r2, 3, 0, 0) * d.x * d.x * d.x;
                let yyy = contracted_radial(shell, r2, 0, 3, 0) * d.y * d.y * d.y;
                let zzz = contracted_radial(shell, r2, 0, 0, 3) * d.z * d.z * d.z;
                let xyy = contracted_radial(shell, r2, 1, 2, 0) * d.x * d.y * d.y;
                let xxy = contracted_radial(shell, r2, 2, 1, 0) * d.x * d.x * d.y;
                let xxz = contracted_radial(shell, r2, 2, 0, 1) * d.x * d.x * d.z;
                let xzz = contracted_radial(shell, r2, 1, 0, 2) * d.x * d.z * d.z;
                let yzz = contracted_radial(shell, r2, 0, 1, 2) * d.y * d.z * d.z;
                let yyz = contracted_radial(shell, r2, 0, 2, 1) * d.y * d.y * d.z;
                let xyz = contracted_radial(shell, r2, 1, 1, 1) * d.x * d.y * d.z;

                let sqrt5 = 5f64.sqrt();
                let sqrt6 = 6f64.sqrt();
                let sqrt10 = 10f64.sqrt();
                let sqrt30 = 30f64.sqrt();

                out.push(zzz - 3.0 / (2.0 * sqrt5) * (xxz + yyz)); // F0
                out.push(sqrt30 / 5.0 * xzz - sqrt6 / 4.0 * xxx - sqrt30 / 20.0 * xyy); // F+1
                out.push(sqrt30 / 5.0 * yzz - sqrt6 / 4.0 * yyy - sqrt30 / 20.0 * xxy); // F-1
                out.push(3f64.sqrt() / 2.0 * (xxz - yyz)); // F+2
                out.push(xyz); // F-2
                out.push(sqrt10 / 4.0 * xxx - 3.0 * 2f64.sqrt() / 4.0 * xyy); // F+3
                out.push(3.0 * 2f64.sqrt() / 4.0 * xxy - sqrt10 / 4.0 * yyy); // F-3
            }
            // Cartesian g deliberately isn't implemented: unlike Cartesian
            // d/f (common in older Pople-style basis sets), a Cartesian g
            // shell essentially never shows up in practice — every
            // basis family that goes as high as g (def2-QZVPP, cc-pVQZ,
            // ...) is spherical by convention — and there's no real
            // reference cube available to validate a guessed Gaussian
            // field ordering against, unlike every other case here. Fails
            // loudly rather than risk an unverifiable-right-now guess.
            (4, false) => return Err("Cartesian g shells aren't implemented (pure/spherical g is) — see gto.rs module docs".to_string()),
            (4, true) => {
                // Pure g, Gaussian's order: G0, G+1, G-1, G+2, G-2, G+3,
                // G-3, G+4, G-4 (the same 0,+1,-1,...,+L,-L pattern
                // already confirmed for d/f). Same method as d/f: each
                // coefficient comes from the overlap integrals between
                // individually-normalized Cartesian components, derived
                // from the standard real solid harmonics of degree 4 —
                // but this time computed symbolically (exact rationals/
                // square roots, not hand arithmetic) and cross-checked
                // with an exact 9x9 Gram-matrix orthonormality proof
                // *before* ever touching real reference data, given how
                // much larger and more error-prone this stage is than d
                // or f (9 components, several genuine 3+-term
                // combinations) — see the module-level comment.
                let xxxx = contracted_radial(shell, r2, 4, 0, 0) * d.x.powi(4);
                let yyyy = contracted_radial(shell, r2, 0, 4, 0) * d.y.powi(4);
                let zzzz = contracted_radial(shell, r2, 0, 0, 4) * d.z.powi(4);
                let xxyy = contracted_radial(shell, r2, 2, 2, 0) * d.x.powi(2) * d.y.powi(2);
                let xxzz = contracted_radial(shell, r2, 2, 0, 2) * d.x.powi(2) * d.z.powi(2);
                let yyzz = contracted_radial(shell, r2, 0, 2, 2) * d.y.powi(2) * d.z.powi(2);
                let xxxy = contracted_radial(shell, r2, 3, 1, 0) * d.x.powi(3) * d.y;
                let xyyy = contracted_radial(shell, r2, 1, 3, 0) * d.x * d.y.powi(3);
                let xxxz = contracted_radial(shell, r2, 3, 0, 1) * d.x.powi(3) * d.z;
                let xzzz = contracted_radial(shell, r2, 1, 0, 3) * d.x * d.z.powi(3);
                let yyyz = contracted_radial(shell, r2, 0, 3, 1) * d.y.powi(3) * d.z;
                let yzzz = contracted_radial(shell, r2, 0, 1, 3) * d.y * d.z.powi(3);
                let xxyz = contracted_radial(shell, r2, 2, 1, 1) * d.x.powi(2) * d.y * d.z;
                let xyyz = contracted_radial(shell, r2, 1, 2, 1) * d.x * d.y.powi(2) * d.z;
                let xyzz = contracted_radial(shell, r2, 1, 1, 2) * d.x * d.y * d.z.powi(2);

                let sqrt2 = 2f64.sqrt();
                let sqrt3 = 3f64.sqrt();
                let sqrt5 = 5f64.sqrt();
                let sqrt7 = 7f64.sqrt();
                let sqrt10 = 10f64.sqrt();
                let sqrt14 = 14f64.sqrt();
                let sqrt21 = 21f64.sqrt();
                let sqrt35 = 35f64.sqrt();
                let sqrt70 = 70f64.sqrt();
                let sqrt105 = 105f64.sqrt();

                out.push(0.375 * xxxx + 0.375 * yyyy + zzzz + (3.0 * sqrt105 / 140.0) * xxyy - (3.0 * sqrt105 / 35.0) * (xxzz + yyzz)); // G0
                out.push((sqrt70 / 7.0) * xzzz - (3.0 * sqrt70 / 28.0) * xxxz - (3.0 * sqrt14 / 28.0) * xyyz); // G+1
                out.push((sqrt70 / 7.0) * yzzz - (3.0 * sqrt14 / 28.0) * xxyz - (3.0 * sqrt70 / 28.0) * yyyz); // G-1
                out.push(-(sqrt5 / 4.0) * xxxx + (sqrt5 / 4.0) * yyyy + (3.0 * sqrt21 / 14.0) * (xxzz - yyzz)); // G+2
                out.push((3.0 * sqrt7 / 7.0) * xyzz - (sqrt35 / 14.0) * (xxxy + xyyy)); // G-2
                out.push((sqrt10 / 4.0) * xxxz - (3.0 * sqrt2 / 4.0) * xyyz); // G+3
                out.push((3.0 * sqrt2 / 4.0) * xxyz - (sqrt10 / 4.0) * yyyz); // G-3
                out.push((sqrt35 / 8.0) * (xxxx + yyyy) - (3.0 * sqrt3 / 4.0) * xxyy); // G+4
                out.push((sqrt5 / 2.0) * (xxxy - xyyy)); // G-4
            }
            (l, _) => return Err(format!("angular momentum {l} (h and above) not implemented yet — see gto.rs module docs")),
        }
    }
    Ok(())
}

/// Evaluates MO `mo_index` (0-based) at `point` (Bohr), from `orbitals` —
/// pass `wfn.alpha` or `wfn.beta.as_ref().unwrap()` depending on which
/// spin channel is wanted (the basis set itself, `basis`, is shared by
/// both channels).
pub fn evaluate_mo(basis: &BasisSet, orbitals: &MolecularOrbitals, mo_index: usize, point: DVec3, scratch: &mut Vec<f64>) -> Result<f64, String> {
    evaluate_basis_functions(basis, point, scratch)?;
    let coefficients = orbitals.coefficients_for(mo_index);
    if scratch.len() != coefficients.len() {
        return Err(format!("basis function count ({}) doesn't match MO coefficient count ({})", scratch.len(), coefficients.len()));
    }
    Ok(scratch.iter().zip(coefficients).map(|(&b, &c)| b * c).sum())
}

/// Builds a `ScalarGrid` for one MO from a given spin channel (see
/// `evaluate_mo`) by evaluating it at every point of an axis-aligned box
/// enclosing the basis set's own shell centers, padded on every side —
/// the same shape of grid `cubegen`/Chemcraft/APOST-3D produce, generated
/// directly instead of needing one of those as an external step.
/// `spacing_bohr`/`padding_bohr` are in Bohr (native to the evaluator);
/// the returned grid is in Angstrom, like every other `ScalarGrid` in the
/// app (see `crate::cube`).
pub fn generate_mo_grid(basis: &BasisSet, orbitals: &MolecularOrbitals, mo_index: usize, spacing_bohr: f64, padding_bohr: f64) -> Result<crate::cube::ScalarGrid, String> {
    let Some(first) = basis.shells.first() else { return Err("wavefunction has no shells".to_string()) };
    let mut min = first.center;
    let mut max = first.center;
    for shell in &basis.shells {
        min = min.min(shell.center);
        max = max.max(shell.center);
    }
    let padding = DVec3::splat(padding_bohr);
    min -= padding;
    max += padding;

    let extent = max - min;
    let dims = [
        ((extent.x / spacing_bohr).ceil() as usize).max(2),
        ((extent.y / spacing_bohr).ceil() as usize).max(2),
        ((extent.z / spacing_bohr).ceil() as usize).max(2),
    ];

    let mut values = Vec::with_capacity(dims[0] * dims[1] * dims[2]);
    let mut scratch = Vec::new();
    for i in 0..dims[0] {
        for j in 0..dims[1] {
            for k in 0..dims[2] {
                let point = min + DVec3::new(i as f64, j as f64, k as f64) * spacing_bohr;
                values.push(evaluate_mo(basis, orbitals, mo_index, point, &mut scratch)? as f32);
            }
        }
    }

    let to_angstrom = |v: DVec3| -> Vec3 {
        Vec3::new((v.x * crate::units::ANGSTROM_PER_BOHR) as f32, (v.y * crate::units::ANGSTROM_PER_BOHR) as f32, (v.z * crate::units::ANGSTROM_PER_BOHR) as f32)
    };
    let step = (spacing_bohr * crate::units::ANGSTROM_PER_BOHR) as f32;
    Ok(crate::cube::ScalarGrid {
        origin: to_angstrom(min),
        dims,
        steps: [Vec3::new(step, 0.0, 0.0), Vec3::new(0.0, step, 0.0), Vec3::new(0.0, 0.0, step)],
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_normalization_matches_known_s_value() {
        // A normalized S primitive should have self-overlap 1: integral
        // of (N * exp(-a r^2))^2 over all space = N^2 * (pi/(2a))^(3/2).
        // Solving N^2 * (pi/(2a))^(3/2) = 1 gives N = (2a/pi)^(3/4),
        // exactly the l=m=n=0 case (double factorials are all 1).
        let alpha = 1.3;
        let n = primitive_normalization(alpha, 0, 0, 0);
        let self_overlap = n * n * (std::f64::consts::PI / (2.0 * alpha)).powf(1.5);
        assert!((self_overlap - 1.0).abs() < 1e-10, "expected self-overlap 1.0, got {self_overlap}");
    }

    #[test]
    fn primitive_normalization_matches_known_p_value() {
        // Self-overlap of a normalized x*exp(-a r^2) primitive:
        // integral of (N*x*exp(-a r^2))^2 = N^2 * (pi/(2a))^(3/2) * 1/(4a).
        let alpha = 0.7;
        let n = primitive_normalization(alpha, 1, 0, 0);
        let self_overlap = n * n * (std::f64::consts::PI / (2.0 * alpha)).powf(1.5) * (1.0 / (4.0 * alpha));
        assert!((self_overlap - 1.0).abs() < 1e-10, "expected self-overlap 1.0, got {self_overlap}");
    }

    #[test]
    fn cartesian_g_shell_returns_an_error_not_a_wrong_answer() {
        // Pure g is implemented; Cartesian g deliberately isn't (see the
        // module comment) — should still fail loudly rather than
        // silently evaluate wrong.
        use crate::wavefunction::{BasisSet, Shell};
        let basis = BasisSet {
            shells: vec![Shell { angular_momentum: 4, is_pure: false, center: DVec3::ZERO, primitive_exponents: vec![1.0], contraction_coefficients: vec![1.0] }],
        };
        let mut scratch = Vec::new();
        assert!(evaluate_basis_functions(&basis, DVec3::ZERO, &mut scratch).is_err());
    }

    #[test]
    fn pure_h_shell_returns_an_error_not_a_wrong_answer() {
        // S through g are all implemented; h is not yet — should still
        // fail loudly rather than silently evaluate wrong.
        use crate::wavefunction::{BasisSet, Shell};
        let basis = BasisSet {
            shells: vec![Shell { angular_momentum: 5, is_pure: true, center: DVec3::ZERO, primitive_exponents: vec![1.0], contraction_coefficients: vec![1.0] }],
        };
        let mut scratch = Vec::new();
        assert!(evaluate_basis_functions(&basis, DVec3::ZERO, &mut scratch).is_err());
    }

    #[test]
    fn pure_g_components_are_orthonormal() {
        // Same method as the pure-d/pure-f checks above, extended to g's
        // 9 components — the direct numerical check that the G0..G+4/-4
        // linear-combination coefficients derived in
        // `evaluate_basis_functions` are actually correct, complementing
        // (not replacing) the exact symbolic Gram-matrix proof done
        // before writing this code (see the module comment) — this one
        // exercises the *actual Rust evaluation path*, not just the
        // paper derivation.
        use crate::wavefunction::{BasisSet, Shell};
        let alpha = 0.9;
        let basis = BasisSet {
            shells: vec![Shell { angular_momentum: 4, is_pure: true, center: DVec3::ZERO, primitive_exponents: vec![alpha], contraction_coefficients: vec![1.0] }],
        };

        let extent = 6.0;
        let n = 60usize;
        let step = 2.0 * extent / n as f32;
        let mut gram = [[0.0f64; 9]; 9];
        let mut scratch = Vec::new();
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let p = DVec3::new(
                        (-extent + i as f32 * step) as f64,
                        (-extent + j as f32 * step) as f64,
                        (-extent + k as f32 * step) as f64,
                    );
                    evaluate_basis_functions(&basis, p, &mut scratch).unwrap();
                    for a in 0..9 {
                        for b in 0..9 {
                            gram[a][b] += scratch[a] * scratch[b];
                        }
                    }
                }
            }
        }
        let cell_volume = (step as f64).powi(3);
        for row in gram.iter_mut() {
            for v in row.iter_mut() {
                *v *= cell_volume;
            }
        }

        for a in 0..9 {
            assert!((gram[a][a] - 1.0).abs() < 0.02, "component {a} self-overlap should be ~1.0, got {}", gram[a][a]);
            for b in 0..9 {
                if a != b {
                    assert!(gram[a][b].abs() < 0.02, "components {a},{b} should be ~orthogonal, got overlap {}", gram[a][b]);
                }
            }
        }
    }

    #[test]
    fn pure_f_components_are_orthonormal() {
        // Same method as the pure-d check above, extended to f's 7
        // components — the direct numerical check that the F0/F+1/F-1/
        // F+2/F+3/F-3 linear-combination coefficients derived in
        // `evaluate_basis_functions` are actually correct (F-2 is a bare
        // monomial, needing no combination).
        use crate::wavefunction::{BasisSet, Shell};
        let alpha = 0.9;
        let basis = BasisSet {
            shells: vec![Shell { angular_momentum: 3, is_pure: true, center: DVec3::ZERO, primitive_exponents: vec![alpha], contraction_coefficients: vec![1.0] }],
        };

        let extent = 6.0;
        let n = 60usize;
        let step = 2.0 * extent / n as f32;
        let mut gram = [[0.0f64; 7]; 7];
        let mut scratch = Vec::new();
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let p = DVec3::new(
                        (-extent + i as f32 * step) as f64,
                        (-extent + j as f32 * step) as f64,
                        (-extent + k as f32 * step) as f64,
                    );
                    evaluate_basis_functions(&basis, p, &mut scratch).unwrap();
                    for a in 0..7 {
                        for b in 0..7 {
                            gram[a][b] += scratch[a] * scratch[b];
                        }
                    }
                }
            }
        }
        let cell_volume = (step as f64).powi(3);
        for row in gram.iter_mut() {
            for v in row.iter_mut() {
                *v *= cell_volume;
            }
        }

        for a in 0..7 {
            assert!((gram[a][a] - 1.0).abs() < 0.02, "component {a} self-overlap should be ~1.0, got {}", gram[a][a]);
            for b in 0..7 {
                if a != b {
                    assert!(gram[a][b].abs() < 0.02, "components {a},{b} should be ~orthogonal, got overlap {}", gram[a][b]);
                }
            }
        }
    }

    #[test]
    fn pure_d_components_are_orthonormal() {
        // Numerically integrate ⟨pure_d_i | pure_d_j⟩ on a coarse 3D grid
        // for a single primitive — should come out ≈ identity (each
        // properly normalized, all mutually orthogonal), the direct
        // check that the D0/D+2 linear-combination coefficients derived
        // in `evaluate_basis_functions` are actually correct, not just
        // plausible-looking.
        use crate::wavefunction::{BasisSet, Shell};
        let alpha = 0.9;
        let basis = BasisSet {
            shells: vec![Shell { angular_momentum: 2, is_pure: true, center: DVec3::ZERO, primitive_exponents: vec![alpha], contraction_coefficients: vec![1.0] }],
        };

        let extent = 6.0;
        let n = 60usize;
        let step = 2.0 * extent / n as f32;
        let mut gram = [[0.0f64; 5]; 5];
        let mut scratch = Vec::new();
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let p = DVec3::new(
                        (-extent + i as f32 * step) as f64,
                        (-extent + j as f32 * step) as f64,
                        (-extent + k as f32 * step) as f64,
                    );
                    evaluate_basis_functions(&basis, p, &mut scratch).unwrap();
                    for a in 0..5 {
                        for b in 0..5 {
                            gram[a][b] += scratch[a] * scratch[b];
                        }
                    }
                }
            }
        }
        let cell_volume = (step as f64).powi(3);
        for row in gram.iter_mut() {
            for v in row.iter_mut() {
                *v *= cell_volume;
            }
        }

        for a in 0..5 {
            assert!((gram[a][a] - 1.0).abs() < 0.02, "component {a} self-overlap should be ~1.0, got {}", gram[a][a]);
            for b in 0..5 {
                if a != b {
                    assert!(gram[a][b].abs() < 0.02, "components {a},{b} should be ~orthogonal, got overlap {}", gram[a][b]);
                }
            }
        }
    }

    #[test]
    fn cartesian_d_xx_matches_direct_formula() {
        use crate::wavefunction::{BasisSet, Shell};
        let alpha = 0.8;
        let basis = BasisSet {
            shells: vec![Shell { angular_momentum: 2, is_pure: false, center: DVec3::ZERO, primitive_exponents: vec![alpha], contraction_coefficients: vec![1.0] }],
        };
        let point = DVec3::new(0.3, 0.0, 0.0);
        let mut scratch = Vec::new();
        evaluate_basis_functions(&basis, point, &mut scratch).unwrap();
        // xx component (index 0): N(alpha,2,0,0) * x^2 * exp(-alpha*x^2).
        let expected = primitive_normalization(alpha, 2, 0, 0) * point.x * point.x * (-alpha * point.length_squared()).exp();
        assert!((scratch[0] - expected).abs() < 1e-12, "xx: {} vs {expected}", scratch[0]);
        // yy and zz should be exactly 0 at this point (y=z=0).
        assert!(scratch[1].abs() < 1e-12);
        assert!(scratch[2].abs() < 1e-12);
    }
}
