//! Several geometries of one molecule from a multi-frame .xyz file (a
//! reaction path from an IRC or NEB, an optimization): one structure whose
//! atoms move from frame to frame.
//!
//! The structure's `molecule.positions` always hold the geometry shown,
//! so labels, measurements, selection, picking and export work on it as
//! on any structure. The bond list is the path's (`Trajectory::path_bonds`,
//! every bond there in some frame, then any bonds made by hand): bonds
//! keep their numbers, and with them what the user set (hidden, dashed),
//! from frame to frame. Each frame says how much of each bond is there:
//! a bond thins out as it breaks and grows as it forms.
//!
//! Playback and the GIF follow one timeline (`timeline`): the frames, and
//! the geometries between them, each shown for a fixed time, with pauses
//! at the ends. What plays in the window is what the GIF records.
//!
//! Views can be kept at chosen frames (`KeptView`): while the frames play
//! (and in the GIF) the camera then moves from one kept view to the next,
//! eased in and out, and stays on the first before it and on the last
//! after it. Each view belongs to a place along the frames, not to a
//! moment, so going back retraces the turn.

use std::collections::HashSet;

use apost3dview_core::{bond_presence, positions_at, superpose, Trajectory};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::app::App;

/// Below this a fading bond is not drawn (nor picked).
const MIN_BOND_PRESENCE: f32 = 0.02;

/// The fastest frame rate a GIF plays reliably (its delays are in
/// hundredths of a second, and browsers slow anything faster down).
pub(crate) const MAX_GIF_FPS: f32 = 50.0;

/// How the frames are lined up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FrameAlignment {
    /// Each frame turned onto the one before, on all atoms (the default).
    Previous,
    /// On these atoms only (0-based), e.g. a catalyst's core.
    Atoms(Vec<usize>),
    /// As written in the file.
    File,
}

/// What happens at the end of the frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LoopMode {
    /// First to last, then back, and again: the natural one for a reaction.
    BackAndForth,
    /// First to last, again and again.
    Repeat,
    /// First to last, then stop.
    Once,
}

impl LoopMode {
    pub(crate) const ALL: [LoopMode; 3] = [LoopMode::BackAndForth, LoopMode::Repeat, LoopMode::Once];

    pub(crate) fn name(self) -> &'static str {
        match self {
            LoopMode::BackAndForth => "Back and forth",
            LoopMode::Repeat => "Repeat",
            LoopMode::Once => "Once",
        }
    }
}

/// How the frames play, live and in the GIF.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PlayerSettings {
    /// Frames of the file per second.
    pub(crate) speed: f32,
    /// Geometries drawn between two frames of the file: 0 jumps from frame
    /// to frame, more is smoother.
    pub(crate) between: u32,
    pub(crate) loop_mode: LoopMode,
    /// Seconds spent on the first and the last frame.
    pub(crate) pause: f32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self { speed: 5.0, between: 4, loop_mode: LoopMode::BackAndForth, pause: 0.5 }
    }
}

impl PlayerSettings {
    /// Pictures per second while the frames move.
    pub(crate) fn fps(&self) -> f32 {
        self.speed * (self.between + 1) as f32
    }
}

/// One step of the timeline: a position along the frames (0 = the first
/// frame, 1.5 = halfway between the second and third) shown for `seconds`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Step {
    pub(crate) position: f32,
    pub(crate) seconds: f32,
}

/// The steps of one cycle of playback for `n` frames: forward through
/// every frame and the geometries between, with the pause on the last
/// frame; back again for Back and forth; the first frame's pause at the
/// start. A cycle of Repeat goes back to the first frame in one jump.
pub(crate) fn timeline(n: usize, settings: &PlayerSettings) -> Vec<Step> {
    let parts = settings.between as usize + 1;
    let last = n.saturating_sub(1) * parts;
    let step = 1.0 / settings.fps().max(0.01);
    let pause = settings.pause.max(0.0);
    let at = |k: usize| k as f32 / parts as f32;
    let mut steps = Vec::with_capacity(2 * last + 2);
    // Forward: the first frame with its pause, then on to the last.
    for k in 0..=last {
        let mut seconds = step;
        if k == 0 {
            seconds += pause;
        }
        if k == last {
            seconds += pause;
        }
        steps.push(Step { position: at(k), seconds });
    }
    if settings.loop_mode == LoopMode::BackAndForth && last > 0 {
        // Back, without showing the last frame twice; the first frame
        // starts the next cycle.
        for k in (1..last).rev() {
            steps.push(Step { position: at(k), seconds: step });
        }
    }
    steps
}

