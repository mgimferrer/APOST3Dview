use apost3dview_render::{Material, OrbitCamera, ViewportCallback, ViewportResources};
use egui::{Color32, Slider};

pub struct App {
    camera: OrbitCamera,
    material: Material,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("eframe must be running with the wgpu backend");

        let resources = ViewportResources::new(&render_state.device, render_state.target_format);
        render_state.renderer.write().callback_resources.insert(resources);

        Self { camera: OrbitCamera::default(), material: Material::default() }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::right("style_panel")
            .default_size(240.0)
            .show(ui, |ui| {
                ui.heading("Style");
                ui.add_space(8.0);

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
                ui.add(
                    Slider::new(&mut self.material.light_pitch, -1.5..=1.5).text("light pitch"),
                );

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
                ui.label("Phase 1: geometry only.");
                ui.label("Drag to orbit, scroll to zoom, shift-drag to pan.");
            });

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
