use std::path::Path;
use std::time::{Duration, Instant};

use apost3dview_core::{format_coordinates, CoordinateFormat, LengthUnit, Molecule};
use apost3dview_render::{Material, OrbitCamera, ViewportCallback, ViewportResources};
use egui::{Color32, Slider};

/// Minimum time the splash screen stays up, regardless of how fast startup
/// actually finishes — startup (parsing the sample .fchk, uploading the
/// molecule) currently takes low-single-digit milliseconds, so in practice
/// this is the entire splash duration today. Once slower loading paths
/// exist (Phase 3's Python bridge), this becomes a floor rather than the
/// whole story.
const SPLASH_MIN_DURATION: Duration = Duration::from_secs(4);

fn load_texture(ctx: &egui::Context, name: &str, png_bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(png_bytes).expect("bundled image should be valid").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

pub struct App {
    camera: OrbitCamera,
    material: Material,
    molecule: Option<Molecule>,
    fchk_filename: String,
    logo_texture: egui::TextureHandle,
    start_time: Instant,

    // Each tool panel is an independent floating window, toggled from the
    // top toolbar — this is the scalable structure: adding a new panel
    // later is one more bool + one more `show_*_window` function, no
    // restructuring of the others.
    show_style: bool,
    show_xyz: bool,
    show_about: bool,

    coordinate_unit: LengthUnit,
    coordinate_format: CoordinateFormat,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("eframe must be running with the wgpu backend");

        let mut resources = ViewportResources::new(&render_state.device, render_state.target_format);
        let mut camera = OrbitCamera::default();

        // Bring-up wiring: load the sample .fchk shipped alongside the repo
        // so there's real geometry to look at and profile against. Real
        // file-open UI is separate future work.
        let fchk_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Bi-dianion-OSD.fchk");
        let fchk_filename = fchk_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut molecule = None;
        match Molecule::from_fchk(&fchk_path) {
            Ok(loaded) => {
                let (center, radius) = loaded.bounding_sphere();
                camera.frame_bounds(center, radius);
                resources.load_molecule(&render_state.device, &loaded);
                molecule = Some(loaded);
            }
            Err(err) => {
                eprintln!("could not load {}: {err}", fchk_path.display());
            }
        }

        render_state.renderer.write().callback_resources.insert(resources);

        let logo_texture = load_texture(&cc.egui_ctx, "apost3d_logo", include_bytes!("../assets/logo.png"));

        Self {
            camera,
            material: Material::default(),
            molecule,
            fchk_filename,
            logo_texture,
            start_time: Instant::now(),
            show_style: true,
            show_xyz: false,
            show_about: false,
            coordinate_unit: LengthUnit::Angstrom,
            coordinate_format: CoordinateFormat::AtomicNumberTable,
        }
    }

    fn show_splash(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);

        let logo_size = self.logo_texture.size_vec2();
        let max_width = (rect.width() * 0.4).min(logo_size.x);
        let scale = max_width / logo_size.x;
        let display_size = logo_size * scale;

        let logo_rect = egui::Rect::from_center_size(rect.center(), display_size);
        ui.painter().image(
            self.logo_texture.id(),
            logo_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("APOST3Dview").strong());
                ui.separator();
                if ui.selectable_label(self.show_style, "Style").clicked() {
                    self.show_style = !self.show_style;
                }
                if ui.selectable_label(self.show_xyz, "XYZ").clicked() {
                    self.show_xyz = !self.show_xyz;
                }
                if ui.selectable_label(self.show_about, "About").clicked() {
                    self.show_about = !self.show_about;
                }
            });
        });
    }

    fn show_style_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_style;
        egui::Window::new("Style")
            .open(&mut open)
            .default_pos([ctx.content_rect().right() - 280.0, 60.0])
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Default").clicked() {
                        self.material = Material::default();
                    }
                    if ui.button("Publication").clicked() {
                        self.material = Material::publication();
                    }
                });

                ui.add_space(8.0);
                ui.label("Geometry");
                ui.add(Slider::new(&mut self.material.atom_scale, 0.1..=1.5).text("atom scale"));
                ui.add(Slider::new(&mut self.material.bond_radius, 0.02..=0.5).text("bond radius"));

                ui.add_space(12.0);
                ui.label("Material");
                ui.add(Slider::new(&mut self.material.ambient, 0.0..=1.0).text("ambient"));
                ui.add(Slider::new(&mut self.material.diffuse, 0.0..=1.0).text("diffuse"));
                ui.add(Slider::new(&mut self.material.specular, 0.0..=1.0).text("specular"));
                ui.add(Slider::new(&mut self.material.shininess, 1.0..=128.0).text("shininess"));

                ui.add_space(12.0);
                ui.label("Lighting");
                ui.add(
                    Slider::new(&mut self.material.light_yaw, -std::f32::consts::PI..=std::f32::consts::PI)
                        .text("light yaw"),
                );
                ui.add(Slider::new(&mut self.material.light_pitch, -1.5..=1.5).text("light pitch"));

                ui.add_space(12.0);
                ui.label("Background");
                let mut background = Color32::from_rgb(
                    (self.material.background[0] * 255.0) as u8,
                    (self.material.background[1] * 255.0) as u8,
                    (self.material.background[2] * 255.0) as u8,
                );
                if ui.color_edit_button_srgba(&mut background).changed() {
                    self.material.background = [
                        background.r() as f32 / 255.0,
                        background.g() as f32 / 255.0,
                        background.b() as f32 / 255.0,
                    ];
                }

                ui.add_space(16.0);
                ui.separator();
                ui.label("Drag to orbit, scroll to zoom, shift-drag to pan.");
            });
        self.show_style = open;
    }

    fn show_xyz_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_xyz;
        egui::Window::new("XYZ")
            .open(&mut open)
            .default_pos([40.0, 60.0])
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Unit:");
                    ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Angstrom, "Ang (Å)");
                    ui.selectable_value(&mut self.coordinate_unit, LengthUnit::Bohr, "Bohr (a.u.)");
                });
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::AtomicNumberTable, "Z x y z");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::XyzFile, "xyz");
                });
                ui.separator();

                match &self.molecule {
                    Some(molecule) => {
                        let text =
                            format_coordinates(molecule, self.coordinate_unit, self.coordinate_format, &self.fchk_filename);
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(text).monospace())
                                    .selectable(true)
                                    .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                    }
                    None => {
                        ui.label("No molecule loaded.");
                    }
                }
            });
        self.show_xyz = open;
    }

    fn show_about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("About APOST3Dview")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let logo_size = self.logo_texture.size_vec2();
                    let display_size = logo_size * (240.0 / logo_size.x);
                    ui.image((self.logo_texture.id(), display_size));

                    ui.add_space(8.0);
                    ui.label(format!("APOST3Dview v{}", env!("CARGO_PKG_VERSION")));
                    ui.label("A molecular visualizer for APOST-3D.");
                    ui.add_space(8.0);
                    ui.label("Martí Gimferrer");
                    ui.hyperlink_to("mgimferrer18@gmail.com", "mailto:mgimferrer18@gmail.com");
                    ui.add_space(8.0);
                    ui.label(
                        "Sister project to APOST-3D, a software to extract state-of-the-art \
                         chemical bonding indicators from wavefunction analysis",
                    );
                });
            });
        self.show_about = open;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.start_time.elapsed() < SPLASH_MIN_DURATION {
            self.show_splash(ui);
            return;
        }

        self.show_toolbar(ui);
        self.show_style_window(ui.ctx());
        self.show_xyz_window(ui.ctx());
        self.show_about_window(ui.ctx());

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let response = ui.interact(
                    rect,
                    ui.id().with("viewport"),
                    egui::Sense::click_and_drag(),
                );

                let drag_delta = response.drag_delta();
                if response.dragged() {
                    if ui.input(|i| i.modifiers.shift) {
                        self.camera.pan(-drag_delta.x * 0.01, drag_delta.y * 0.01);
                    } else {
                        self.camera.orbit(-drag_delta.x * 0.005, -drag_delta.y * 0.005);
                    }
                }
                let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll_delta != 0.0 {
                    self.camera.zoom(scroll_delta * 0.02);
                }

                let background = self.material.background;
                let bg_color = Color32::from_rgb(
                    (background[0] * 255.0) as u8,
                    (background[1] * 255.0) as u8,
                    (background[2] * 255.0) as u8,
                );
                ui.painter().rect_filled(rect, 0.0, bg_color);

                let aspect_ratio = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };
                let callback = ViewportCallback {
                    camera: self.camera,
                    material: self.material,
                    aspect_ratio,
                };
                ui.painter()
                    .add(egui_wgpu::Callback::new_paint_callback(rect, callback));

                if response.dragged() || scroll_delta != 0.0 {
                    ui.ctx().request_repaint();
                }
            });
    }
}
