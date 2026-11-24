//! Multi-frame .xyz files (2026-11-24): several geometries of one molecule
//! (a reaction path) open as one structure whose frames are shown one at a
//! time, lined up, with bonds that form and break along the way; they play
//! in the window and as a GIF; sessions keep where the frames were.
//!
//! The example is a reaction path of 22 frames: ethylamine adding to
//! methyl acetate, its N-H hydrogen (atom 15) moving to the methoxy oxygen
//! (atom 8) while the nitrogen (12) bonds to the carbonyl carbon (2) and the
//! C2-O8 bond breaks.

mod common;

use egui::accesskit::{Action, ActionData, ActionRequest, Role};
use egui_kittest::kittest::{By, NodeT, Queryable};

use common::{app_with, repo_dir, scratch_dir, screenshot};

type Harness = egui_kittest::Harness<'static, apost3dview::App>;

fn reaction_path() -> std::path::PathBuf {
    repo_dir().join("examples/reaction-path/reaction-path.xyz")
}

fn has_bond(harness: &Harness, a: usize, b: usize) -> bool {
    harness.state().drawn_bonds().contains(&[a.min(b), a.max(b)])
}

#[test]
fn a_reaction_path_opens_as_one_structure_with_frames() {
    let mut harness = app_with(vec![reaction_path()]);
    harness.run_steps(3);
    assert_eq!(harness.state().structure_labels(), vec!["reaction-path.xyz".to_string()]);
    assert!(harness.query_by_label("22 frames").is_some(), "the Molecule window says how many frames");
    assert!(harness.query_by_label("Frame 1 of 22").is_some());
    assert_eq!(harness.state().frame_position(), Some(0.0));

    // Reactants: N-H on the amine, C-O of the ester, no N-C bond yet.
    assert!(has_bond(&harness, 12, 15) && has_bond(&harness, 2, 8) && !has_bond(&harness, 2, 12) && !has_bond(&harness, 8, 15));
    let first = harness.state().atom_positions();

    // Next frame with the button, then the last one.
    harness.get_by_label("\u{25b6}|").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().frame_position(), Some(1.0));
    assert!(harness.query_by_label("Frame 2 of 22").is_some());
    assert_ne!(harness.state().atom_positions(), first, "the atoms moved");
    harness.get_by_label("\u{23ed}").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().frame_position(), Some(21.0));
    // Products: the amide's N-C bond and methanol's O-H; the ester's C-O
    // and the amine's N-H are gone.
    assert!(has_bond(&harness, 2, 12) && has_bond(&harness, 8, 15) && !has_bond(&harness, 2, 8) && !has_bond(&harness, 12, 15));
    screenshot(&mut harness, "frames-last");

    harness.get_by_label("\u{23ee}").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().atom_positions(), first, "back on the first frame, the same atoms");
}

/// Sets the number field whose shown value contains `shown` (e.g. "frames/s"),
/// as a screen reader would (a DragValue can't be typed into headlessly).
fn set_field(harness: &mut Harness, shown: &str, value: f64) {
    let field = harness
        .get_all(By::new().role(Role::SpinButton))
        .find(|n| n.value().is_some_and(|v| v.contains(shown)))
        .unwrap_or_else(|| panic!("no field showing {shown}"));
    let (target_node, target_tree) = field.accesskit_node().locate();
    harness.event(egui::Event::AccessKitActionRequest(ActionRequest { action: Action::SetValue, target_node, target_tree, data: Some(ActionData::NumericValue(value)) }));
    harness.run_steps(2);
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    glam::Vec3::from_array(a).distance(glam::Vec3::from_array(b))
}

/// The example with every frame but the first turned and moved
/// differently, as a program that reorients each geometry would write it.
fn turned_copy() -> std::path::PathBuf {
    let text = std::fs::read_to_string(reaction_path()).unwrap();
    let mut out = String::new();
    for (frame, block) in text.split_inclusive('\n').collect::<Vec<_>>().chunks(23).enumerate() {
        let turn = glam::Quat::from_rotation_y(0.3 * frame as f32) * glam::Quat::from_rotation_x(0.2 * frame as f32);
        let shift = glam::Vec3::new(frame as f32, -0.5 * frame as f32, 2.0);
        out.push_str(block[0]);
        out.push_str(block[1]);
        for line in &block[2..] {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let p = glam::Vec3::new(fields[1].parse().unwrap(), fields[2].parse().unwrap(), fields[3].parse().unwrap());
            let p = if frame == 0 { p } else { turn * p + shift };
            out.push_str(&format!("{} {:.8} {:.8} {:.8}\n", fields[0], p.x, p.y, p.z));
        }
    }
    let path = scratch_dir("frames-turned").join("turned.xyz");
    std::fs::write(&path, out).unwrap();
    path
}

