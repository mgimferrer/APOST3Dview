//! The 3D viewport.

use apost3dview_core::{element_data, measure};
use apost3dview_render::{glyph_scale_for_font_size, glyph_scale_for_world_size, pick_atom, pick_bond, ray_from_ndc, GlyphInstance, ViewportCallback};
use egui::Color32;
use glam::Vec2;

use crate::app::App;
use crate::labels::{EDGE_BIAS_ATOM_LABEL, EDGE_BIAS_BOLD, EDGE_BIAS_NORMAL, push_label};
use crate::state::AtomLabelMode;
use crate::util::{color32_to_rgb, format_measurement, measurement_anchor, project_to_screen, toggle_selected};

/// Arrow-key rotation speed, radians per second.
pub(crate) const ARROW_ROTATE_SPEED: f32 = 1.2;
/// Default auto-spin speed, radians per second (one turn in ~12.5 s), a
/// calm pace suited to presentations.
pub(crate) const DEFAULT_AUTO_SPIN_SPEED: f32 = 0.5;

impl App {
    /// The 3D viewport (central panel): camera input, picking, labels, and
    /// the wgpu paint callback.
    pub(crate) fn show_viewport(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();

        let background = self.material.background;
        let bg_color = Color32::from_rgb(
            (background[0] * 255.0) as u8,
            (background[1] * 255.0) as u8,
            (background[2] * 255.0) as u8,
        );
        ui.painter().rect_filled(rect, 0.0, bg_color);

        let Some(active) = self.active_structure else {
            self.show_empty_state(ui, rect);
            return;
        };

        let response = ui.interact(
            rect,
            ui.id().with("viewport"),
            egui::Sense::click_and_drag(),
        );

        // Left- or right-drag rotates, shift+drag pans. A left
        // *click* without movement still selects (below): egui only
        // reports a drag once the pointer moves past its drag
        // threshold, so the two don't conflict. This makes the
        // viewport usable from a trackpad, where a right-button
        // drag is awkward or unavailable. Dragging a measurement
        // label still moves the label, since labels are registered
        // later, on top of the viewport.
        let camera_dragging = response.dragged_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Secondary);
        let drag_delta = response.drag_delta();
        if camera_dragging {
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
        // Trackpad pinch (and ctrl/cmd+scroll, which egui reports
        // the same way): a multiplicative zoom factor.
        let zoom_factor = ui.input(|i| i.zoom_delta());
        if zoom_factor != 1.0 && zoom_factor > 0.0 {
            self.camera.zoom(self.camera.distance * (1.0 - 1.0 / zoom_factor));
        }

        // Arrow keys rotate continuously while held, at a speed
        // independent of frame rate, with no limit in any
        // direction (see `OrbitCamera`). Two arrows together turn
        // along the diagonal, at the same speed as a single arrow.
        // Ignored while a text field (or other widget) has
        // keyboard focus, so typing doesn't rotate the molecule.
        let keyboard_free = !ui.ctx().egui_wants_keyboard_input();
        let (key_direction, space_pressed, dt) = ui.input(|i| {
            let mut direction = Vec2::ZERO;
            if i.key_down(egui::Key::ArrowLeft) {
                direction.x -= 1.0;
            }
            if i.key_down(egui::Key::ArrowRight) {
                direction.x += 1.0;
            }
            if i.key_down(egui::Key::ArrowUp) {
                direction.y += 1.0;
            }
            if i.key_down(egui::Key::ArrowDown) {
                direction.y -= 1.0;
            }
            // Clamped so a stalled frame doesn't produce a jump.
            (direction, i.key_pressed(egui::Key::Space), i.stable_dt.min(0.1))
        });
        let key_direction = if keyboard_free { key_direction.normalize_or_zero() } else { Vec2::ZERO };
        let keyboard_rotating = key_direction != Vec2::ZERO;
        if keyboard_rotating {
            self.spin_direction = key_direction;
            self.camera.orbit(key_direction.x * ARROW_ROTATE_SPEED * dt, key_direction.y * ARROW_ROTATE_SPEED * dt);
        }
        if keyboard_free && space_pressed {
            self.auto_spin = !self.auto_spin;
        }
        let auto_spinning = self.auto_spin && !keyboard_rotating && !camera_dragging;
        if auto_spinning {
            self.camera.orbit_along(self.spin_direction, self.auto_spin_speed * dt);
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.clear_selection();
        }

        let aspect_ratio = if rect.height() > 0.0 { rect.width() / rect.height() } else { 1.0 };
        self.last_aspect_ratio = aspect_ratio;

        // Left-click always does something useful: in Measure
        // mode it extends the pending pick, otherwise it selects
        // whatever's under the cursor — an atom if one's there,
        // a bond if not — for the Molecule window's selection actions.
        // No separate "Atoms/Bonds/Off" mode needed for that
        // anymore, since a click can only ever mean one thing at
        // a time regardless.
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let ndc_x = ((pos.x - rect.left()) / rect.width()) * 2.0 - 1.0;
                let ndc_y = 1.0 - ((pos.y - rect.top()) / rect.height()) * 2.0;
                let (origin, direction) = ray_from_ndc(&self.camera, aspect_ratio, ndc_x, ndc_y);

                let structure = &self.structures[active];
                let atom_hit = pick_atom(&structure.molecule, origin, direction, self.material.atom_scale, &structure.hidden_atoms);
                let bond_hit = if atom_hit.is_none() {
                    pick_bond(
                        &structure.molecule,
                        origin,
                        direction,
                        self.material.bond_radius,
                        &structure.hidden_atoms,
                        &structure.hidden_bonds,
                    )
                } else {
                    None
                };

                if let Some(index) = atom_hit {
                    toggle_selected(&mut self.structures[active].selected_atoms, index);
                    self.rebuild_highlights();
                } else if let Some(index) = bond_hit {
                    toggle_selected(&mut self.structures[active].selected_bonds, index);
                    self.rebuild_highlights();
                }
            }
        }

        // True 3D labels: real depth-tested billboard glyph quads
        // (see crates/render/src/{glyphs,label,shaders/text.wgsl}),
        // not a 2D UI overlay — so a bond or atom nearer the camera
        // correctly, even partially, occludes them. Built fresh
        // each repaint and handed to the wgpu callback below.
        // Atom labels are sized in world units (scale with zoom,
        // like the atom itself); measurement labels still hold a
        // constant apparent screen size, which depends on distance
        // from the camera and so is also recomputed every frame.
        let mut label_instances: Vec<GlyphInstance> = Vec::new();

        if self.atom_label_mode != AtomLabelMode::None {
            let structure = &self.structures[active];
            let color = color32_to_rgb(self.atom_label_style.text_color);
            for (index, (&z, &position)) in structure.molecule.atomic_numbers.iter().zip(&structure.molecule.positions).enumerate()
            {
                if structure.hidden_atoms.contains(&index) {
                    continue;
                }
                // A real world-space size, tied to this atom's own
                // rendered radius — labels are true 3D geometry, so
                // they grow and shrink with zoom exactly like the
                // atom they're attached to, rather than holding a
                // constant on-screen size the way a 2D overlay would.
                let radius = element_data(z).vdw_radius * self.material.atom_scale;
                let scale = glyph_scale_for_world_size(radius * self.atom_label_style.relative_size);
                let text = match self.atom_label_mode {
                    AtomLabelMode::Number => format!("{}", index + 1),
                    AtomLabelMode::Type => element_data(z).symbol.to_string(),
                    AtomLabelMode::NumberType => format!("{}{}", element_data(z).symbol, index + 1),
                    AtomLabelMode::None => unreachable!(),
                };
                // The label anchor starts at the atom's own 3D
                // center, but the sphere impostor's near surface
                // renders a full radius closer to the camera than
                // that — so a label sitting exactly at the center
                // is depth-occluded by its own atom. Push the
                // anchor toward the camera past the sphere's near
                // surface (radius + a little clearance) so the
                // label wins the depth test against its own atom.
                let to_camera = (self.camera.eye() - position).normalize_or_zero();
                let label_anchor = position + to_camera * (radius * 1.15 + 0.02);
                push_label(&mut label_instances, &self.glyph_atlas, &text, label_anchor, scale, color, EDGE_BIAS_ATOM_LABEL);
            }
        }

        // Measurement labels still live in screen space for the
        // *offset* the user drags them by (simplest way to keep
        // "nudge it N pixels clear of the clutter" behavior
        // whatever the current zoom/rotation) — but that offset
        // is converted to a world-space position every frame
        // before rendering, same as the atom labels above, so
        // it's still real, depth-tested 3D geometry.
        let mut label_updates: Vec<(usize, egui::Vec2)> = Vec::new();
        let (camera_right, camera_up) = self.camera.screen_basis();
        let measurement_color = color32_to_rgb(self.measurement_style.text_color);
        for (index, measurement) in self.structures[active].measurements.iter().enumerate() {
            let anchor = measurement_anchor(&self.structures[active].molecule, measurement.kind);
            let distance = (anchor - self.camera.eye()).length();
            let world_per_pixel = self.camera.world_units_per_pixel(distance, rect.height());
            let world_offset = camera_right * (measurement.label_offset.x * world_per_pixel)
                - camera_up * (measurement.label_offset.y * world_per_pixel);
            let final_anchor = anchor + world_offset;

            let value = measure(&self.structures[active].molecule, measurement.kind);
            let text = format_measurement(measurement.kind, value, self.coordinate_unit, self.measurement_style.decimal_places);
            let scale = glyph_scale_for_font_size(self.measurement_style.font_size, world_per_pixel);
            let edge_bias = if self.measurement_style.bold { EDGE_BIAS_BOLD } else { EDGE_BIAS_NORMAL };
            push_label(&mut label_instances, &self.glyph_atlas, &text, final_anchor, scale, measurement_color, edge_bias);

            // Hit-test area for dragging: approximate (text length
            // × font size), since there's no 2D layout pass to get
            // an exact box from anymore.
            if let Some(screen_pos) = project_to_screen(&self.camera, aspect_ratio, rect, final_anchor) {
                let approx_width = text.chars().count() as f32 * self.measurement_style.font_size * 0.55;
                let approx_height = self.measurement_style.font_size * 1.3;
                let hit_rect = egui::Rect::from_center_size(screen_pos, egui::vec2(approx_width, approx_height));
                let label_response = ui.interact(hit_rect, ui.id().with(("measurement_label", active, index)), egui::Sense::drag());
                if label_response.dragged() {
                    label_updates.push((index, measurement.label_offset + label_response.drag_delta()));
                }
            }
        }
        let any_label_dragged = !label_updates.is_empty();
        for (index, offset) in label_updates {
            if let Some(measurement) = self.structures[active].measurements.get_mut(index) {
                measurement.label_offset = offset;
            }
        }

        // "Phase C" progressive AO quality: cheap every frame the
        // camera is actually moving (orbit/pan/zoom) *or* a slider
        // just changed, a one-time full-quality recompute the
        // instant both stop changing, then no recompute at all on
        // further idle frames — see the `ao_last_camera`/
        // `ao_last_settings`/`ao_settled` field docs. The extra
        // `request_repaint` on a just-changed frame is what
        // guarantees a follow-up frame actually happens to catch
        // "it settled" — egui doesn't keep repainting once input
        // stops, so without this the settle transition would only
        // ever fire by coincidence.
        let ao_recompute_samples = if !self.ao_enabled {
            None
        } else if self.ao_last_camera != Some(self.camera) || self.ao_last_settings != Some(self.ao_settings) {
            self.ao_last_camera = Some(self.camera);
            self.ao_last_settings = Some(self.ao_settings);
            self.ao_settled = false;
            ui.ctx().request_repaint();
            Some(apost3dview_render::AO_LIVE_SAMPLE_COUNT)
        } else if !self.ao_settled {
            self.ao_settled = true;
            Some(apost3dview_render::AO_KERNEL_SIZE as u32)
        } else {
            None
        };

        let pixels_per_point = ui.ctx().pixels_per_point();
        let callback = ViewportCallback {
            camera: self.camera,
            material: self.ao_render_material(),
            aspect_ratio,
            label_instances,
            ambient_occlusion: self.ao_enabled.then_some(self.ao_settings),
            viewport_size_px: [(rect.width() * pixels_per_point).round() as u32, (rect.height() * pixels_per_point).round() as u32],
            // `rect` is the 3D viewport's own position within the
            // full window (in egui's logical points) — the AO
            // shaders need it in physical pixels, matching what
            // `@builtin(position)` actually reports (see
            // `ViewportCallback::viewport_offset_px`).
            viewport_offset_px: [rect.min.x * pixels_per_point, rect.min.y * pixels_per_point],
            ao_recompute_samples,
            depth_of_field: self.dof_enabled.then_some(self.dof_settings),
            dof_focus_distance: self.camera.distance,
            background: self.material.background,
        };
        ui.painter()
            .add(egui_wgpu::Callback::new_paint_callback(rect, callback));

        if camera_dragging || scroll_delta != 0.0 || zoom_factor != 1.0 || keyboard_rotating || auto_spinning || any_label_dragged {
            ui.ctx().request_repaint();
        }
    }
}
