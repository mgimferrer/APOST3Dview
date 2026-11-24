//! Several geometries of one molecule in order (a reaction path from an
//! IRC or NEB, an optimization, a trajectory), as read from a multi-frame
//! .xyz file: the same atoms in every frame, only the positions change.
//!
//! What the viewer needs to show them as one structure: the frames lined
//! up so the molecule doesn't tumble between them (`aligned_frames`), one
//! bond list for all frames (`path_bonds`) with how much of each bond is
//! there in a given geometry (`bond_presence`, so a bond grows and fades
//! smoothly as it forms and breaks), the geometry between two frames
//! (`positions_at`), and the energies written on the comment lines.

use glam::Vec3;

use crate::molecule::{bond_cutoff, Bond};
use crate::superpose::superpose;
use crate::xyz::XyzFrame;

/// A bond is drawn whole up to the perception cutoff (as for a single
/// geometry) and thins out to nothing at this multiple of it.
pub const BOND_FADE_END: f32 = 1.1;

/// Hartree per electronvolt (CODATA 2018).
const HARTREE_PER_EV: f64 = 1.0 / 27.211386245988;

/// The frames of one molecule.
#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    pub atomic_numbers: Vec<u32>,
    /// Angstrom, as in the file, at least one frame.
    pub frames: Vec<Vec<Vec3>>,
    /// The energy of each frame from its comment line, hartree; `None`
    /// unless every frame has one.
    pub energies: Option<Vec<f64>>,
}

impl Trajectory {
    /// The frames of a file as one molecule; `None` when they don't all
    /// have the same atoms in the same order (then they are different
    /// molecules, to be shown as separate structures).
    pub fn from_frames(frames: &[XyzFrame]) -> Option<Self> {
        let first = frames.first()?;
        if frames.iter().any(|f| f.atomic_numbers != first.atomic_numbers) {
            return None;
        }
        let energies: Option<Vec<f64>> = frames.iter().map(|f| comment_energy(&f.comment)).collect();
        Some(Self {
            atomic_numbers: first.atomic_numbers.clone(),
            frames: frames.iter().map(|f| f.positions.clone()).collect(),
            energies,
        })
    }

    /// Every bond that is there, even partly, in some frame, in atom
    /// order: one list for the whole path, so a bond keeps its number (and
    /// what the user set for it: hidden, dashed) from frame to frame.
    pub fn path_bonds(&self) -> Vec<Bond> {
        let n = self.atomic_numbers.len();
        let mut bonds = Vec::new();
        for a in 0..n {
            for b in (a + 1)..n {
                let reach = bond_cutoff(self.atomic_numbers[a], self.atomic_numbers[b]) * BOND_FADE_END;
                if self.frames.iter().any(|p| {
                    let d = p[a].distance(p[b]);
                    d > 0.1 && d < reach
                }) {
                    bonds.push(Bond { atom_a: a, atom_b: b });
                }
            }
        }
        bonds
    }

    /// Every frame turned and moved onto the one before it (least squares
    /// on `fit_atoms`, or on all atoms), so the molecule doesn't tumble or
    /// drift as the frames go by; the first frame stays as in the file.
    /// Chaining onto the previous frame rather than the first keeps every
    /// fit between nearly equal geometries. A frame whose fit can't be
    /// made (fewer than three atoms) follows the previous frame's motion.
    pub fn aligned_frames(&self, fit_atoms: Option<&[usize]>) -> Vec<Vec<Vec3>> {
        let n = self.atomic_numbers.len();
        let atoms: Vec<usize> = match fit_atoms {
            Some(atoms) => atoms.iter().copied().filter(|&a| a < n).collect(),
            None => (0..n).collect(),
        };
        let mut aligned: Vec<Vec<Vec3>> = Vec::with_capacity(self.frames.len());
        let mut previous_motion = (glam::Quat::IDENTITY, Vec3::ZERO);
        for frame in &self.frames {
            let Some(reference) = aligned.last() else {
                aligned.push(frame.clone());
                continue;
            };
            let pick = |positions: &[Vec3]| atoms.iter().map(|&a| positions[a]).collect::<Vec<_>>();
            let (rotation, translation) = superpose(&pick(reference), &pick(frame)).map_or(previous_motion, |s| (s.rotation, s.translation));
            previous_motion = (rotation, translation);
            aligned.push(frame.iter().map(|&p| rotation * p + translation).collect());
        }
        aligned
    }
}

