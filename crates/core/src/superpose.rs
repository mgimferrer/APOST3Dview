//! Rigid superposition of matched atoms: the rotation and translation that
//! best put one set of positions onto another (least squares), as used to
//! show related molecules from separate calculations in the same
//! orientation.
//!
//! Horn's closed-form quaternion solution (J. Opt. Soc. Am. A 4, 629
//! (1987)): the best rotation is the eigenvector of largest eigenvalue of
//! a symmetric 4 x 4 matrix built from the cross-covariance of the two
//! centered sets. It is always a proper rotation (never a reflection).

use faer::{Mat, Side};
use glam::{DQuat, DVec3, Quat, Vec3};

/// `reference[i] ≈ rotation * moving[i] + translation`, with the root mean
/// square distance left between the matched atoms after the fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Superposition {
    pub rotation: Quat,
    pub translation: Vec3,
    /// In the units of the positions.
    pub rmsd: f64,
}

/// The best rigid superposition of `moving` onto `reference`, atom `i` on
/// atom `i`. `None` for fewer than three pairs or sets of different
/// length. Three atoms in a line leave the rotation about that line free:
/// any of the equally good ones is returned.
pub fn superpose(reference: &[Vec3], moving: &[Vec3]) -> Option<Superposition> {
    if reference.len() != moving.len() || reference.len() < 3 {
        return None;
    }
    let n = reference.len() as f64;
    let r: Vec<DVec3> = reference.iter().map(|p| p.as_dvec3()).collect();
    let m: Vec<DVec3> = moving.iter().map(|p| p.as_dvec3()).collect();
    let r_center = r.iter().sum::<DVec3>() / n;
    let m_center = m.iter().sum::<DVec3>() / n;

    // Cross-covariance s[a][b] = sum over atoms of moving_a * reference_b.
    let mut s = [[0.0f64; 3]; 3];
    for (ri, mi) in r.iter().zip(&m) {
        let (a, b) = ((*mi - m_center).to_array(), (*ri - r_center).to_array());
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += a[i] * b[j];
            }
        }
    }
    let [[sxx, sxy, sxz], [syx, syy, syz], [szx, szy, szz]] = s;
    let k = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let matrix = Mat::from_fn(4, 4, |i, j| k[i][j]);
    let eigen = matrix.self_adjoint_eigen(Side::Lower).ok()?;
    // Eigenvalues come smallest first: the last column is the rotation,
    // as (w, x, y, z).
    let u = eigen.U();
    let q = DQuat::from_xyzw(u[(1, 3)], u[(2, 3)], u[(3, 3)], u[(0, 3)]).normalize();
    let translation = r_center - q * m_center;
    let squared: f64 = r.iter().zip(&m).map(|(ri, mi)| (q * *mi + translation - *ri).length_squared()).sum();
    Some(Superposition { rotation: q.as_quat(), translation: translation.as_vec3(), rmsd: (squared / n).sqrt() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atoms() -> Vec<Vec3> {
        vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.5, 0.0, 0.0), Vec3::new(-0.4, 1.1, 0.2), Vec3::new(0.3, -0.6, 1.3), Vec3::new(2.2, 0.9, -0.7)]
    }

    #[test]
    fn a_rigid_motion_is_found_again() {
        let rotation = Quat::from_axis_angle(Vec3::new(0.3, -1.0, 0.5).normalize(), 2.1);
        let translation = Vec3::new(4.0, -2.5, 7.0);
        let reference = atoms();
        // `moving` is the reference moved away by the inverse motion.
        let moving: Vec<Vec3> = reference.iter().map(|p| rotation.inverse() * (*p - translation)).collect();
        let fit = superpose(&reference, &moving).unwrap();
        assert!(fit.rmsd < 1e-5, "{}", fit.rmsd);
        assert!(fit.rotation.dot(rotation).abs() > 1.0 - 1e-6, "{:?} vs {rotation:?}", fit.rotation);
        assert!((fit.translation - translation).length() < 1e-4);
        for (r, m) in reference.iter().zip(&moving) {
            assert!((fit.rotation * *m + fit.translation - *r).length() < 1e-4);
        }
    }

    #[test]
    fn a_mirror_image_is_not_turned_into_a_reflection() {
        let reference = atoms();
        let mirrored: Vec<Vec3> = reference.iter().map(|p| Vec3::new(-p.x, p.y, p.z)).collect();
        let fit = superpose(&reference, &mirrored).unwrap();
        assert!(fit.rotation.is_normalized());
        assert!(fit.rmsd > 0.1, "a reflection can't be undone by a rotation: {}", fit.rmsd);
    }

    #[test]
    fn noise_leaves_a_small_rmsd() {
        let reference = atoms();
        let moving: Vec<Vec3> = reference.iter().enumerate().map(|(i, p)| *p + Vec3::splat(if i % 2 == 0 { 0.01 } else { -0.01 })).collect();
        let fit = superpose(&reference, &moving).unwrap();
        assert!(fit.rmsd < 0.02 && fit.rmsd > 0.0, "{}", fit.rmsd);
    }

    #[test]
    fn too_few_or_unmatched_atoms() {
        assert!(superpose(&atoms()[..2], &atoms()[..2]).is_none());
        assert!(superpose(&atoms(), &atoms()[..4]).is_none());
    }
}