#[test]
fn frames_turned_in_the_file_are_shown_lined_up() {
    let mut original = app_with(vec![reaction_path()]);
    let mut turned = app_with(vec![turned_copy()]);
    for k in [5.0, 13.0, 21.0] {
        original.state_mut().go_to_frame_position(k);
        turned.state_mut().go_to_frame_position(k);
        let largest = original.state().atom_positions().into_iter().zip(turned.state().atom_positions()).map(|(a, b)| distance(a, b)).fold(0.0, f32::max);
        assert!(largest < 2e-3, "frame {}: lined up, the turned file shows the same atoms (off by {largest} A)", k + 1.0);
    }
    // As in the file: the turned frames as written.
    turned.run_steps(2);
    turned.get_by_label("As in the file").click_accesskit();
    turned.run_steps(2);
    let as_written = turned.state().atom_positions();
    let largest = original.state().atom_positions().into_iter().zip(as_written).map(|(a, b)| distance(a, b)).fold(0.0, f32::max);
    assert!(largest > 5.0, "as in the file, the last frame is turned away ({largest} A)");
    // On three selected atoms: the ester's carbons and carbonyl oxygen
    // (atoms 1, 2, 4) stay put from frame to frame.
    turned.state_mut().select_atoms(&[0, 1, 3]);
    turned.run_steps(2);
    turned.get_by_label("On the selected atoms").click_accesskit();
    turned.run_steps(2);
    assert!(turned.query_by_label("On 3 atoms").is_some());
    let mut previous: Option<Vec<[f32; 3]>> = None;
    for k in 0..22 {
        turned.state_mut().go_to_frame_position(k as f32);
        let now = turned.state().atom_positions();
        if let Some(before) = &previous {
            let moved = [0, 1, 3].iter().map(|&a| distance(before[a], now[a])).fold(0.0, f32::max);
            assert!(moved < 0.2, "frame {}: the three atoms the frames are lined up on hardly move ({moved} A)", k + 1);
        }
        previous = Some(now);
    }
}

#[test]
fn between_two_frames_the_atoms_are_between() {
    let mut harness = app_with(vec![reaction_path()]);
    harness.state_mut().go_to_frame_position(10.0);
    let a = harness.state().atom_positions();
    harness.state_mut().go_to_frame_position(11.0);
    let b = harness.state().atom_positions();
    harness.state_mut().go_to_frame_position(10.5);
    harness.run_steps(2);
    assert!(harness.query_by_label("Between frames 11 and 12 of 22").is_some());
    let middle = harness.state().atom_positions();
    for atom in 0..a.len() {
        let step = distance(a[atom], b[atom]);
        assert!(distance(middle[atom], a[atom]) <= step * 0.75 + 1e-4 && distance(middle[atom], b[atom]) <= step * 0.75 + 1e-4, "atom {} is between frames 11 and 12", atom + 1);
    }
}

#[test]
fn the_frames_play_and_stop() {
    let mut harness = app_with(vec![reaction_path()]);
    harness.run_steps(2);
    set_field(&mut harness, "frames/s", 30.0);
    set_field(&mut harness, "between", 0.0);
    set_field(&mut harness, " s", 0.0);
    harness.get_by_label("\u{25b6} Play").click_accesskit();
    harness.run_steps(20);
    let position = harness.state().frame_position().unwrap();
    assert!(position > 0.0, "playing moved the frames on");
    assert_eq!(position, position.round(), "without geometries between, only frames of the file are shown");
    harness.get_by_label("\u{23f8} Pause").click_accesskit();
    harness.run_steps(2);
    let paused = harness.state().frame_position();
    harness.run_steps(10);
    assert_eq!(harness.state().frame_position(), paused, "paused, the frames stay");
    // Comma and period step a frame back and on.
    let before = paused.unwrap();
    harness.key_press(egui::Key::Period);
    harness.run_steps(2);
    assert_eq!(harness.state().frame_position(), Some((before + 1.0).min(21.0)));
    harness.key_press(egui::Key::Comma);
    harness.key_press(egui::Key::Comma);
    harness.run_steps(2);
    assert_eq!(harness.state().frame_position(), Some(((before + 1.0).min(21.0) - 2.0).max(0.0)));
}

