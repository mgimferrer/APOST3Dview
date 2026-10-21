//! What the executable writes down about its own start, so a problem on a
//! user's computer can be diagnosed: a log file, replaced at every start,
//! in the per-user settings folder (next to `presets/`), and an error
//! dialog when the app panics or cannot start. Release builds on Windows
//! have no console, so without this a failure there is silent.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// `apost3dview.log` in the settings folder (`None` without a home folder).
pub fn log_path() -> Option<PathBuf> {
    eframe::storage_dir(apost3dview::APP_ID).map(|dir| dir.join("apost3dview.log"))
}

struct FileLogger {
    file: Option<Mutex<File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Warnings from everything; the steps of the start from the app,
        // eframe and the graphics layers (which adapter, which backend).
        let target = metadata.target();
        metadata.level() <= log::Level::Warn
            || (metadata.level() <= log::Level::Info
                && ["apost3dview", "eframe", "egui_wgpu", "wgpu_hal", "wgpu_core"].iter().any(|prefix| target.starts_with(prefix)))
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{:>5} {}: {}\n", record.level(), record.target(), record.args());
        eprint!("{line}");
        if let Some(file) = &self.file
            && let Ok(mut file) = file.lock()
        {
            // Flushed line by line: if the app hangs, the log still shows
            // the last step it reached.
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    fn flush(&self) {}
}

/// Starts the log and the panic dialog. Call first thing in `main`.
pub fn start() {
    let file = log_path().and_then(|path| {
        std::fs::create_dir_all(path.parent()?).ok()?;
        File::create(&path).ok()
    });
    let logger = Box::leak(Box::new(FileLogger { file: file.map(Mutex::new) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    log::info!("APOST3Dview {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("{info}");
        show_error(&format!("APOST3Dview stopped because of an internal error:\n\n{info}"));
        default_hook(info);
    }));
}

/// The graphics adapters this computer offers, written to the log before
/// the window is created (which one eframe then picks is logged by the app).
pub fn log_adapters(backends: wgpu::Backends) {
    log::info!("graphics APIs tried: {backends:?}");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..wgpu::InstanceDescriptor::new_without_display_handle_from_env() });
    let adapters = pollster::block_on(instance.enumerate_adapters(backends));
    if adapters.is_empty() {
        log::warn!("no graphics adapter found");
    }
    for adapter in adapters {
        let info = adapter.get_info();
        log::info!("adapter {} ({:?}, {:?}, driver {} {})", info.name, info.backend, info.device_type, info.driver, info.driver_info);
    }
}

/// An error dialog, with where the log is.
pub fn show_error(message: &str) {
    let log = log_path().map(|path| format!("\n\nDetails are in {}", path.display())).unwrap_or_default();
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("APOST3Dview")
        .set_description(format!("{message}{log}"))
        .show();
}
