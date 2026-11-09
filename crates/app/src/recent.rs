//! Open > Recent: the files and sessions opened or saved last, newest
//! first, kept in the per-user settings folder (`recent.txt`, one path per
//! line) so they survive a restart. Only the real application keeps the
//! list on disk (`StartupOptions::recent_file`); tests and the headless
//! renderer keep it in memory.

use std::path::{Path, PathBuf};

/// How many entries the menu keeps.
pub(crate) const RECENT_LIMIT: usize = 10;

/// The default place of the list: the settings folder, next to the saved
/// looks.
pub fn default_recent_file() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|dir| dir.join("recent.txt"))
}

#[derive(Debug, Default)]
pub(crate) struct RecentFiles {
    pub(crate) paths: Vec<PathBuf>,
    /// Where the list is kept; `None`: in memory only.
    store: Option<PathBuf>,
}

impl RecentFiles {
    /// The list kept at `store`, or an empty one kept in memory.
    pub(crate) fn load(store: Option<PathBuf>) -> Self {
        let paths = store
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).take(RECENT_LIMIT).collect())
            .unwrap_or_default();
        Self { paths, store }
    }

    /// `path` first; an earlier entry for the same file is removed, and the
    /// oldest beyond the limit dropped.
    pub(crate) fn add(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(RECENT_LIMIT);
        self.save();
    }

    pub(crate) fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.save();
    }

    pub(crate) fn clear(&mut self) {
        self.paths.clear();
        self.save();
    }

    /// Written at once, so a crash doesn't lose it; a failure to write
    /// (read-only settings folder) only means the list isn't kept.
    fn save(&self) {
        let Some(store) = &self.store else { return };
        if let Some(dir) = store.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self.paths.iter().map(|p| format!("{}\n", p.display())).collect();
        let _ = std::fs::write(store, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_repeats_and_limited() {
        let dir = std::env::temp_dir().join(format!("apost3dview-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("recent.txt");
        let mut recent = RecentFiles::load(Some(store.clone()));
        assert!(recent.paths.is_empty());
        let files: Vec<PathBuf> = (0..12).map(|i| dir.join(format!("f{i}.xyz"))).collect();
        for f in &files {
            std::fs::write(f, "").unwrap();
            recent.add(f);
        }
        recent.add(&files[5]);
        assert_eq!(recent.paths.len(), RECENT_LIMIT);
        assert!(recent.paths[0].ends_with("f5.xyz"), "{:?}", recent.paths);
        assert_eq!(recent.paths.iter().filter(|p| p.ends_with("f5.xyz")).count(), 1);
        assert!(!recent.paths.iter().any(|p| p.ends_with("f0.xyz")), "the oldest dropped");
        // Read back from the file.
        let again = RecentFiles::load(Some(store.clone()));
        assert_eq!(again.paths, recent.paths);
        recent.clear();
        assert!(RecentFiles::load(Some(store)).paths.is_empty());
    }
}
