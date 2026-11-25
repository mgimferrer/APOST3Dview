//! Features: which optional areas a person shows (only Building, for now).
//! Kept in the settings folder (`features.toml`); asked once per computer
//! at the first start, changed later from About > Features...
//!
//! Tests and the headless renderer keep it in memory with every area on
//! and never ask (`StartupOptions::features_file` is `None`).

use std::path::PathBuf;

use crate::app::App;

/// Where the choice is kept: the settings folder.
pub fn default_features_file() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|dir| dir.join("features.toml"))
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
struct Stored {
    /// The first-start question was answered on this computer.
    #[serde(default)]
    asked: bool,
    /// Building and editing molecules (the Build window).
    #[serde(default)]
    building: bool,
}

pub(crate) struct Features {
    /// The Build window and its toolbar button.
    pub(crate) building: bool,
    /// The Features window is up (first start, or About > Features...).
    pub(crate) window: bool,
    /// It is the first-start question (a welcome), not About's.
    first_start: bool,
    store: Option<PathBuf>,
}

impl Features {
    /// The choice kept at `store` (asking for it when it was never made),
    /// or, without a store, every area on and nothing asked.
    pub(crate) fn load(store: Option<PathBuf>) -> Self {
        let Some(path) = store else { return Self { building: true, window: false, first_start: false, store: None } };
        let stored = std::fs::read_to_string(&path).ok().and_then(|text| toml::from_str::<Stored>(&text).ok());
        match stored {
            Some(s) if s.asked => Self { building: s.building, window: false, first_start: false, store: Some(path) },
            _ => Self { building: false, window: true, first_start: true, store: Some(path) },
        }
    }

    fn save(&self) {
        if let Some(path) = &self.store {
            let text = toml::to_string(&Stored { asked: true, building: self.building }).unwrap_or_default();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, text);
        }
    }
}

impl App {
    /// Turns Building on (a session with edited copies needs it), saying so.
    pub(crate) fn require_building(&mut self) {
        if !self.features.building {
            self.features.building = true;
            self.features.save();
            self.show_status("Building tools shown: this session has edited structures (About > Features... to change).");
        }
    }

    /// The Features window: at the first start, and from About.
    pub(crate) fn show_features_window(&mut self, ctx: &egui::Context) {
        if !self.features.window {
            return;
        }
        let mut done = false;
        let mut building = self.features.building;
        let title = if self.features.first_start { "Welcome to APOST-3Dview" } else { "Features" };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                let question = if self.features.first_start { "What will you use it for? You can change this later in About > Features..." } else { "Which tools to show:" };
                ui.label(question);
                ui.separator();
                ui.label(egui::RichText::new("Always included: molecules, orbitals and densities, APOST-3D results, figures and videos.").weak());
                ui.add_space(4.0);
                ui.checkbox(&mut building, "Building and editing molecules");
                ui.label(egui::RichText::new("Build window: edit geometries, delete atoms, change elements, save as .xyz.").small().weak());
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(if self.features.first_start { "Start" } else { "Done" }).clicked() {
                        done = true;
                    }
                });
            });
        self.features.building = building;
        if done {
            self.features.window = false;
            self.features.first_start = false;
            self.features.save();
            if !self.features.building {
                self.set_window_open(crate::windows::ToolWindow::Build, false);
            }
        }
    }

    /// Opens the Features window (About > Features...).
    pub(crate) fn open_features(&mut self) {
        self.features.window = true;
    }
}