/// Total time of a timeline, seconds.
pub(crate) fn cycle_seconds(steps: &[Step]) -> f32 {
    steps.iter().map(|s| s.seconds).sum()
}

/// The step shown `t` seconds into a timeline; `None` past its end.
pub(crate) fn step_at(steps: &[Step], t: f32) -> Option<&Step> {
    let mut start = 0.0;
    for step in steps {
        if t < start + step.seconds {
            return Some(step);
        }
        start += step.seconds;
    }
    None
}

/// When a timeline first shows `position` (or the nearest step to it).
fn time_of(steps: &[Step], position: f32) -> f32 {
    let mut start = 0.0;
    let mut best = (f32::INFINITY, 0.0);
    for step in steps {
        let distance = (step.position - position).abs();
        if distance < best.0 {
            best = (distance, start);
        }
        start += step.seconds;
    }
    best.1
}

/// A view kept at a place along the frames: where the camera looks, how
/// far it is and how it is turned, in the coordinates of the frames as
/// drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct KeptView {
    /// Along the frames, 0 = the first frame.
    pub(crate) position: f32,
    pub(crate) target: Vec3,
    pub(crate) distance: f32,
    pub(crate) orientation: Quat,
}

/// Two kept views closer than this along the frames are the same place.
const SAME_PLACE: f32 = 1e-3;

/// The camera (target, distance, orientation) at `position` along the
/// frames, from views sorted by position: the nearest view outside them,
/// eased from one to the next between them (turned along the shortest
/// way, zoomed evenly). `None` without views.
pub(crate) fn view_at(views: &[KeptView], position: f32) -> Option<(Vec3, f32, Quat)> {
    let parts = |v: &KeptView| (v.target, v.distance, v.orientation);
    let (first, last) = (views.first()?, views.last()?);
    if position <= first.position {
        return Some(parts(first));
    }
    if position >= last.position {
        return Some(parts(last));
    }
    let i = views.windows(2).position(|w| position < w[1].position)?;
    let (a, b) = (&views[i], &views[i + 1]);
    let t = ((position - a.position) / (b.position - a.position)).clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    let end = if a.orientation.dot(b.orientation) < 0.0 { -b.orientation } else { b.orientation };
    let orientation = a.orientation.slerp(end, s).normalize();
    let distance = a.distance * (b.distance / a.distance.max(1e-6)).powf(s);
    Some((a.target.lerp(b.target, s), distance, orientation))
}

/// A place along the frames as the player shows it: "12" for a frame,
/// "12.5" between two (1-based).
pub(crate) fn place_text(position: f32) -> String {
    let shown = position + 1.0;
    if (shown - shown.round()).abs() < SAME_PLACE { format!("{}", shown.round() as usize) } else { format!("{shown:.1}") }
}

/// The frames of a structure opened from a multi-frame .xyz file.
pub(crate) struct Frames {
    /// The file's frames and energies.
    pub(crate) path: Trajectory,
    /// The frames as drawn: lined up by `alignment`.
    pub(crate) drawn: Vec<Vec<Vec3>>,
    pub(crate) alignment: FrameAlignment,
    /// Where along the frames the structure is shown (0 = first frame).
    pub(crate) position: f32,
    /// The first `path_bonds` bonds of the molecule are the path's; any
    /// after were made by hand, and are always drawn.
    pub(crate) path_bonds: usize,
    /// How much of each path bond is there in the geometry shown.
    pub(crate) bond_presence: Vec<f32>,
    /// Path bonds not drawn in the geometry shown.
    pub(crate) absent_bonds: HashSet<usize>,
    pub(crate) player: PlayerSettings,
    pub(crate) playing: bool,
    /// Seconds into the playback cycle.
    pub(crate) clock: f32,
    /// Views kept at places along the frames, sorted by place.
    pub(crate) views: Vec<KeptView>,
    /// Whether the camera follows the kept views (when there are any).
    pub(crate) follow_views: bool,
}

impl Frames {
    pub(crate) fn new(path: Trajectory, path_bonds: usize) -> Self {
        let drawn = path.aligned_frames(None);
        Self {
            path,
            drawn,
            alignment: FrameAlignment::Previous,
            position: 0.0,
            path_bonds,
            bond_presence: vec![1.0; path_bonds],
            absent_bonds: HashSet::new(),
            player: PlayerSettings::default(),
            playing: false,
            clock: 0.0,
            views: Vec::new(),
            follow_views: true,
        }
    }

    /// Whether the camera follows kept views.
    pub(crate) fn following(&self) -> bool {
        self.follow_views && !self.views.is_empty()
    }