#[test]
fn sessions_keep_the_frame_the_line_up_and_the_player() {
    let session = scratch_dir("frames-session").join("path.a3dview");
    let picture = {
        let mut harness = app_with(vec![reaction_path()]);
        harness.run_steps(2);
        set_field(&mut harness, "frames/s", 3.0);
        harness.state_mut().select_atoms(&[0, 1, 3]);
        harness.run_steps(2);
        harness.get_by_label("On the selected atoms").click_accesskit();
        harness.run_steps(2);
        harness.state_mut().go_to_frame_position(14.0);
        harness.state_mut().select_atoms(&[]);
        harness.run_steps(3);
        harness.state_mut().save_session_to(&session).unwrap();
        harness.state_mut().render_image(320, 240).unwrap()
    };
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("frame = 15.0") && text.contains("line_up = \"atoms\"") && text.replace([' ', '\n'], "").contains("line_up_atoms=[1,2,4,]") && text.contains("speed = 3.0"), "{text}");

    let mut harness = app_with(vec![session]);
    harness.run_steps(3);
    assert_eq!(harness.state().frame_position(), Some(14.0));
    assert!(harness.query_by_label("On 3 atoms").is_some());
    assert!(harness.query_by_label("3.0 frames/s").is_some() || harness.get_all(By::new().role(Role::SpinButton)).any(|n| n.value().is_some_and(|v| v.starts_with("3.0 frames"))));
    assert!(harness.state_mut().render_image(320, 240).unwrap() == picture, "the session reopens pixel-identical");
}

#[test]
fn frames_open_as_structures_of_their_own_and_come_back_in_sessions() {
    let session = scratch_dir("frames-as-structures").join("frames.a3dview");
    {
        let mut harness = app_with(vec![reaction_path()]);
        harness.run_steps(2);
        harness.get_by_label("Open each frame as a structure").click_accesskit();
        harness.run_steps(2);
        let labels = harness.state().structure_labels();
        assert_eq!(labels.len(), 23);
        assert_eq!(labels[1], "reaction-path.xyz (1)");
        assert_eq!(labels[22], "reaction-path.xyz (22)");
        harness.state_mut().save_session_to(&session).unwrap();
    }
    let mut harness = app_with(vec![session]);
    harness.run_steps(3);
    let labels = harness.state().structure_labels();
    assert_eq!(labels.len(), 23, "the path and its 22 frames: {labels:?}");
    assert_eq!(labels[22], "reaction-path.xyz (22)");
}

#[test]
fn different_molecules_in_one_file_open_as_separate_structures() {
    let water = "3\nwater\nO 0 0 0\nH 0.96 0 0\nH -0.24 0.93 0\n";
    let ammonia = "4\nammonia\nN 0 0 0\nH 1.01 0 0\nH -0.34 0.95 0\nH -0.34 -0.48 0.82\n";
    let path = scratch_dir("frames-different").join("two.xyz");
    std::fs::write(&path, format!("{water}{ammonia}")).unwrap();
    let mut harness = app_with(vec![path]);
    harness.run_steps(2);
    assert_eq!(harness.state().structure_labels(), vec!["two.xyz (1)".to_string(), "two.xyz (2)".to_string()]);
    assert_eq!(harness.state().frame_position(), None, "no frames to play");
}

#[test]
fn the_gif_plays_the_frames_as_the_window_does() {
    let mut harness = app_with(vec![reaction_path()]);
    harness.run_steps(2);
    set_field(&mut harness, "frames/s", 10.0);
    set_field(&mut harness, "between", 1.0);
    set_field(&mut harness, " s", 0.5);
    harness.get_by_label("Export").click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_label("Play the frames").is_some() && harness.query_by_label("Turn").is_some());
    set_field(&mut harness, " px", 200.0);
    harness.state_mut().go_to_frame_position(7.0);
    let before = harness.state().atom_positions();
    let path = scratch_dir("frames-gif").join("path.gif");
    let pictures = harness.state_mut().save_frames_gif(&path).unwrap();
    // 22 frames with one geometry between: 43 forward, 41 back.
    assert_eq!(pictures, 84);
    assert_eq!(harness.state().atom_positions(), before, "the frames are left where they were");

    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(decoder.width(), 200);
    let mut delays = Vec::new();
    while let Some(frame) = decoder.read_next_frame().unwrap() {
        delays.push(frame.delay);
    }
    assert_eq!(delays.len(), 84, "one GIF frame per picture");
    // 20 pictures per second (5 cs each); the first and last frames of the
    // file are held half a second longer.
    assert_eq!(delays[1], 5);
    assert_eq!(delays[0], 55);
    assert_eq!(delays[42], 55);
    let total: u32 = delays.iter().map(|&d| d as u32).sum();
    assert_eq!(total, 84 * 5 + 100);

    // Too fast for a GIF: refused with the reason.
    set_field(&mut harness, "frames/s", 30.0);
    set_field(&mut harness, "between", 4.0);
    let refused = harness.state_mut().save_frames_gif(&path).unwrap_err();
    assert!(refused.contains("at most 50 pictures per second"), "{refused}");
}

