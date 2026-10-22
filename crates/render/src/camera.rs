use glam::{Mat4, Quat, Vec2, Vec3};

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
    /// Where "up" is for the soft fill light (the sky/ground ambient),
    /// unit length: world +Y, unless the structure seen is aligned onto
    /// another, whose coordinates are then turned (`world_up` is the
    /// shared +Y in this structure's own coordinates, so both are lit
    /// alike). Not changed by orbiting, panning or zooming.
    pub world_up: Vec3,
    /// Parallel projection (no perspective), as VMD's and CYLview's
    /// figures: equal bonds look equal anywhere in the picture. The view
    /// then shows, at the target, the same height a perspective view at
    /// `distance` shows there (`2 · distance · tan(fov/2)`), so zooming,
    /// framing and saved views mean the same in both; the eye (for depth
    /// order, ambient occlusion, depth of field and picking) sits
    /// `ORTHOGRAPHIC_EYE_DISTANCE` behind the target.
    pub orthographic: bool,
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
            world_up: Vec3::Y,
            orthographic: false,
        }
    }
}

impl OrbitCamera {
    const MIN_DISTANCE: f32 = 0.5;
    const MAX_DISTANCE: f32 = 500.0;
    /// How far behind the target an orthographic camera's eye is: far
    /// enough that rays from it to anything in view are parallel to within
    /// 1/100 of a degree per angstrom off the axis, near enough for f32
    /// depths to stay precise to 1e-4 angstrom.
    pub const ORTHOGRAPHIC_EYE_DISTANCE: f32 = 1000.0;
    /// Depth range of an orthographic view either side of the target.
    const ORTHOGRAPHIC_DEPTH: f32 = 500.0;

    /// Orientation equivalent to a classic yaw/pitch orbit position
    /// (yaw around world +Y, then pitch up toward +Y), for setting a
    /// familiar starting view.
    pub fn orientation_from_yaw_pitch(yaw: f32, pitch: f32) -> Quat {
        Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch)
    }

    pub fn eye(&self) -> Vec3 {
        let distance = if self.orthographic { Self::ORTHOGRAPHIC_EYE_DISTANCE } else { self.distance };
        self.target + self.orientation * Vec3::new(0.0, 0.0, distance)
    }

    /// Distance from the eye to the target: where depth of field focuses.
    pub fn eye_distance(&self) -> f32 {
        if self.orthographic { Self::ORTHOGRAPHIC_EYE_DISTANCE } else { self.distance }
    }

    /// Half the height of the view at the target (in world units).
    pub fn half_height_at_target(&self) -> f32 {
        self.distance * (self.fov_y_radians * 0.5).tan()
    }

    pub fn view_matrix(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, self.orientation * Vec3::Y)
    }

    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        // wgpu's NDC depth range is [0, 1], matching the directx convention.
        if self.orthographic {
            let half_height = self.half_height_at_target();
            let half_width = half_height * aspect_ratio.max(0.0001);
            let (near, far) = (Self::ORTHOGRAPHIC_EYE_DISTANCE - Self::ORTHOGRAPHIC_DEPTH, Self::ORTHOGRAPHIC_EYE_DISTANCE + Self::ORTHOGRAPHIC_DEPTH);
            return glam::camera::rh::proj::directx::orthographic(-half_width, half_width, -half_height, half_height, near, far);
        }
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

    /// Rotates by `angle` radians along a screen-space `direction` (x =
    /// horizontal, y = vertical, need not be normalized), as one exact
    /// rotation about the fixed screen axis perpendicular to it. Applying
    /// it N times with `angle = TAU / N` returns exactly to the starting
    /// view, diagonals included, which is what makes a seamless looping
    /// animation possible (chaining small `orbit` steps only approximates
    /// a diagonal axis). Matches `orbit`'s sign conventions.
    pub fn orbit_along(&mut self, direction: Vec2, angle: f32) {
        let direction = direction.normalize_or_zero();
        if direction == Vec2::ZERO {
            return;
        }
        let axis = Vec3::new(-direction.y, direction.x, 0.0);
        self.orientation = (self.orientation * Quat::from_axis_angle(axis, angle)).normalize();
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
    /// Orthographic: the same at every depth, so `distance` is ignored.
    pub fn world_units_per_pixel(&self, distance: f32, viewport_height_px: f32) -> f32 {
        if viewport_height_px <= 0.0 {
            return 0.0;
        }
        let distance = if self.orthographic { self.distance } else { distance };
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
    fn diagonal_spin_closes_into_a_seamless_loop() {
        // One full turn split into animation frames must land exactly back
        // on the first frame, for any direction, so a looping GIF has no
        // visible jump.
        for direction in [Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0), Vec2::new(1.0, 1.0), Vec2::new(-0.3, 0.8)] {
            let mut camera = OrbitCamera::default();
            let start = camera.eye();
            let frames = 90;
            for _ in 0..frames {
                camera.orbit_along(direction, std::f32::consts::TAU / frames as f32);
            }
            assert_vec_close(camera.eye(), start, 1e-3);
        }
    }

    #[test]
    fn orbit_along_matches_orbit_for_small_steps() {
        // Same sign conventions as `orbit`: a tiny step along +x/+y should
        // move the eye the same way a tiny yaw/pitch step does.
        let mut a = OrbitCamera::default();
        let mut b = OrbitCamera::default();
        a.orbit(1e-3, 0.0);
        b.orbit_along(Vec2::X, 1e-3);
        assert_vec_close(a.eye(), b.eye(), 1e-5);
        let mut a = OrbitCamera::default();
        let mut b = OrbitCamera::default();
        a.orbit(0.0, 1e-3);
        b.orbit_along(Vec2::Y, 1e-3);
        assert_vec_close(a.eye(), b.eye(), 1e-5);
    }

    #[test]
    fn basis_stays_orthonormal_and_aimed_at_the_target() {
        let mut camera = OrbitCamera { target: Vec3::new(1.0, -2.0, 0.5), ..OrbitCamera::default() };
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

    fn ndc(camera: &OrbitCamera, p: Vec3) -> Vec3 {
        let clip = camera.view_projection_matrix(1.5) * p.extend(1.0);
        clip.truncate() / clip.w
    }

    #[test]
    fn orthographic_shows_the_target_plane_as_perspective_does() {
        let mut camera = OrbitCamera { target: Vec3::new(1.0, -2.0, 0.5), distance: 9.0, ..OrbitCamera::default() };
        camera.orbit(0.7, -0.4);
        let (right, up) = camera.screen_basis();
        let on_plane = camera.target + right * 2.0 - up * 1.3;
        let perspective = ndc(&camera, on_plane);
        camera.orthographic = true;
        let orthographic = ndc(&camera, on_plane);
        assert!((perspective.truncate() - orthographic.truncate()).length() < 1e-5, "{perspective:?} vs {orthographic:?}");
        // Parallel: moving along the view direction doesn't move a point
        // on screen, only in depth (front nearer than back).
        let front = ndc(&camera, on_plane - camera.forward() * 3.0);
        let back = ndc(&camera, on_plane + camera.forward() * 3.0);
        assert!((front.truncate() - orthographic.truncate()).length() < 1e-5);
        assert!((back.truncate() - orthographic.truncate()).length() < 1e-5);
        assert!(front.z < orthographic.z && orthographic.z < back.z && (0.0..1.0).contains(&front.z) && back.z < 1.0);
        // Labels: the same size at any depth, the size they have at the target.
        let at_target = camera.world_units_per_pixel(camera.distance, 800.0);
        assert_eq!(camera.world_units_per_pixel(camera.eye_distance() + 4.0, 800.0), at_target);
    }
}