    /// Keeps `camera`'s view at the place shown (replacing a view kept
    /// there); its index.
    pub(crate) fn keep_view(&mut self, camera: &apost3dview_render::OrbitCamera) -> usize {
        let view = KeptView { position: self.position, target: camera.target, distance: camera.distance, orientation: camera.orientation };
        self.add_view(view)
    }

    /// Adds a view in its place (replacing one kept there); its index.
    pub(crate) fn add_view(&mut self, view: KeptView) -> usize {
        if let Some(i) = self.view_here(view.position) {
            self.views[i] = view;
            return i;
        }
        let i = self.views.partition_point(|v| v.position < view.position);
        self.views.insert(i, view);
        i
    }

    /// The kept view at `position`, if any.
    pub(crate) fn view_here(&self, position: f32) -> Option<usize> {
        self.views.iter().position(|v| (v.position - position).abs() < SAME_PLACE)
    }

    pub(crate) fn count(&self) -> usize {
        self.drawn.len()
    }

    pub(crate) fn last(&self) -> f32 {
        self.count().saturating_sub(1) as f32
    }

    /// The frames lined up again, after the alignment changed. Kept views
    /// move with their frame, so they still show it as they did.
    pub(crate) fn realign(&mut self) {
        let old = std::mem::take(&mut self.drawn);
        self.drawn = match &self.alignment {
            FrameAlignment::Previous => self.path.aligned_frames(None),
            FrameAlignment::Atoms(atoms) => self.path.aligned_frames(Some(atoms)),
            FrameAlignment::File => self.path.frames.clone(),
        };
        if old.len() != self.drawn.len() {
            return;
        }
        for view in &mut self.views {
            let k = (view.position.round() as usize).min(old.len() - 1);
            // new ≈ rotation · old + translation (exactly: both are the
            // same frame of the file, turned and moved).
            if let Some(fit) = superpose(&self.drawn[k], &old[k]) {
                view.target = fit.rotation * view.target + fit.translation;
                view.orientation = (fit.rotation * view.orientation).normalize();
            }
        }
    }

    /// A sphere holding every frame as drawn: center and radius, angstrom.
    pub(crate) fn bounds(&self) -> (Vec3, f32) {
        let all = self.drawn.iter().flatten();
        let count = self.drawn.iter().map(Vec::len).sum::<usize>().max(1);
        let center = all.clone().fold(Vec3::ZERO, |acc, &p| acc + p) / count as f32;
        let radius = all.map(|p| p.distance(center)).fold(0.0_f32, f32::max).max(1.0);
        (center, radius)
    }

    /// The energy of each frame relative to the first, kcal/mol.
    pub(crate) fn relative_energies(&self) -> Option<Vec<f64>> {
        let energies = self.path.energies.as_ref()?;
        let first = *energies.first()?;
        Some(energies.iter().map(|e| (e - first) * crate::results_view::KCAL_PER_HARTREE).collect())
    }

    /// The frame number shown (1-based) and whether it lies between two.
    pub(crate) fn frame_text(&self) -> String {
        let n = self.count();
        let nearest = self.position.round();
        if (self.position - nearest).abs() < 1e-3 {
            format!("Frame {} of {n}", nearest as usize + 1)
        } else {
            format!("Between frames {} and {} of {n}", self.position.floor() as usize + 1, self.position.floor() as usize + 2)
        }
    }
}

impl App {
    /// Shows structure `index` at `position` along its frames (clamped):
    /// moves its atoms, works out which bonds are there, and redraws if it
    /// is the active structure.
    pub(crate) fn show_frame_position(&mut self, index: usize, position: f32) {
        let Some(structure) = self.structures.get_mut(index) else { return };
        let Some(frames) = structure.frames.as_mut() else { return };
        frames.position = position.clamp(0.0, frames.last());
        structure.molecule.positions = positions_at(&frames.drawn, frames.position);
        let molecule = &structure.molecule;
        frames.absent_bonds.clear();
        for (i, bond) in molecule.bonds.iter().take(frames.path_bonds).enumerate() {
            let presence = bond_presence(&molecule.atomic_numbers, &molecule.positions, bond.atom_a, bond.atom_b);
            frames.bond_presence[i] = presence;
            if presence < MIN_BOND_PRESENCE {
                frames.absent_bonds.insert(i);
            }
        }
        if self.active_structure == Some(index) {
            self.geometry_moved();
        }
    }

    /// Shows structure `index` at `position` and, when it follows kept
    /// views, turns the camera to the view there: where the player, the
    /// keys and the GIF go.
    pub(crate) fn go_to_frame(&mut self, index: usize, position: f32) {
        self.show_frame_position(index, position);
        self.follow_kept_views(index);
    }

