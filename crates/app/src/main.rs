// No console window next to the app on Windows (release builds; debug
// builds keep it for messages).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod diagnostics;

fn main() -> eframe::Result<()> {
    // `apost3dview --render session.a3dview out.png [--size WIDTHxHEIGHT]`:
    // the session's figure, rendered without a window.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--render") {
        std::process::exit(render_from_command_line(&args[1..]));
    }
    diagnostics::start();
    let backends = apost3dview::graphics::backends();
    if diagnostics::log_adapters(backends) == 0 {
        diagnostics::show_error(&apost3dview::graphics::no_adapter_message());
        std::process::exit(1);
    }

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("bundled icon.png should be a valid image");

    // Files named on the command line open at startup.
    let startup = apost3dview::StartupOptions {
        files: std::env::args_os().skip(1).map(std::path::PathBuf::from).collect(),
        recent_file: apost3dview::default_recent_file(),
        ..Default::default()
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_icon(icon),
        depth_buffer: apost3dview_render::DEPTH_BUFFER_BITS,
        multisampling: apost3dview_render::MSAA_SAMPLES as u16,
        wgpu_options: wgpu_options(backends),
        ..Default::default()
    };

    let result = eframe::run_native(
        apost3dview::APP_ID,
        native_options,
        Box::new(|cc| {
            // An adapter the renderer can't use (OpenGL) is refused here,
            // with the reason, before any pipeline is built.
            if let Some(problem) = cc.wgpu_render_state.as_ref().and_then(|state| apost3dview::graphics::unsupported_adapter(&state.adapter.get_info())) {
                log::error!("{problem}");
                diagnostics::show_error(&problem);
                std::process::exit(1);
            }
            Ok(Box::new(apost3dview::App::with_options(cc, startup)))
        }),
    );
    if let Err(err) = &result {
        log::error!("could not start: {err}");
        diagnostics::show_error(&format!("APOST-3Dview could not start:\n\n{err}"));
    }
    result
}

fn wgpu_options(backends: wgpu::Backends) -> egui_wgpu::WgpuConfiguration {
    let mut options = egui_wgpu::WgpuConfiguration::default();
    if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_setup {
        setup.instance_descriptor.backends = backends;
    }
    options
}

/// `--render` mode; returns the exit code.
fn render_from_command_line(args: &[std::ffi::OsString]) -> i32 {
    let usage = "usage: apost3dview --render <session.a3dview> <output.png> [--size WIDTHxHEIGHT]";
    let (Some(session), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("{usage}");
        return 2;
    };
    let mut size = apost3dview::DEFAULT_SIZE;
    if let Some(flag) = args.get(2) {
        let parsed = (flag == "--size")
            .then(|| args.get(3)?.to_str()?.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
            .flatten()
            .filter(|[w, h]: &[u32; 2]| *w > 0 && *h > 0);
        match parsed {
            Some(s) => size = s,
            None => {
                eprintln!("{usage}");
                return 2;
            }
        }
    }
    match apost3dview::render_session(std::path::Path::new(session), std::path::Path::new(out), size) {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(err) => {
            eprintln!("apost3dview: {err}");
            1
        }
    }
}
