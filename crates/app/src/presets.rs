//! Saved looks of your own (Appearance window).

use std::path::PathBuf;

use apost3dview_render::{AoSettings, DofSettings, IsosurfaceMaterial, Material};

use crate::app::App;

/// A saved "look": everything the Default/Publication buttons set
/// together, bundled so a user's own preferred combination can be named
/// and reused. Serialized as a plain TOML file per preset, one of at most
/// a handful expected in practice, not a generic settings-persistence
/// mechanism — Default and Publication stay hardcoded reference points,
/// not files, on purpose.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct StylePreset {
    pub(crate) material: Material,
    pub(crate) ao_settings: AoSettings,
    pub(crate) dof_settings: DofSettings,
    pub(crate) isosurface_material: IsosurfaceMaterial,
}

/// Where saved Custom style presets live: a `presets` folder in the
/// per-user folder where eframe already keeps the app's own state
/// (`~/Library/Application Support/APOST3Dview` on macOS,
/// `%APPDATA%\APOST3Dview\data` on Windows, `~/.local/share/apost3dview`
/// on Linux). The same for a downloaded build and one built from source;
/// the Appearance window's "Open presets folder" shows it, since these
/// folders are hard to find by hand. Was the repository's `presets/`
/// (through `CARGO_MANIFEST_DIR`, fixed at compile time) until downloadable
/// builds existed. `None` only on a system with no home folder.
pub(crate) fn presets_dir() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|dir| dir.join("presets"))
}

/// Names (without the `.toml` extension) of every saved preset currently
/// on disk, sorted for a stable display order. Returns empty rather than
/// erroring if the folder doesn't exist yet — true the first time this
/// app runs, before anyone has saved a preset.
pub(crate) fn list_style_presets() -> Vec<String> {
    let Some(Ok(entries)) = presets_dir().map(std::fs::read_dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|e| e.to_str()) == Some("toml"))
                .then(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_string))
                .flatten()
        })
        .collect();
    names.sort();
    names
}

impl App {
    /// Loads a saved Custom preset by name and applies it, the same shape
    /// as the Default/Publication buttons (reset first, so a preset saved
    /// before some other field existed still ends up with a sane value
    /// for it, then overwrite with whatever the preset actually specifies).
    pub(crate) fn apply_style_preset(&mut self, name: &str) {
        let Some(dir) = presets_dir() else { return };
        let contents = match std::fs::read_to_string(dir.join(format!("{name}.toml"))) {
            Ok(contents) => contents,
            Err(err) => {
                self.show_warning(format!("Could not read preset '{name}': {err}"));
                return;
            }
        };
        let preset: StylePreset = match toml::from_str(&contents) {
            Ok(preset) => preset,
            Err(err) => {
                self.show_warning(format!("Could not parse preset '{name}': {err}"));
                return;
            }
        };
        self.material = preset.material;
        self.ao_settings = preset.ao_settings;
        self.dof_settings = preset.dof_settings;
        self.isosurface_material = preset.isosurface_material;
        self.rebuild_isosurface();
    }

    /// Saves the current material/AO/depth-of-field/isosurface-material
    /// settings as a new named Custom preset, overwriting one of the same
    /// name if it already exists.
    pub(crate) fn save_style_preset(&mut self, name: &str) {
        let preset = StylePreset { material: self.material, ao_settings: self.ao_settings, dof_settings: self.dof_settings, isosurface_material: self.isosurface_material };
        let contents = match toml::to_string_pretty(&preset) {
            Ok(contents) => contents,
            Err(err) => {
                self.show_warning(format!("Could not save preset: {err}"));
                return;
            }
        };
        let Some(dir) = presets_dir() else {
            self.show_warning("Could not save preset: no folder for settings on this system");
            return;
        };
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.show_warning(format!("Could not create presets folder: {err}"));
            return;
        }
        if let Err(err) = std::fs::write(dir.join(format!("{name}.toml")), contents) {
            self.show_warning(format!("Could not save preset '{name}': {err}"));
            return;
        }
        self.style_presets = list_style_presets();
    }

    pub(crate) fn delete_style_preset(&mut self, name: &str) {
        let Some(dir) = presets_dir() else { return };
        if let Err(err) = std::fs::remove_file(dir.join(format!("{name}.toml"))) {
            self.show_warning(format!("Could not delete preset '{name}': {err}"));
            return;
        }
        self.style_presets = list_style_presets();
    }

    /// Shows the presets folder in Finder, Explorer or the desktop's file
    /// manager, creating it first so there is something to open.
    pub(crate) fn open_presets_folder(&mut self) {
        let Some(dir) = presets_dir() else {
            self.show_warning("No folder for settings on this system");
            return;
        };
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        if let Err(err) = std::fs::create_dir_all(&dir).and_then(|()| std::process::Command::new(opener).arg(&dir).spawn().map(drop)) {
            self.show_warning(format!("Could not open {}: {err}", dir.display()));
        }
    }
}