    /// Turns the camera to the kept view at the place shown, when the
    /// active structure `index` follows kept views.
    pub(crate) fn follow_kept_views(&mut self, index: usize) {
        if self.active_structure != Some(index) {
            return;
        }
        let Some(frames) = self.structures.get(index).and_then(|s| s.frames.as_ref()) else { return };
        if !frames.following() {
            return;
        }
        if let Some((target, distance, orientation)) = view_at(&frames.views, frames.position) {
            self.camera.target = target;
            self.camera.distance = distance;
            self.camera.orientation = orientation;
        }
    }

    /// Whether kept views turn the active structure while it plays (so
    /// Spin waits).
    pub(crate) fn kept_views_turning(&self) -> bool {
        self.active_structure.and_then(|i| self.structures.get(i)).and_then(|s| s.frames.as_ref()).is_some_and(|f| f.playing && f.following())
    }

    /// Keeps the current view at the place the active structure's frames
    /// are shown — public for tests.
    pub fn keep_view_here(&mut self) {
        let camera = self.camera;
        let Some(frames) = self.active_structure.and_then(|i| self.structures.get_mut(i)).and_then(|s| s.frames.as_mut()) else { return };
        let replaced = frames.view_here(frames.position).is_some();
        frames.keep_view(&camera);
        let place = place_text(frames.position);
        self.show_status(if replaced { format!("View at frame {place} replaced") } else { format!("View kept at frame {place}") });
    }

    /// The places (1-based frames) of the active structure's kept views —
    /// for tests.
    pub fn kept_view_frames(&self) -> Vec<f32> {
        self.active_structure
            .and_then(|i| self.structures.get(i))
            .and_then(|s| s.frames.as_ref())
            .map(|f| f.views.iter().map(|v| v.position + 1.0).collect())
            .unwrap_or_default()
    }

    /// Redraws the active structure after its atoms moved.
    pub(crate) fn geometry_moved(&mut self) {
        self.rebuild_geometry();
        self.rebuild_highlights();
        self.rebuild_measurements();
    }

    /// Lines up the frames of structure `index` in another way, keeping
    /// the position shown (and the kept views on their frames).
    pub(crate) fn set_frame_alignment(&mut self, index: usize, alignment: FrameAlignment) {
        let Some(frames) = self.structures.get_mut(index).and_then(|s| s.frames.as_mut()) else { return };
        frames.alignment = alignment;
        frames.realign();
        let position = frames.position;
        self.go_to_frame(index, position);
    }

    /// Starts or stops playing the active structure's frames. Playing
    /// starts where the frames are (from the first frame when at the end of
    /// a single pass).
    pub(crate) fn toggle_playing(&mut self) {
        let Some(frames) = self.active_structure.and_then(|i| self.structures.get_mut(i)).and_then(|s| s.frames.as_mut()) else { return };
        frames.playing = !frames.playing;
        if frames.playing {
            let steps = timeline(frames.count(), &frames.player);
            let at_end = frames.position >= frames.last();
            frames.clock = if at_end && frames.player.loop_mode != LoopMode::BackAndForth { 0.0 } else { time_of(&steps, frames.position) };
        }
    }

    /// Moves the frames on by `dt` seconds while playing; true when the
    /// view changed (so another frame is wanted).
    pub(crate) fn advance_playback(&mut self, dt: f32) -> bool {
        let Some(index) = self.active_structure else { return false };
        let Some(frames) = self.structures.get_mut(index).and_then(|s| s.frames.as_mut()) else { return false };
        if !frames.playing {
            return false;
        }
        let steps = timeline(frames.count(), &frames.player);
        let cycle = cycle_seconds(&steps);
        frames.clock += dt;
        if frames.clock >= cycle {
            if frames.player.loop_mode == LoopMode::Once {
                frames.playing = false;
                let last = frames.last();
                self.go_to_frame(index, last);
                return true;
            }
            frames.clock %= cycle.max(1e-3);
        }
        let position = step_at(&steps, frames.clock).map_or(frames.position, |s| s.position);
        if position != frames.position {
            self.go_to_frame(index, position);
        }
        true
    }

    /// Opens every frame of the active structure's path as a structure of
    /// its own (as drawn, lined up), e.g. to compare two side by side or
    /// make panels of them.
    pub(crate) fn frames_as_structures(&mut self) {
        let Some(active) = self.active_structure else { return };
        let structure = &self.structures[active];
        let Some(frames) = &structure.frames else { return };
        let new: Vec<crate::state::LoadedStructure> = frames
            .drawn
            .iter()
            .enumerate()
            .map(|(k, positions)| {
                let molecule = apost3dview_core::Molecule::from_atoms(frames.path.atomic_numbers.clone(), positions.clone());
                let mut s = crate::state::LoadedStructure::new(format!("{} ({})", structure.label, k + 1), molecule, structure.source_path.clone());
                s.file_frame = Some(k);
                s
            })
            .collect();
        let first = self.structures.len();
        let n = new.len();
        self.structures.extend(new);
        self.keep_view = true;
        self.set_active(first);
        self.show_status(format!("{n} frames opened as structures"));
    }

