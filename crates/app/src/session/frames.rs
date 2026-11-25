//! The Frames area's part of a session: where a path's frames were, how
//! they are lined up and played, and the views kept along them.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::frames::{FrameAlignment, Frames};

/// How a structure with frames was left.
#[derive(Serialize, Deserialize)]
pub(super) struct FramesState {
    /// Where along the frames, 1 = the first frame (1.5 = halfway to the
    /// second).
    pub(super) frame: f32,
    #[serde(default)]
    pub(super) line_up: LineUp,
    /// The atoms the frames are lined up on (1-based), for `line_up =
    /// "atoms"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) line_up_atoms: Vec<usize>,
    #[serde(default)]
    pub(super) player: crate::frames::PlayerSettings,
    /// Views kept along the frames (2026-11-24; absent = none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) views: Vec<KeptViewState>,
    /// Whether the camera follows them while the frames play.
    #[serde(default = "follow_views_default", skip_serializing_if = "is_true")]
    pub(super) follow_views: bool,
}

pub(super) fn follow_views_default() -> bool {
    true
}

pub(super) fn is_true(value: &bool) -> bool {
    *value
}

/// A view kept at a place along the frames.
#[derive(Serialize, Deserialize)]
pub(super) struct KeptViewState {
    /// 1 = the first frame.
    pub(super) frame: f32,
    pub(super) target: [f32; 3],
    pub(super) distance: f32,
    /// Quaternion, x y z w.
    pub(super) orientation: [f32; 4],
}

#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum LineUp {
    #[default]
    Previous,
    Atoms,
    File,
}

/// A structure's frames as saved.
pub(super) fn saved_frames(f: &Frames) -> FramesState {
    let (line_up, line_up_atoms) = match &f.alignment {
        FrameAlignment::Previous => (LineUp::Previous, Vec::new()),
        FrameAlignment::Atoms(atoms) => (LineUp::Atoms, atoms.iter().map(|a| a + 1).collect()),
        FrameAlignment::File => (LineUp::File, Vec::new()),
    };
    let views = f
        .views
        .iter()
        .map(|v| KeptViewState { frame: v.position + 1.0, target: v.target.to_array(), distance: v.distance, orientation: v.orientation.to_array() })
        .collect();
    FramesState { frame: f.position + 1.0, line_up, line_up_atoms, player: f.player, views, follow_views: f.follow_views }
}

impl App {
    /// Puts the frames of structure `index` back as saved (only when its
    /// atoms are the ones recorded).
    pub(super) fn restore_frames(&mut self, index: usize, saved: &FramesState) {
        let structure = &mut self.structures[index];
        let n_atoms = structure.molecule.atomic_numbers.len();
        let valid = |atoms: &[usize]| atoms.iter().all(|&a| a >= 1 && a <= n_atoms);
        if let Some(frames) = structure.frames.as_mut() {
            frames.player = saved.player;
            frames.alignment = match saved.line_up {
                LineUp::Previous => FrameAlignment::Previous,
                LineUp::Atoms if valid(&saved.line_up_atoms) => FrameAlignment::Atoms(saved.line_up_atoms.iter().map(|a| a - 1).collect()),
                LineUp::Atoms => FrameAlignment::Previous,
                LineUp::File => FrameAlignment::File,
            };
            frames.realign();
            // Saved in the coordinates of the frames as lined up then: added
            // after lining up, so they aren't carried.
            frames.follow_views = saved.follow_views;
            for v in saved.views.iter().filter(|v| v.distance > 0.0 && v.frame >= 1.0) {
                let orientation = Quat::from_array(v.orientation);
                frames.add_view(crate::frames::KeptView {
                    position: (v.frame - 1.0).min(frames.last()),
                    target: Vec3::from_array(v.target),
                    distance: v.distance,
                    orientation: if orientation.is_normalized() { orientation } else { orientation.normalize() },
                });
            }
            let position = saved.frame - 1.0;
            self.show_frame_position(index, position);
        }
    }
}
