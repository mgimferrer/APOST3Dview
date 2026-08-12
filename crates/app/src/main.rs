mod app;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        depth_buffer: apost3dview_render::DEPTH_BUFFER_BITS,
        multisampling: apost3dview_render::MSAA_SAMPLES as u16,
        ..Default::default()
    };

    eframe::run_native(
        "APOST3Dview",
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