    /// Goes to the next (`1`) or previous (`-1`) frame of the file,
    /// stopping playback.
    pub(crate) fn step_frame(&mut self, direction: i32) {
        let Some(index) = self.active_structure else { return };
        let Some(frames) = self.structures.get_mut(index).and_then(|s| s.frames.as_mut()) else { return };
        frames.playing = false;
        let target = if direction > 0 { frames.position.floor() + 1.0 } else { frames.position.ceil() - 1.0 };
        self.go_to_frame(index, target);
    }

    /// Where the active structure is along its frames — for tests.
    pub fn frame_position(&self) -> Option<f32> {
        self.active_structure.and_then(|i| self.structures.get(i)).and_then(|s| s.frames.as_ref()).map(|f| f.position)
    }

    /// Shows the active structure at `position` along its frames, seen
    /// from the kept views if it follows them — for tests (and later,
    /// scripting).
    pub fn go_to_frame_position(&mut self, position: f32) {
        if let Some(index) = self.active_structure {
            self.go_to_frame(index, position);
        }
    }

    /// The atom positions of the active structure as shown — for tests.
    pub fn atom_positions(&self) -> Vec<[f32; 3]> {
        self.active_structure.and_then(|i| self.structures.get(i)).map(|s| s.molecule.positions.iter().map(|p| p.to_array()).collect()).unwrap_or_default()
    }

    /// The bonds drawn in the active structure, as 1-based atom pairs — for
    /// tests.
    pub fn drawn_bonds(&self) -> Vec<[usize; 2]> {
        let Some(structure) = self.active_structure.and_then(|i| self.structures.get(i)) else { return Vec::new() };
        let hidden = structure.drawn_hidden_bonds();
        structure.molecule.bonds.iter().enumerate().filter(|(i, _)| !hidden.contains(i)).map(|(_, b)| [b.atom_a + 1, b.atom_b + 1]).collect()
    }
}

/// Height of the energy profile in the player bar, points.
const PROFILE_HEIGHT: f32 = 46.0;
const PROFILE_WIDTH: f32 = 260.0;