#[test]
fn energies_on_the_comment_lines_are_plotted() {
    // The example with an energy on each comment line, ORCA's way: a
    // barrier of 25 kcal/mol at frame 12, products 5 kcal/mol below.
    let text = std::fs::read_to_string(reaction_path()).unwrap();
    let mut frame = 0;
    let with_energies: String = text
        .lines()
        .map(|line| {
            if line.trim().starts_with("Energy:") {
                let x = frame as f64 / 21.0;
                let kcal = 25.0 * (-((x - 0.52) / 0.18).powi(2)).exp() - 5.0 * x;
                frame += 1;
                format!("Coordinates from ORCA-job path E {:.8}\n", -286.0 + kcal / 627.509_474_063_1)
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    let path = scratch_dir("frames-energies").join("with-energies.xyz");
    std::fs::write(&path, with_energies).unwrap();
    let mut harness = app_with(vec![path]);
    harness.state_mut().go_to_frame_position(15.0);
    harness.run_steps(3);
    screenshot(&mut harness, "frames-energies");
    // The barrier as plotted: the highest frame's energy (frame 12).
    assert!(harness.query_all(By::new().label_contains("kcal/mol (highest +22.4)")).next().is_some(), "the energy profile's title");
    harness.state_mut().go_to_frame_position(17.5);
    harness.run_steps(3);
    screenshot(&mut harness, "frames-between");
}

fn button<'a>(harness: &'a Harness, label: &str) -> egui_kittest::Node<'a> {
    harness
        .get_all(By::new().role(Role::Button))
        .find(|n| n.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no button {label}"))
}

#[test]
fn kept_views_turn_the_camera_along_the_path() {
    let mut harness = app_with(vec![reaction_path()]);
    harness.run_steps(2);
    harness.get_by_label("Keep this view here").click_accesskit();
    harness.run_steps(2);
    let start = harness.state().camera_state();

    // At the transition state: turned and closer, kept there too.
    harness.state_mut().go_to_frame_position(10.0);
    assert_eq!(harness.state().camera_state(), start, "after the last kept view the camera stays on it");
    harness.state_mut().turn_view(1.2, 0.4);
    harness.state_mut().zoom_view(0.6);
    harness.run_steps(1);
    harness.get_by_label("Keep this view here").click_accesskit();
    harness.run_steps(2);
    let ts = harness.state().camera_state();
    assert_eq!(harness.state().kept_view_frames(), vec![1.0, 11.0]);
    assert!(harness.query_by_label("Replace the view here").is_some());
    assert!(harness.query_by_label("Follow the kept views").is_some());

    // Between them the camera is between them; outside, held.
    harness.state_mut().go_to_frame_position(5.0);
    let mid = harness.state().camera_state();
    assert!(mid.1 < start.1 && mid.1 > ts.1, "zoomed partly: {} between {} and {}", mid.1, start.1, ts.1);
    assert!(mid.2 != start.2 && mid.2 != ts.2, "turned partly");
    harness.state_mut().go_to_frame_position(21.0);
    assert_eq!(harness.state().camera_state(), ts);
    // The player's buttons go along the views too.
    harness.get_by_label("\u{23ee}").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().camera_state(), start);

    // Not followed: the camera stays where it is turned.
    harness.get_by_label("Follow the kept views").click_accesskit();
    harness.run_steps(2);
    harness.state_mut().turn_view(0.3, 0.0);
    let free = harness.state().camera_state();
    harness.state_mut().go_to_frame_position(10.0);
    assert_eq!(harness.state().camera_state(), free);
    // A kept view's button still shows it.
    button(&harness, "11").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().camera_state(), ts);

    // Removed.
    button(&harness, "\u{d7}").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().kept_view_frames(), vec![11.0]);
}

