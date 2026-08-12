use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use apost3dview_core::{element_data, format_coordinates, CoordinateFormat, LengthUnit, Molecule};
use apost3dview_render::{
    pick_atom, pick_bond, ray_from_ndc, BondVisualStyle, Material, OrbitCamera, ViewportCallback, ViewportResources,
};
use egui::{Color32, Slider};

/// Minimum time the splash screen stays up, regardless of how fast startup
/// actually finishes — startup (parsing the sample .fchk, uploading the
/// molecule) currently takes low-single-digit milliseconds, so in practice
/// this is the entire splash duration today. Once slower loading paths
/// exist (Phase 3's Python bridge), this becomes a floor rather than the
/// whole story.
const SPLASH_MIN_DURATION: Duration = Duration::from_secs(4);

const WARNING_DURATION: Duration = Duration::from_secs(3);

fn load_texture(ctx: &egui::Context, name: &str, png_bytes: &[u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(png_bytes).expect("bundled image should be valid").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

fn toggle_selected(list: &mut Vec<usize>, index: usize) {
    if let Some(pos) = list.iter().position(|&i| i == index) {
        list.remove(pos);
    } else {
        list.push(index);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionMode {
    None,
    Atoms,
    Bonds,
}

pub struct App {
    camera: OrbitCamera,
    material: Material,
    molecule: Option<Molecule>,
    fchk_filename: String,
    logo_texture: egui::TextureHandle,
    start_time: Instant,
    render_state: egui_wgpu::RenderState,

    // Each tool panel is an independent floating window, toggled from the
    // top toolbar — this is the scalable structure: adding a new panel
    // later is one more bool + one more `show_*_window` function, no
    // restructuring of the others.
    show_style: bool,
    show_xyz: bool,
    show_visualization: bool,
    show_about: bool,

    coordinate_unit: LengthUnit,
    coordinate_format: CoordinateFormat,

    selection_mode: SelectionMode,
    selected_atoms: Vec<usize>,
    selected_bonds: Vec<usize>,
    hidden_atoms: HashSet<usize>,
    hidden_bonds: HashSet<usize>,
    bond_styles: Vec<BondVisualStyle>,
    warning: Option<(String, Instant)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("eframe must be running with the wgpu backend");

        let mut resources = ViewportResources::new(&render_state.device, render_state.target_format);
        let mut camera = OrbitCamera::default();

        // Bring-up wiring: load the sample .fchk shipped alongside the repo
        // so there's real geometry to look at and profile against. Real
        // file-open UI is separate future work.
        let fchk_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Bi-dianion-OSD.fchk");
        let fchk_filename = fchk_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut molecule = None;
        let mut bond_styles = Vec::new();
        match Molecule::from_fchk(&fchk_path) {
            Ok(loaded) => {
                let (center, radius) = loaded.bounding_sphere();
                camera.frame_bounds(center, radius);
                resources.load_molecule(&render_state.device, &loaded);
                bond_styles = vec![BondVisualStyle::Single; loaded.bonds.len()];
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
            render_state,
            show_style: true,
            show_xyz: false,
            show_visualization: false,
            show_about: false,
            coordinate_unit: LengthUnit::Angstrom,
            coordinate_format: CoordinateFormat::AtomicNumberTable,
            selection_mode: SelectionMode::None,
            selected_atoms: Vec::new(),
            selected_bonds: Vec::new(),
            hidden_atoms: HashSet::new(),
            hidden_bonds: HashSet::new(),
            bond_styles,
            warning: None,
        }
    }

    fn rebuild_geometry(&self) {
        let Some(molecule) = &self.molecule else { return };
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_geometry(&self.render_state.device, molecule, &self.hidden_atoms, &self.hidden_bonds, &self.bond_styles);
        }
    }

    fn rebuild_highlights(&self) {
        let Some(molecule) = &self.molecule else { return };
        let mut renderer = self.render_state.renderer.write();
        if let Some(resources) = renderer.callback_resources.get_mut::<ViewportResources>() {
            resources.update_highlights(&self.render_state.device, molecule, &self.selected_atoms, &self.selected_bonds);
        }
    }

    fn show_warning(&mut self, message: impl Into<String>) {
        self.warning = Some((message.into(), Instant::now()));
    }

    fn clear_selection(&mut self) {
        self.selected_atoms.clear();
        self.selected_bonds.clear();
        self.rebuild_highlights();
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
                if ui.selectable_label(self.show_visualization, "Visualization").clicked() {
                    self.show_visualization = !self.show_visualization;
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
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::AtomicNumberTable, "Standard");
                    ui.selectable_value(&mut self.coordinate_format, CoordinateFormat::XyzFile, "Symbol XYZ");
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

    fn show_visualization_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_visualization;
        egui::Window::new("Visualization")
            .open(&mut open)
            .default_pos([40.0, 400.0])
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.label("Selection mode");
                ui.horizontal(|ui| {
                    let changed_from_atoms = self.selection_mode == SelectionMode::Atoms;
                    let changed_from_bonds = self.selection_mode == SelectionMode::Bonds;
                    ui.selectable_value(&mut self.selection_mode, SelectionMode::None, "Off");
                    ui.selectable_value(&mut self.selection_mode, SelectionMode::Atoms, "Atoms");
                    ui.selectable_value(&mut self.selection_mode, SelectionMode::Bonds, "Bonds");
                    // Switching pick target clears the other kind's
                    // selection so highlight/state can't get stale.
                    if changed_from_atoms && self.selection_mode != SelectionMode::Atoms && !self.selected_atoms.is_empty() {
                        self.selected_atoms.clear();
                        self.rebuild_highlights();
                    }
                    if changed_from_bonds && self.selection_mode != SelectionMode::Bonds && !self.selected_bonds.is_empty() {
                        self.selected_bonds.clear();
                        self.rebuild_highlights();
                    }
                });
                if self.selection_mode != SelectionMode::None {
                    ui.label(
                        egui::RichText::new("Click atoms/bonds in the viewport to select them.")
                            .small()
                            .italics(),
                    );
                }

                ui.add_space(10.0);
                ui.separator();

                if let Some(molecule) = &self.molecule {
                    match self.selection_mode {
                        SelectionMode::Atoms => {
                            let summary = self
                                .selected_atoms
                                .iter()
                                .map(|&i| format!("{}{}", element_data(molecule.atomic_numbers[i]).symbol, i + 1))
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(format!("Selected atoms: {}", self.selected_atoms.len()));
                            if !summary.is_empty() {
                                ui.label(egui::RichText::new(summary).small());
                            }
                            ui.horizontal(|ui| {
                                if ui.button("Hide atoms").clicked() {
                                    if self.selected_atoms.is_empty() {
                                        self.show_warning("No atoms selected");
                                    } else {
                                        self.hidden_atoms.extend(self.selected_atoms.drain(..));
                                        self.rebuild_geometry();
                                        self.rebuild_highlights();
                                    }
                                }
                                if ui.button("Clear selection").clicked() {
                                    self.clear_selection();
                                }
                            });
                        }
                        SelectionMode::Bonds => {
                            ui.label(format!("Selected bonds: {}", self.selected_bonds.len()));
                            ui.horizontal(|ui| {
                                if ui.button("Single").clicked() {
                                    self.apply_bond_style(BondVisualStyle::Single);
                                }
                                if ui.button("TS").clicked() {
                                    self.apply_bond_style(BondVisualStyle::TransitionState);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Hide bonds").clicked() {
                                    if self.selected_bonds.is_empty() {
                                        self.show_warning("No bonds selected");
                                    } else {
                                        self.hidden_bonds.extend(self.selected_bonds.drain(..));
                                        self.rebuild_geometry();
                                        self.rebuild_highlights();
                                    }
                                }
                                if ui.button("Clear selection").clicked() {
                                    self.clear_selection();
                                }
                            });
                        }
                        SelectionMode::None => {
                            ui.label(egui::RichText::new("Turn on Atoms or Bonds mode to select.").weak());
                        }
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label("Global");
                ui.horizontal(|ui| {
                    if ui.button("Hide H-atoms").clicked() {
                        if let Some(molecule) = &self.molecule {
                            self.hidden_atoms.extend(
                                molecule.atomic_numbers.iter().enumerate().filter(|&(_, &z)| z == 1).map(|(i, _)| i),
                            );
                        }
                        self.rebuild_geometry();
                        self.rebuild_highlights();
                    }
                    if ui.button("Show all").clicked() {
                        self.hidden_atoms.clear();
                        self.hidden_bonds.clear();
                        self.bond_styles.iter_mut().for_each(|s| *s = BondVisualStyle::Single);
                        self.selected_atoms.clear();
                        self.selected_bonds.clear();
                        self.rebuild_geometry();
                        self.rebuild_highlights();
                    }
                });
            });
        self.show_visualization = open;
    }

    fn apply_bond_style(&mut self, style: BondVisualStyle) {
        if self.selected_bonds.is_empty() {
            self.show_warning("No bonds selected");
            return;
        }
        for &index in &self.selected_bonds {
            if let Some(entry) = self.bond_styles.get_mut(index) {
                *entry = style;
            }
        }
        self.rebuild_geometry();
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

    fn show_warning_overlay(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let Some((message, shown_at)) = &self.warning else { return };
        if shown_at.elapsed() > WARNING_DURATION {
            self.warning = None;
            return;
        }

        egui::Area::new(egui::Id::new("warning_toast"))
            .fixed_pos(egui::pos2(rect.center().x - 90.0, rect.top() + 20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::from_rgb(196, 60, 40)).show(ui, |ui| {
                    ui.label(egui::RichText::new(message).color(Color32::WHITE).strong());
                });
            });

        ctx.request_repaint_after(Duration::from_millis(100));
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
        self.show_visualization_window(ui.ctx());
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

                let aspect_ratio = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };

                if self.selection_mode != SelectionMode::None && response.clicked() {
                    if let (Some(pos), Some(molecule)) = (response.interact_pointer_pos(), &self.molecule) {
                        let ndc_x = ((pos.x - rect.left()) / rect.width()) * 2.0 - 1.0;
                        let ndc_y = 1.0 - ((pos.y - rect.top()) / rect.height()) * 2.0;
                        let (origin, direction) = ray_from_ndc(&self.camera, aspect_ratio, ndc_x, ndc_y);

                        match self.selection_mode {
                            SelectionMode::Atoms => {
                                if let Some(index) = pick_atom(molecule, origin, direction, self.material.atom_scale, &self.hidden_atoms) {
                                    toggle_selected(&mut self.selected_atoms, index);
                                    self.rebuild_highlights();
                                }
                            }
                            SelectionMode::Bonds => {
                                if let Some(index) = pick_bond(
                                    molecule,
                                    origin,
                                    direction,
                                    self.material.bond_radius,
                                    &self.hidden_atoms,
                                    &self.hidden_bonds,
                                ) {
                                    toggle_selected(&mut self.selected_bonds, index);
                                    self.rebuild_highlights();
                                }
                            }
                            SelectionMode::None => {}
                        }
                    }
                }

                let background = self.material.background;
                let bg_color = Color32::from_rgb(
                    (background[0] * 255.0) as u8,
                    (background[1] * 255.0) as u8,
                    (background[2] * 255.0) as u8,
                );
                ui.painter().rect_filled(rect, 0.0, bg_color);

                let callback = ViewportCallback {
                    camera: self.camera,
                    material: self.material,
                    aspect_ratio,
                };
                ui.painter()
                    .add(egui_wgpu::Callback::new_paint_callback(rect, callback));

                self.show_warning_overlay(ui.ctx(), rect);

                if response.dragged() || scroll_delta != 0.0 {
                    ui.ctx().request_repaint();
                }
            });
    }
}