impl App {
    /// The player along the bottom, above the status bar, while the active
    /// structure has frames: step, play, the frame slider, how it plays,
    /// and the energy profile when the file gives energies.
    pub(crate) fn show_frames_bar(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.active_structure else { return };
        if self.structures.get(index).and_then(|s| s.frames.as_ref()).is_none() {
            return;
        }
        let response = egui::Panel::bottom("frames_bar").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    self.show_player_controls(ui, index);
                    self.show_kept_views_row(ui, index);
                });
                if let Some(energies) = self.structures[index].frames.as_ref().and_then(Frames::relative_energies) {
                    ui.separator();
                    self.show_energy_profile(ui, index, &energies);
                }
            });
            ui.add_space(2.0);
        });
        self.window_column.bottom = response.response.rect.top();
    }

    fn show_player_controls(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(frames) = self.structures[index].frames.as_ref() else { return };
        let (position, last, playing, mut player) = (frames.position, frames.last(), frames.playing, frames.player);
        let marks: Vec<f32> = frames.views.iter().map(|v| v.position).collect();
        let mut go_to = None;
        let mut toggle = false;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.button("\u{23ee}").on_hover_text("First frame").clicked() {
                    go_to = Some(0.0);
                }
                if ui.button("\u{25c0}").on_hover_text("Previous frame (,)").clicked() {
                    go_to = Some((position.ceil() - 1.0).max(0.0));
                }
                let play = if playing { "\u{23f8} Pause" } else { "\u{25b6} Play" };
                if ui.button(play).on_hover_text("Play the frames. Rotate, zoom and change the look while they play: the GIF (Export window) records the same.").clicked() {
                    toggle = true;
                }
                if ui.button("\u{25b6}|").on_hover_text("Next frame (.)").clicked() {
                    go_to = Some((position.floor() + 1.0).min(last));
                }
                if ui.button("\u{23ed}").on_hover_text("Last frame").clicked() {
                    go_to = Some(last);
                }
                // No step on the slider itself: it would snap a position
                // between two frames (while playing) to a frame. What the
                // user picks is rounded to a frame instead.
                let mut shown = position + 1.0;
                let slider = egui::Slider::new(&mut shown, 1.0..=last + 1.0).show_value(false);
                let slider = ui.add_sized([220.0, 18.0], slider);
                if slider.changed() {
                    go_to = Some((shown - 1.0).round());
                }
                // The kept views, as marks under the slider's rail (which
                // runs between the handle's centers at both ends).
                let rect = slider.rect;
                let inset = rect.height() / 2.5;
                let color = ui.visuals().selection.bg_fill;
                for &mark in &marks {
                    let x = rect.left() + inset + (rect.width() - 2.0 * inset) * mark / last.max(1.0);
                    let y = rect.bottom() + 1.0;
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![egui::pos2(x, y - 4.0), egui::pos2(x + 3.5, y + 1.5), egui::pos2(x - 3.5, y + 1.5)],
                        color,
                        egui::Stroke::NONE,
                    ));
                }
                ui.label(self.structures[index].frames.as_ref().map(Frames::frame_text).unwrap_or_default());
            });
            ui.horizontal(|ui| {
                ui.label("Speed:");
                ui.add(egui::DragValue::new(&mut player.speed).range(0.5..=30.0).speed(0.1).max_decimals(1).suffix(" frames/s"))
                    .on_hover_text("Frames of the file per second");
                ui.label("Smoothness:");
                ui.add(egui::DragValue::new(&mut player.between).range(0..=10).suffix(" between"))
                    .on_hover_text("Geometries drawn between two frames of the file, on a smooth curve through them. 0 jumps from frame to frame.");
                egui::ComboBox::from_id_salt("frames_loop").selected_text(player.loop_mode.name()).show_ui(ui, |ui| {
                    for mode in LoopMode::ALL {
                        ui.selectable_value(&mut player.loop_mode, mode, mode.name());
                    }
                });
                ui.label("Pause at the ends:");
                ui.add(egui::DragValue::new(&mut player.pause).range(0.0..=5.0).speed(0.05).max_decimals(1).suffix(" s"));
            });
        });
        if let Some(frames) = self.structures[index].frames.as_mut()
            && frames.player != player
        {
            frames.player = player;
        }
        if toggle {
            self.toggle_playing();
        }
        if let Some(target) = go_to {
            if let Some(frames) = self.structures[index].frames.as_mut() {
                frames.playing = false;
            }
            self.go_to_frame(index, target);
        }
    }

    /// The kept views: keep the current one here, go to or remove one,
    /// and whether the camera follows them.
    fn show_kept_views_row(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(frames) = self.structures[index].frames.as_ref() else { return };
        let places: Vec<f32> = frames.views.iter().map(|v| v.position).collect();
        let here = frames.view_here(frames.position).is_some();
        let mut follow = frames.follow_views;
        let mut keep = false;
        let mut go_to = None;
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("Views:");
            let keep_text = if here { "Replace the view here" } else { "Keep this view here" };
            if ui
                .button(keep_text)
                .on_hover_text("Keeps how the molecule is seen now (turn, zoom, position) at this frame. As the frames play, and in the GIF, the camera moves smoothly from each kept view to the next.")
                .clicked()
            {
                keep = true;
            }
            if places.is_empty() {
                ui.label(egui::RichText::new("Turn and zoom at a frame, then keep that view: the camera moves from view to view as the frames play.").small().weak());
            }
            for (i, &place) in places.iter().enumerate() {
                let text = place_text(place);
                if ui.small_button(&text).on_hover_text(format!("Go to frame {text}, as kept")).clicked() {
                    go_to = Some(place);
                }
                if ui.small_button("\u{d7}").on_hover_text(format!("Remove the view kept at frame {text}")).clicked() {
                    remove = Some(i);
                }
            }
            if !places.is_empty() {
                ui.checkbox(&mut follow, "Follow the kept views")
                    .on_hover_text("Off: the frames play from whatever view you turn to, and Spin turns them, as without kept views. The views stay kept.");
            }
        });
        if keep {
            self.keep_view_here();
        }
        let Some(frames) = self.structures[index].frames.as_mut() else { return };
        let turned_on = follow && !frames.follow_views;
        frames.follow_views = follow;
        if let Some(i) = remove {
            frames.views.remove(i);
            self.show_status("Kept view removed");
        }
        if let Some(place) = go_to {
            if let Some(frames) = self.structures[index].frames.as_mut() {
                frames.playing = false;
            }
            // Going to a kept view shows it, even when not following.
            self.show_frame_position(index, place);
            if let Some((target, distance, orientation)) = self.structures[index].frames.as_ref().and_then(|f| view_at(&f.views, place)) {
                self.camera.target = target;
                self.camera.distance = distance;
                self.camera.orientation = orientation;
            }
        } else if turned_on {
            self.follow_kept_views(index);
        }
    }

    /// The energy of each frame relative to the first, with where the
    /// frames are; click or drag on it to go to a frame.
    fn show_energy_profile(&mut self, ui: &mut egui::Ui, index: usize, energies: &[f64]) {
        let Some(frames) = self.structures[index].frames.as_ref() else { return };
        let (position, last) = (frames.position, frames.last().max(1.0));
        let (low, high) = energies.iter().fold((0.0_f64, 0.0_f64), |(lo, hi), &e| (lo.min(e), hi.max(e)));
        let span = (high - low).max(1e-6);
        ui.vertical(|ui| {
            let highest = energies.iter().cloned().fold(f64::MIN, f64::max);
            ui.label(egui::RichText::new(format!("\u{394}E, kcal/mol (highest {highest:+.1})")).small());
            let (rect, response) = ui.allocate_exact_size(egui::vec2(PROFILE_WIDTH, PROFILE_HEIGHT), egui::Sense::click_and_drag());
            let inner = rect.shrink2(egui::vec2(5.0, 5.0));
            let point = |k: f32, e: f64| egui::pos2(inner.left() + inner.width() * k / last, inner.bottom() - inner.height() * ((e - low) / span) as f32);
            let painter = ui.painter_at(rect);
            let visuals = ui.visuals();
            let ink = visuals.text_color();
            let line = visuals.widgets.inactive.fg_stroke.color;
            // The first frame's energy, as a faint guide.
            let zero = point(0.0, 0.0).y;
            painter.line_segment([egui::pos2(inner.left(), zero), egui::pos2(inner.right(), zero)], egui::Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.5)));
            let points: Vec<egui::Pos2> = energies.iter().enumerate().map(|(k, &e)| point(k as f32, e)).collect();
            painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, line)));
            for p in &points {
                painter.circle_filled(*p, 2.5, line);
            }
            let x = inner.left() + inner.width() * position / last;
            let accent = visuals.selection.bg_fill;
            painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(2.0, accent));
            if let Some(pointer) = response.hover_pos() {
                let k = (((pointer.x - inner.left()) / inner.width()) * last).round().clamp(0.0, last) as usize;
                painter.circle_stroke(points[k], 4.0, egui::Stroke::new(1.5, ink));
                response.clone().on_hover_text(format!("Frame {}: {:+.1} kcal/mol", k + 1, energies[k]));
            }
            if (response.clicked() || response.dragged())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let target = (((pointer.x - inner.left()) / inner.width()) * last).round().clamp(0.0, last);
                if let Some(frames) = self.structures[index].frames.as_mut() {
                    frames.playing = false;
                }
                self.go_to_frame(index, target);
            }
        });
    }

    /// The Molecule window's section for a structure with frames: how they
    /// are lined up, and opening them as structures of their own.
    pub(crate) fn show_frames_section(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(frames) = self.structures[index].frames.as_ref() else { return };
        let n = frames.count();
        let alignment = frames.alignment.clone();
        let has_energies = frames.path.energies.is_some();
        let selected = self.structures[index].selected_atoms.clone();
        ui.label(egui::RichText::new(format!("{n} frames")).strong());
        ui.label(
            egui::RichText::new(if has_energies {
                "Play them with the bar at the bottom. The energies on the file's comment lines are plotted there."
            } else {
                "Play them with the bar at the bottom."
            })
            .small()
            .weak(),
        );
        let mut choice = alignment.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Line up:");
            let on_atoms = matches!(alignment, FrameAlignment::Atoms(_));
            if ui.selectable_label(alignment == FrameAlignment::Previous, "Each frame on the one before")
                .on_hover_text("Turns and moves each frame onto the one before it (all atoms), so the molecule doesn't tumble or drift between frames")
                .clicked()
            {
                choice = FrameAlignment::Previous;
            }
            let atoms_text = match &alignment {
                FrameAlignment::Atoms(atoms) => format!("On {} atoms", atoms.len()),
                _ => "On the selected atoms".to_string(),
            };
            let can_pick = selected.len() >= 3;
            let response = ui
                .add_enabled(can_pick || on_atoms, egui::Button::selectable(on_atoms, atoms_text))
                .on_hover_text("Lines the frames up on the atoms selected (three or more), e.g. a catalyst or the part that doesn't react, so the rest is seen moving")
                .on_disabled_hover_text("Select three or more atoms first");
            if response.clicked() && can_pick {
                choice = FrameAlignment::Atoms(selected.clone());
            }
            if ui.selectable_label(alignment == FrameAlignment::File, "As in the file").clicked() {
                choice = FrameAlignment::File;
            }
        });
        if choice != alignment {
            self.set_frame_alignment(index, choice);
        }
        if ui.button("Open each frame as a structure").on_hover_text("One structure per frame, as lined up here: to compare frames side by side or make panels of them").clicked() {
            self.frames_as_structures();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeline_goes_through_every_frame_and_back() {
        let settings = PlayerSettings { speed: 2.0, between: 1, loop_mode: LoopMode::BackAndForth, pause: 0.5 };
        let steps = timeline(3, &settings);
        let positions: Vec<f32> = steps.iter().map(|s| s.position).collect();
        assert_eq!(positions, vec![0.0, 0.5, 1.0, 1.5, 2.0, 1.5, 1.0, 0.5]);
        // 4 pictures per second, plus the pauses on the first and last frame.
        assert!((cycle_seconds(&steps) - (8.0 * 0.25 + 1.0)).abs() < 1e-6);
        assert_eq!(step_at(&steps, 0.0).unwrap().position, 0.0);
        assert_eq!(step_at(&steps, 0.74).unwrap().position, 0.0);
        assert_eq!(step_at(&steps, 0.76).unwrap().position, 0.5);
        assert!(step_at(&steps, 10.0).is_none());
        assert!((time_of(&steps, 1.5) - 1.25).abs() < 1e-6);
    }

    #[test]
    fn kept_views_are_eased_from_one_to_the_next() {
        let turned = Quat::from_rotation_y(1.0);
        let views = [
            KeptView { position: 2.0, target: Vec3::ZERO, distance: 10.0, orientation: Quat::IDENTITY },
            KeptView { position: 6.0, target: Vec3::new(4.0, 0.0, 0.0), distance: 2.5, orientation: turned },
        ];
        assert!(view_at(&[], 1.0).is_none());
        // Held before the first and after the last.
        assert_eq!(view_at(&views, 0.0), Some((Vec3::ZERO, 10.0, Quat::IDENTITY)));
        assert_eq!(view_at(&views, 9.0), Some((Vec3::new(4.0, 0.0, 0.0), 2.5, turned)));
        // Halfway: half the turn, half the way, the zoom evenly between.
        let (target, distance, orientation) = view_at(&views, 4.0).unwrap();
        assert!((target - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
        assert!((distance - 5.0).abs() < 1e-4);
        assert!(orientation.angle_between(Quat::from_rotation_y(0.5)) < 1e-4);
        // Eased: a quarter of the way along is less than a quarter turned.
        let (_, _, early) = view_at(&views, 3.0).unwrap();
        assert!(early.angle_between(Quat::IDENTITY) < 0.25 * 1.0 - 0.05);
        // The shortest way, even when a quaternion is written the other sign.
        let flipped = [views[0], KeptView { orientation: -turned, ..views[1] }];
        assert!(view_at(&flipped, 4.0).unwrap().2.angle_between(Quat::from_rotation_y(0.5)) < 1e-4);
    }

    #[test]
    fn a_view_kept_in_the_same_place_replaces_the_one_there() {
        let frame = apost3dview_core::XyzFrame { atomic_numbers: vec![1, 1], positions: vec![Vec3::ZERO, Vec3::X], comment: String::new() };
        let mut frames = Frames::new(Trajectory::from_frames(&vec![frame; 3]).unwrap(), 1);
        let view = |position, distance| KeptView { position, target: Vec3::ZERO, distance, orientation: Quat::IDENTITY };
        frames.add_view(view(2.0, 1.0));
        frames.add_view(view(0.0, 1.0));
        frames.add_view(view(2.0, 3.0));
        assert_eq!(frames.views.iter().map(|v| (v.position, v.distance)).collect::<Vec<_>>(), vec![(0.0, 1.0), (2.0, 3.0)]);
        assert_eq!(place_text(10.0), "11");
        assert_eq!(place_text(10.5), "11.5");
    }

    #[test]
    fn without_frames_between_it_jumps_from_frame_to_frame() {
        let settings = PlayerSettings { speed: 5.0, between: 0, loop_mode: LoopMode::Repeat, pause: 0.0 };
        let steps = timeline(4, &settings);
        assert_eq!(steps.iter().map(|s| s.position).collect::<Vec<_>>(), vec![0.0, 1.0, 2.0, 3.0]);
        assert!(steps.iter().all(|s| (s.seconds - 0.2).abs() < 1e-6));
        assert_eq!(timeline(1, &settings).len(), 1);
    }
}
