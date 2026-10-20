// No console window next to the app on Windows (release builds; debug
// builds keep it for messages).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("bundled icon.png should be a valid image");

    // Files named on the command line open at startup.
    let startup = apost3dview::StartupOptions { files: std::env::args_os().skip(1).map(std::path::PathBuf::from).collect(), ..Default::default() };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_icon(icon),
        depth_buffer: apost3dview_render::DEPTH_BUFFER_BITS,
        multisampling: apost3dview_render::MSAA_SAMPLES as u16,
        ..Default::default()
    };

    eframe::run_native(
        apost3dview::APP_ID,
        native_options,
        Box::new(|cc| Ok(Box::new(apost3dview::App::with_options(cc, startup)))),
    )
}
