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

/// Where saved Custom style presets live: a `presets/` folder at the
/// repository root, alongside `TESTS-VISUALIZER/`/`media/` — deliberately
/// not the OS's per-user config directory, so a source-built user can
/// find their own saved presets exactly where they'd look for anything
/// else in this project, right next to the code, rather than in some
/// hard-to-find system folder. Resolved via `CARGO_MANIFEST_DIR` (this
/// crate's own source directory, a path Cargo bakes in at compile time on
/// whoever's machine runs `cargo build`) rather than the process's
/// current working directory, so it resolves to the same place regardless
/// of how the binary happens to be launched (`cargo run` from the repo
/// root vs. double-clicking the built executable directly land in
/// different working directories, but the same `CARGO_MANIFEST_DIR`).
/// This assumes a source build from a live checkout, which is how this
/// app is used today; would need to change (e.g. to the OS's per-user
/// config directory) if a prebuilt distributable binary is ever shipped
/// to people who didn't compile it themselves.
pub(crate) fn presets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../presets")
}

/// Names (without the `.toml` extension) of every saved preset currently
/// on disk, sorted for a stable display order. Returns empty rather than
/// erroring if the folder doesn't exist yet — true the first time this
/// app runs, before anyone has saved a preset.
pub(crate) fn list_style_presets() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(presets_dir()) else { return Vec::new() };
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
        let path = presets_dir().join(format!("{name}.toml"));
        let contents = match std::fs::read_to_string(&path) {
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
        let dir = presets_dir();
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
        if let Err(err) = std::fs::remove_file(presets_dir().join(format!("{name}.toml"))) {
            self.show_warning(format!("Could not delete preset '{name}': {err}"));
            return;
        }
        self.style_presets = list_style_presets();
    }
}
