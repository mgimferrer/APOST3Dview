use glam::{Mat4, Quat, Vec3};

/// Trackball-style orbit camera: rotates around a fixed target at a fixed
/// distance, with its orientation stored as a quaternion. Every rotation
/// is applied about the camera's own current screen axes (horizontal
/// input turns around the screen's vertical axis, vertical input around
/// its horizontal axis), so there is no fixed world "up", no pitch limit
/// and no gimbal lock: the molecule can be turned any number of times in
/// any direction, including over the top. A yaw/pitch camera with a
/// fixed up vector cannot do this, since it has to clamp pitch short of
/// +/-90 degrees to avoid flipping, which leaves some orientations
/// unreachable.
/// `PartialEq` (exact float equality) is used by the live view's
/// "has the camera actually moved since last frame" check — see
/// `App`'s AO settle logic — not for any tolerance-based comparison,
/// so exactness is exactly what's wanted: only real orbit/pan/zoom
/// input changes these fields at all, never incidental drift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    /// Rotation from camera-local space (looking down -Z, +Y up, +X
    /// right) to world space.
    pub orientation: Quat,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 12.0,
            orientation: Self::orientation_from_yaw_pitch(-0.6, 0.35),
            fov_y_radians: 45.0_f32.to_radians(),
            near: 0.05,
            far: 500.0,
        }
    }
}

impl OrbitCamera {
    const MIN_DISTANCE: f32 = 0.5;
    const MAX_DISTANCE: f32 = 500.0;

    /// Orientation equivalent to a classic yaw/pitch orbit position
    /// (yaw around world +Y, then pitch up toward +Y), for setting a
    /// familiar starting view.
    pub fn orientation_from_yaw_pitch(yaw: f32, pitch: f32) -> Quat {
        Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch)
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.orientation * Vec3::new(0.0, 0.0, self.distance)
    }

    pub fn view_matrix(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, self.orientation * Vec3::Y)
    }

    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        // wgpu's NDC depth range is [0, 1], matching the directx convention.
        glam::camera::rh::proj::directx::perspective(
            self.fov_y_radians,
            aspect_ratio.max(0.0001),
            self.near,
            self.far,
        )
    }

    pub fn view_projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        self.projection_matrix(aspect_ratio) * self.view_matrix()
    }

    /// Rotates the view about the camera's current screen axes: positive
    /// `delta_yaw` around the screen's vertical axis, positive
    /// `delta_pitch` moves the camera up over the target. Unlimited in
    /// both directions.
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.orientation = (self.orientation * Quat::from_rotation_y(delta_yaw) * Quat::from_rotation_x(-delta_pitch)).normalize();
    }

    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let (right, up) = self.screen_basis();
        self.target += right * delta_x + up * delta_y;
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta).clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    pub fn forward(&self) -> Vec3 {
        self.orientation * Vec3::NEG_Z
    }

    /// (right, up) camera basis vectors, used to billboard atom impostors
    /// toward the camera.
    pub fn screen_basis(&self) -> (Vec3, Vec3) {
        (self.orientation * Vec3::X, self.orientation * Vec3::Y)
    }

    /// Frames the camera on a bounding sphere: recenters the target and
    /// backs the distance off enough for the whole molecule to fit in view
    /// at the current field of view.
    pub fn frame_bounds(&mut self, center: Vec3, radius: f32) {
        self.target = center;
        self.distance = (radius / (self.fov_y_radians * 0.5).sin()).max(Self::MIN_DISTANCE);
    }

    /// World units spanned by one screen pixel at a given distance from
    /// the eye — the standard perspective-camera conversion. Used to size
    /// 3D-anchored labels so they keep a constant *apparent* (on-screen)
    /// size regardless of how far their anchor is from the camera, the
    /// same way a 2D overlay would behave, despite now being real
    /// world-space geometry.
    pub fn world_units_per_pixel(&self, distance: f32, viewport_height_px: f32) -> f32 {
        if viewport_height_px <= 0.0 {
            return 0.0;
        }
        2.0 * distance * (self.fov_y_radians * 0.5).tan() / viewport_height_px
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec_close(a: Vec3, b: Vec3, tol: f32) {
        assert!((a - b).length() < tol, "expected {b:?}, got {a:?}");
    }

    #[test]
    fn default_view_matches_the_previous_yaw_pitch_camera() {
        // The yaw/pitch camera this replaced placed the eye at
        // target + d * (cos p sin y, sin p, cos p cos y); the default
        // starting view should be unchanged.
        let camera = OrbitCamera::default();
        let (yaw, pitch, d) = (-0.6f32, 0.35f32, camera.distance);
        let expected = Vec3::new(d * pitch.cos() * yaw.sin(), d * pitch.sin(), d * pitch.cos() * yaw.cos());
        assert_vec_close(camera.eye(), expected, 1e-4);
    }

    #[test]
    fn vertical_rotation_is_unlimited_and_goes_over_the_top() {
        // A full 360 degree vertical turn, in small steps the way the
        // arrow keys apply it, must come back to the starting view rather
        // than stopping at the pole.
        let mut camera = OrbitCamera::default();
        let start = camera.eye();
        let steps = 720;
        let mut highest = f32::MIN;
        for _ in 0..steps {
            camera.orbit(0.0, std::f32::consts::TAU / steps as f32);
            highest = highest.max(camera.eye().y);
        }
        assert!(highest > camera.distance * 0.99, "camera never passed over the top (max height {highest})");
        assert_vec_close(camera.eye(), start, 1e-2);
    }

    #[test]
    fn basis_stays_orthonormal_and_aimed_at_the_target() {
        let mut camera = OrbitCamera::default();
        camera.target = Vec3::new(1.0, -2.0, 0.5);
        for i in 0..5000 {
            let t = i as f32 * 0.01;
            camera.orbit(0.013 * t.sin(), 0.017 * (1.3 * t).cos());
        }
        let (right, up) = camera.screen_basis();
        let forward = camera.forward();
        assert!((right.length() - 1.0).abs() < 1e-4 && (up.length() - 1.0).abs() < 1e-4);
        assert!(right.dot(up).abs() < 1e-4 && right.dot(forward).abs() < 1e-4 && up.dot(forward).abs() < 1e-4);
        assert_vec_close(forward, (camera.target - camera.eye()).normalize(), 1e-4);
    }
}
