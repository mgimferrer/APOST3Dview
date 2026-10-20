//! The 3D viewport.

use apost3dview_render::{pick_atom, pick_bond, ray_from_ndc, ViewportCallback};
use glam::Vec2;

use crate::app::App;
use crate::util::toggle_selected;

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
        let bg_color = crate::util::rgb_to_color32(background);
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
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
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
                // With a pair quantity shown, a click chooses the focus.
                if !self.focus_on_clicked_atom(index) {
                    toggle_selected(&mut self.structures[active].selected_atoms, index);
                    self.rebuild_highlights();
                }
            } else if let Some(index) = bond_hit {
                toggle_selected(&mut self.structures[active].selected_bonds, index);
                self.rebuild_highlights();
            }
        }

        // Labels over the molecule (atom names, measurements, APOST-3D
        // values), placed so none overlaps: see `labels.rs`. A
        // measurement label can be dragged where it landed.
        let labels = self.scene_labels(active, &self.camera, rect);
        let (label_instances, measurement_hits) = (labels.glyphs, labels.measurement_hits);
        let mut label_updates: Vec<(usize, egui::Vec2)> = Vec::new();
        for (index, hit_rect) in measurement_hits {
            let label_response = ui.interact(hit_rect, ui.id().with(("measurement_label", active, index)), egui::Sense::drag());
            if label_response.dragged() {
                let offset = self.structures[active].measurements[index].label_offset;
                label_updates.push((index, offset + label_response.drag_delta()));
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
        self.paint_legend(ui.painter(), rect);

        if camera_dragging || scroll_delta != 0.0 || zoom_factor != 1.0 || keyboard_rotating || auto_spinning || any_label_dragged {
            ui.ctx().request_repaint();
        }
    }
}