/// The GIF's first picture and its whole bytes.
fn gif_of(harness: &mut Harness, name: &str) -> (Vec<u8>, Vec<u8>) {
    let path = scratch_dir("frames-gif-views").join(name);
    harness.state_mut().save_frames_gif(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes.as_slice()).unwrap();
    let first = decoder.read_next_frame().unwrap().unwrap().buffer.to_vec();
    (first, bytes)
}

#[test]
fn the_gif_follows_the_kept_views_and_spin_waits() {
    let mut harness = app_with(vec![reaction_path()]);
    harness.run_steps(2);
    set_field(&mut harness, "frames/s", 10.0);
    set_field(&mut harness, "between", 0.0);
    harness.get_by_label("Export").click_accesskit();
    harness.run_steps(2);
    set_field(&mut harness, " px", 160.0);
    let (plain_first, plain) = gif_of(&mut harness, "plain.gif");

    harness.get_by_label("Keep this view here").click_accesskit();
    harness.run_steps(2);
    harness.state_mut().go_to_frame_position(21.0);
    harness.state_mut().turn_view(1.5, 0.2);
    harness.run_steps(1);
    harness.get_by_label("Keep this view here").click_accesskit();
    harness.run_steps(2);
    harness.state_mut().go_to_frame_position(0.0);
    let camera = harness.state().camera_state();
    assert!(harness.query_by_label_contains("seen from the kept views").is_some());
    let (views_first, views) = gif_of(&mut harness, "views.gif");
    assert_eq!(harness.state().camera_state(), camera, "the view is left as it was");
    // Both start from the same view (gifski smooths each picture with its
    // neighbors, so the first pictures differ only by that).
    let differing = views_first.chunks(4).zip(plain_first.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 32)).count();
    eprintln!("first pictures: {differing} of {} pixels differ", views_first.len() / 4);
    assert!(differing * 100 < views_first.len() / 4, "both start from the same view ({differing} pixels differ)");
    assert_ne!(views, plain, "the kept views turn the molecule");

    harness.get_by_label("Spin (Space)").click_accesskit();
    harness.run_steps(1);
    assert!(harness.query_by_label_contains("Spin is not used").is_some());
    let (_, spun) = gif_of(&mut harness, "spin.gif");
    assert_eq!(spun, views, "Spin waits while kept views turn the molecule");
}

#[test]
fn kept_views_stay_on_their_frame_lined_up_otherwise_and_in_sessions() {
    let session = scratch_dir("frames-views-session").join("views.a3dview");
    let mut harness = app_with(vec![turned_copy()]);
    harness.run_steps(2);
    harness.get_by_label("Keep this view here").click_accesskit();
    harness.run_steps(2);
    harness.state_mut().go_to_frame_position(12.0);
    harness.state_mut().turn_view(0.8, -0.3);
    harness.run_steps(1);
    harness.get_by_label("Keep this view here").click_accesskit();
    harness.run_steps(2);
    let lined_up = harness.state_mut().render_image(320, 240).unwrap();

    // As in the file, every frame sits elsewhere: the view goes with it.
    harness.get_by_label("As in the file").click_accesskit();
    harness.run_steps(2);
    let as_in_file = harness.state_mut().render_image(320, 240).unwrap();
    let differing = lined_up.chunks(4).zip(as_in_file.chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 16)).count();
    eprintln!("lined up otherwise: {differing} pixels differ");
    assert!(differing * 100 < 320 * 240, "the frame is seen as before ({differing} pixels differ)");

    harness.state_mut().save_session_to(&session).unwrap();
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("[[structures.frames.views]]") && !text.contains("follow_views"), "{text}");
    let mut reopened = app_with(vec![session]);
    reopened.run_steps(3);
    assert_eq!(reopened.state().kept_view_frames(), vec![1.0, 13.0]);
    assert!(reopened.state_mut().render_image(320, 240).unwrap() == as_in_file, "the session reopens pixel-identical");
    reopened.state_mut().go_to_frame_position(0.0);
    harness.state_mut().go_to_frame_position(0.0);
    assert_eq!(reopened.state().camera_state(), harness.state().camera_state(), "the views came back");
}
