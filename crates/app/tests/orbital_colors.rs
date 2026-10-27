//! Appearance > Orbital colors (2026-10-27): the pair chosen colors the
//! orbitals and spin densities drawn from then on; "Apply to drawn
//! surfaces" recolors the ones already there, except a total density; the
//! choice is saved in sessions, and an older session reopens in the
//! default pair.

mod common;

use egui::accesskit::Role;
use egui_kittest::kittest::{By, Queryable};

use common::{app_with, repo_dir, scratch_dir};

type Harness = egui_kittest::Harness<'static, apost3dview::App>;

const AZURE: [u8; 3] = [20, 95, 170];
const CRIMSON: [u8; 3] = [175, 25, 60];
const BLUE: [u8; 3] = [0, 114, 178];
const AMBER: [u8; 3] = [220, 140, 30];
const TEAL: [u8; 3] = [0, 120, 120];
const DENSITY_BLUE: [u8; 3] = [120, 160, 210];

/// The Orbital colors menu's current choice.
fn chosen(harness: &Harness) -> String {
    let names = ["Blue and red", "Blue and amber", "Teal and amber"];
    harness
        .get_all(By::new().role(Role::ComboBox))
        .filter_map(|n| n.value())
        .find(|v| names.contains(&v.as_str()))
        .expect("the Orbital colors menu")
}

/// Picks `option` in the Orbital colors menu.
fn choose(harness: &mut Harness, option: &str) {
    let current = chosen(harness);
    harness.get_all(By::new().role(Role::ComboBox)).find(|n| n.value().as_deref() == Some(current.as_str())).unwrap().click_accesskit();
    harness.run_steps(2);
    harness.query_all_by_label(option).last().unwrap_or_else(|| panic!("no \"{option}\" in the menu")).click_accesskit();
    harness.run_steps(2);
    // A menu picked through accessibility stays open.
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert_eq!(chosen(harness), option);
}

#[test]
fn the_chosen_pair_colors_new_surfaces_and_is_saved() {
    let session = scratch_dir("orbital-colors").join("water.a3dview");
    {
        let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-631G.fchk")]);
        harness.get_by_label("Surfaces").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("HOMO/LUMO").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Generate 2 orbital(s)").click_accesskit();
        harness.run_steps(3);
        assert_eq!(harness.state().surface_colors(), vec![(AZURE, CRIMSON); 2]);

        // A new pair: the drawn surfaces keep theirs, new ones take it.
        harness.get_by_label("Appearance").click_accesskit();
        harness.run_steps(2);
        assert_eq!(chosen(&harness), "Blue and red");
        choose(&mut harness, "Blue and amber");
        assert_eq!(harness.state().surface_colors(), vec![(AZURE, CRIMSON); 2]);
        harness.get_by_label("Clear").click_accesskit();
        harness.get_all(By::new().role(Role::CheckBox).label_contains("Hartree")).next().expect("MO 1").click_accesskit();
        harness.run_steps(2);
        harness.get_by_label("Generate 1 orbital(s)").click_accesskit();
        harness.run_steps(3);
        assert_eq!(harness.state().surface_colors(), vec![(AZURE, CRIMSON), (AZURE, CRIMSON), (BLUE, AMBER)]);

        harness.get_by_label("Apply to drawn surfaces").click_accesskit();
        harness.run_steps(3);
        assert_eq!(harness.state().surface_colors(), vec![(BLUE, AMBER); 3]);
        assert!(harness.state().toast_texts().iter().any(|t| t == "2 surfaces recolored"), "{:?}", harness.state().toast_texts());
        harness.state_mut().save_session_to(&session).unwrap();
    }
    let text = std::fs::read_to_string(&session).unwrap();
    assert!(text.contains("orbital_colors = \"blue-amber\""), "{text}");

    let mut harness = app_with(vec![session.clone()]);
    harness.run_steps(3);
    harness.get_by_label("Appearance").click_accesskit();
    harness.run_steps(2);
    assert_eq!(chosen(&harness), "Blue and amber");
    assert_eq!(harness.state().surface_colors(), vec![(BLUE, AMBER); 3]);

    // A session saved before the choice existed: the default pair, and its
    // surfaces in their own saved colors.
    let older = session.with_file_name("older.a3dview");
    std::fs::write(&older, text.lines().filter(|l| !l.starts_with("orbital_colors")).collect::<Vec<_>>().join("\n")).unwrap();
    let mut harness = app_with(vec![older]);
    harness.run_steps(3);
    harness.get_by_label("Appearance").click_accesskit();
    harness.run_steps(2);
    assert_eq!(chosen(&harness), "Blue and red");
    assert_eq!(harness.state().surface_colors(), vec![(BLUE, AMBER); 3]);
}

#[test]
fn the_total_density_keeps_its_own_blue() {
    let mut harness = app_with(vec![repo_dir().join("validation/gaussian/H2O-T-ccpVTZ.fchk")]);
    harness.get_by_label("Surfaces").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Total density").click_accesskit();
    harness.get_by_label("Spin density").click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Generate 2 surface(s)").click_accesskit();
    harness.run_steps(4);
    assert_eq!(harness.state().surface_colors(), vec![(DENSITY_BLUE, CRIMSON), (AZURE, CRIMSON)]);

    harness.get_by_label("Appearance").click_accesskit();
    harness.run_steps(2);
    choose(&mut harness, "Teal and amber");
    harness.get_by_label("Apply to drawn surfaces").click_accesskit();
    harness.run_steps(3);
    assert_eq!(harness.state().surface_colors(), vec![(DENSITY_BLUE, CRIMSON), (TEAL, AMBER)]);
    assert!(harness.state().toast_texts().iter().any(|t| t == "1 surface recolored"), "{:?}", harness.state().toast_texts());
}
