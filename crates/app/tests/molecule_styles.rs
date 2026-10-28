//! Molecule window > Style (2026-10-28): ball and stick, sticks, thin
//! sticks and space-filling, one style for everything shown. Switching
//! back gives the same picture, the style and its sizes are saved in
//! sessions, and a session saved before reopens in ball and stick.

mod common;

use egui::accesskit::Role;
use egui_kittest::kittest::{By, NodeT, Queryable};

use common::{app_with, repo_dir, scratch_dir};

type Harness = egui_kittest::Harness<'static, apost3dview::App>;

fn picture(harness: &mut Harness) -> Vec<u8> {
    harness.run_steps(2);
    harness.state_mut().render_image(320, 240).unwrap()
}

/// Whether a slider named `name` is shown.
fn slider(harness: &Harness, name: &str) -> bool {
    harness.get_all(By::new().role(Role::Slider)).any(|n| n.accesskit_node().label().as_deref() == Some(name))
}

fn choose(harness: &mut Harness, style: &str) {
    harness.get_by_label(style).click_accesskit();
    harness.run_steps(2);
}

fn show_molecule_window(harness: &mut Harness) {
    if harness.query_by_label("Style:").is_none() {
        harness.get_by_label("Molecule").click_accesskit();
        harness.run_steps(2);
    }
    assert!(harness.query_by_label("Style:").is_some(), "the Molecule window shows the styles");
}

#[test]
fn styles_switch_and_come_back_exactly() {
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
    show_molecule_window(&mut harness);
    let ball_and_stick = picture(&mut harness);
    assert!(slider(&harness, "atom size") && slider(&harness, "bond radius"));

    let mut pictures = vec![ball_and_stick.clone()];
    for style in ["Sticks", "Thin sticks", "Space-filling"] {
        choose(&mut harness, style);
        let shown = picture(&mut harness);
        assert!(pictures.iter().all(|p| *p != shown), "{style} looks like another style");
        pictures.push(shown);
    }
    // Each style shows its own sizes.
    assert!(slider(&harness, "atom size") && !slider(&harness, "bond radius"));
    choose(&mut harness, "Sticks");
    assert!(slider(&harness, "stick radius") && !slider(&harness, "atom size"));

    choose(&mut harness, "Ball and stick");
    assert!(picture(&mut harness) == ball_and_stick, "back to ball and stick, the same picture");
}

#[test]
fn the_style_is_saved_in_sessions() {
    let session = scratch_dir("molecule-styles").join("water.a3dview");
    let (ball_and_stick, sticks) = {
        let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
        show_molecule_window(&mut harness);
        let ball_and_stick = picture(&mut harness);
        choose(&mut harness, "Sticks");
        let sticks = picture(&mut harness);
        harness.state_mut().save_session_to(&session).unwrap();
        (ball_and_stick, sticks)
    };
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("[appearance.molecule_style]") && text.contains("style = \"sticks\""), "{text}");

    let mut harness = app_with(vec![session.clone()]);
    harness.run_steps(3);
    assert!(picture(&mut harness) == sticks, "the session reopens in sticks, pixel-identical");

    // Saved before the styles existed: ball and stick.
    let older = session.with_file_name("older.a3dview");
    let mut skipping = false;
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| {
            if l.starts_with('[') {
                skipping = *l == "[appearance.molecule_style]";
            }
            !skipping
        })
        .collect();
    std::fs::write(&older, kept.join("\n")).unwrap();
    let mut harness = app_with(vec![older]);
    harness.run_steps(3);
    assert!(picture(&mut harness) == ball_and_stick, "an older session reopens in ball and stick");
}
