//! Open > Recent: files and sessions opened or saved, newest first, kept
//! across restarts, opened from the menu; a file that is gone is reported
//! and taken off the list.

mod common;

use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use apost3dview::{App, StartupOptions};
use common::{repo_dir, scratch_dir, toasts};

/// The app with its recent list kept in `store` (as the real application
/// keeps it in the settings folder), opening `files`.
fn app_keeping(store: &Path, files: Vec<PathBuf>) -> Harness<'static, App> {
    let store = store.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .renderer(common::AppRenderer::new())
        .build_eframe(move |cc| App::with_options(cc, StartupOptions { files, splash: false, recent_file: Some(store) }));
    harness.run_steps(4);
    harness
}

#[test]
fn recent_files_are_kept_and_opened_from_the_menu() {
    let dir = scratch_dir("recent");
    let store = dir.join("settings/recent.txt");
    let water = dir.join("water.fchk");
    let bicl3 = dir.join("bicl3.fchk");
    std::fs::copy(repo_dir().join("validation/gaussian/H2O-631G.fchk"), &water).unwrap();
    std::fs::copy(repo_dir().join("validation/gaussian/BiCl3-def2TZVPP.fchk"), &bicl3).unwrap();

    // Two files opened, then a session saved: newest first.
    let first = app_keeping(&store, vec![water.clone(), bicl3.clone()]);
    let session = dir.join("both.a3dview");
    first.state().save_session_to(&session).unwrap();
    let names = |h: &Harness<'static, App>| h.state().recent_files().iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect::<Vec<_>>();
    assert_eq!(names(&first), vec!["bicl3.fchk", "water.fchk"]);
    drop(first);

    // A restart keeps the list; opening a session puts it first.
    let mut second = app_keeping(&store, vec![session.clone()]);
    assert_eq!(names(&second), vec!["both.a3dview", "bicl3.fchk", "water.fchk"]);

    // The menu lists them: Open > Recent.
    second.get_by_label("Open").click();
    second.run_steps(2);
    second.get_by_label("Recent \u{23f5}").click();
    second.run_steps(2);
    for name in ["both.a3dview", "bicl3.fchk", "water.fchk"] {
        assert!(second.query_all_by_label(name).count() >= 1, "{name} in the menu");
    }
    second.get_by_label("Clear recent files");
    second.key_press(egui::Key::Escape);
    second.run_steps(2);

    // Picking one opens it (what the menu entry does) and moves it first.
    let water_entry = second.state().recent_files().into_iter().find(|p| p.ends_with("water.fchk")).unwrap();
    second.state_mut().open_recent(&water_entry);
    second.run_steps(4);
    assert_eq!(names(&second)[0], "water.fchk");

    // A file that is gone: said so, and removed.
    std::fs::remove_file(&bicl3).unwrap();
    let bicl3_entry = second.state().recent_files().into_iter().find(|p| p.ends_with("bicl3.fchk")).unwrap();
    second.state_mut().open_recent(&bicl3_entry);
    second.run_steps(3);
    assert!(toasts(&second).iter().any(|t| t.contains("is no longer there")), "{:?}", toasts(&second));
    assert!(!names(&second).contains(&"bicl3.fchk".to_string()));

    // Clear recent files empties the list on disk too.
    second.get_by_label("Open").click();
    second.run_steps(2);
    second.get_by_label("Recent \u{23f5}").click();
    second.run_steps(2);
    second.get_by_label("Clear recent files").click_accesskit();
    second.run_steps(2);
    assert!(second.state().recent_files().is_empty());
    assert_eq!(std::fs::read_to_string(&store).unwrap(), "");
}