/// How much of the bond between atoms `a` and `b` is there at these
/// positions: 1 up to the perception cutoff, 0 from `BOND_FADE_END` times
/// it, smooth in between.
pub fn bond_presence(atomic_numbers: &[u32], positions: &[Vec3], a: usize, b: usize) -> f32 {
    let cutoff = bond_cutoff(atomic_numbers[a], atomic_numbers[b]);
    let d = positions[a].distance(positions[b]);
    if d <= 0.1 {
        return 0.0;
    }
    let t = ((cutoff * BOND_FADE_END - d) / (cutoff * (BOND_FADE_END - 1.0))).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The geometry at `position` along the frames (0 = the first frame, 1 =
/// the second, 1.5 halfway between them): a Catmull-Rom curve through the
/// frames, so atoms pass through every frame without a kink at each one.
/// The ends are held (the curve is clamped to the first and last frame).
pub fn positions_at(frames: &[Vec<Vec3>], position: f32) -> Vec<Vec3> {
    let Some(last) = frames.len().checked_sub(1) else { return Vec::new() };
    let position = position.clamp(0.0, last as f32);
    let i = (position.floor() as usize).min(last.saturating_sub(1));
    let t = position - i as f32;
    if last == 0 || t == 0.0 {
        return frames[i].clone();
    }
    if t == 1.0 {
        return frames[i + 1].clone();
    }
    let p0 = &frames[i.saturating_sub(1)];
    let (p1, p2) = (&frames[i], &frames[i + 1]);
    let p3 = &frames[(i + 2).min(last)];
    let (t2, t3) = (t * t, t * t * t);
    (0..p1.len())
        .map(|a| {
            0.5 * (2.0 * p1[a]
                + (p2[a] - p0[a]) * t
                + (2.0 * p0[a] - 5.0 * p1[a] + 4.0 * p2[a] - p3[a]) * t2
                + (3.0 * p1[a] - p0[a] - 3.0 * p2[a] + p3[a]) * t3)
        })
        .collect()
}

/// The energy on an .xyz comment line, hartree, when it says one: an
/// extended-xyz `energy=` (electronvolt, as ASE writes it), a number after
/// `E` or `energy` (ORCA's `... E -76.4`, xtb's `energy: -5.07 ...`,
/// hartree), or a comment that is only a number (hartree).
pub fn comment_energy(comment: &str) -> Option<f64> {
    let tokens: Vec<&str> = comment.split_whitespace().collect();
    // Extended xyz: key=value pairs, energies in eV.
    if comment.contains("Properties=") || comment.to_ascii_lowercase().contains("energy=") {
        return tokens.iter().find_map(|t| {
            let (key, value) = t.split_once('=')?;
            (key.eq_ignore_ascii_case("energy")).then(|| value.trim_matches('"').parse::<f64>().ok())?.map(|e| e * HARTREE_PER_EV)
        });
    }
    if let [only] = tokens.as_slice() {
        return only.parse().ok();
    }
    tokens.windows(2).find_map(|pair| {
        let key = pair[0].trim_end_matches([':', '=']);
        (key == "E" || key.eq_ignore_ascii_case("energy")).then(|| pair[1].parse::<f64>().ok())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    fn frame(positions: Vec<Vec3>, comment: &str) -> XyzFrame {
        XyzFrame { atomic_numbers: vec![6, 6, 1, 1], positions, comment: comment.to_string() }
    }

    /// Two carbons pulled apart along x, plus two hydrogens.
    fn stretching(d: f32) -> Vec<Vec3> {
        vec![Vec3::ZERO, Vec3::new(d, 0.0, 0.0), Vec3::new(-1.09, 0.0, 0.0), Vec3::new(d + 1.09, 0.0, 0.3)]
    }

    #[test]
    fn energies_from_comment_lines() {
        assert_eq!(comment_energy("Coordinates from ORCA-job neb E -76.123"), Some(-76.123));
        assert_eq!(comment_energy(" energy: -5.070544 gnorm: 0.0003 xtb: 6.7.1"), Some(-5.070544));
        assert_eq!(comment_energy("Energy: -40.5"), Some(-40.5));
        assert_eq!(comment_energy("-153.2"), Some(-153.2));
        assert_eq!(comment_energy("Energy:       unknown"), None);
        assert_eq!(comment_energy("water"), None);
        assert_eq!(comment_energy(""), None);
        let ev = comment_energy("Lattice=\"\" Properties=species:S:1:pos:R:3 energy=-27.211386245988 pbc=\"F F F\"").unwrap();
        assert!((ev + 1.0).abs() < 1e-12);
    }

    #[test]
    fn energies_only_when_every_frame_has_one() {
        let with = Trajectory::from_frames(&[frame(stretching(1.5), "E -1.0"), frame(stretching(1.6), "E -0.9")]).unwrap();
        assert_eq!(with.energies, Some(vec![-1.0, -0.9]));
        let without = Trajectory::from_frames(&[frame(stretching(1.5), "E -1.0"), frame(stretching(1.6), "")]).unwrap();
        assert_eq!(without.energies, None);
    }

    #[test]
    fn different_atoms_are_not_one_path() {
        let mut other = frame(stretching(1.5), "");
        other.atomic_numbers[3] = 8;
        assert!(Trajectory::from_frames(&[frame(stretching(1.5), ""), other]).is_none());
    }

    #[test]
    fn a_breaking_bond_fades_and_stays_in_the_list() {
        let cutoff = bond_cutoff(6, 6);
        let path = Trajectory::from_frames(&[frame(stretching(1.5), ""), frame(stretching(cutoff * 1.05), ""), frame(stretching(3.0), "")]).unwrap();
        let bonds = path.path_bonds();
        assert!(bonds.iter().any(|b| (b.atom_a, b.atom_b) == (0, 1)), "the C-C bond is in the list");
        let presence: Vec<f32> = path.frames.iter().map(|p| bond_presence(&path.atomic_numbers, p, 0, 1)).collect();
        assert_eq!(presence[0], 1.0);
        assert!((presence[1] - 0.5).abs() < 1e-3, "halfway through the fade: {}", presence[1]);
        assert_eq!(presence[2], 0.0);
        // Whole exactly where a single geometry perceives the bond.
        assert_eq!(bond_presence(&[6, 6], &[Vec3::ZERO, Vec3::new(cutoff * 0.999, 0.0, 0.0)], 0, 1), 1.0);
    }

    #[test]
    fn frames_turned_in_the_file_are_lined_up() {
        let base = stretching(1.5);
        let turn = Quat::from_rotation_y(1.0) * Quat::from_rotation_x(0.4);
        let turned: Vec<Vec3> = base.iter().map(|&p| turn * p + Vec3::new(3.0, -2.0, 1.0)).collect();
        let path = Trajectory::from_frames(&[frame(base.clone(), ""), frame(turned, "")]).unwrap();
        let aligned = path.aligned_frames(None);
        assert_eq!(aligned[0], base);
        for (a, b) in aligned[1].iter().zip(&base) {
            assert!(a.distance(*b) < 1e-4, "{a} vs {b}");
        }
        // On two atoms only: no fit, the frame is left as it is.
        assert_eq!(path.aligned_frames(Some(&[0, 1]))[1], path.frames[1]);
    }

    #[test]
    fn the_curve_passes_through_every_frame() {
        let frames: Vec<Vec<Vec3>> = (0..4).map(|k| vec![Vec3::new(k as f32, (k * k) as f32, 0.0)]).collect();
        for (k, f) in frames.iter().enumerate() {
            assert_eq!(positions_at(&frames, k as f32), *f);
        }
        let middle = positions_at(&frames, 1.5)[0];
        assert!((middle.x - 1.5).abs() < 1e-6, "even spacing stays even: {middle}");
        assert!(middle.y > 1.0 && middle.y < 4.0);
        assert_eq!(positions_at(&frames, -1.0), frames[0]);
        assert_eq!(positions_at(&frames, 9.0), frames[3]);
        assert_eq!(positions_at(&frames[..1], 0.5), frames[0]);
    }
}
