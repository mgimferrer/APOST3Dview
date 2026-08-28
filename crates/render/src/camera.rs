use glam::{Mat4, Vec3};

/// Orbit camera: rotates around a fixed target at a fixed distance, driven
/// by yaw/pitch. Matches the drag-to-orbit / scroll-to-zoom interaction
/// model used by CYLview, IBOview and VMD.
/// `PartialEq` (exact float equality) is used by the live view's
/// "has the camera actually moved since last frame" check — see
/// `App`'s AO settle logic — not for any tolerance-based comparison,
/// so exactness is exactly what's wanted: only real orbit/pan/zoom
/// input changes these fields at all, never incidental drift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 12.0,
            yaw: -0.6,
            pitch: 0.35,
            fov_y_radians: 45.0_f32.to_radians(),
            near: 0.05,
            far: 500.0,
        }
    }
}

impl OrbitCamera {
    // Just shy of +/- 90 degrees so the up vector never flips.
    const PITCH_LIMIT: f32 = 1.5;
    const MIN_DISTANCE: f32 = 0.5;
    const MAX_DISTANCE: f32 = 500.0;

    pub fn eye(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    pub fn view_matrix(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Y)
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

    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-Self::PITCH_LIMIT, Self::PITCH_LIMIT);
    }

    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let forward = (self.target - self.eye()).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        self.target += right * delta_x + up * delta_y;
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta).clamp(Self::MIN_DISTANCE, Self::MAX_DISTANCE);
    }

    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye()).normalize_or_zero()
    }

    /// (right, up) camera basis vectors, used to billboard atom impostors
    /// toward the camera.
    pub fn screen_basis(&self) -> (Vec3, Vec3) {
        let forward = self.forward();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        (right, up)
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
