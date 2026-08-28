//! CPU-side data for the ambient-occlusion post-process: the GPU uniform
//! layout and a deterministic hemisphere sample kernel. GPU orchestration
//! (textures, passes) lives on `ViewportResources` in `viewport.rs` —
//! mirrors how `export.rs` keeps pure math separate from GPU code.
//!
//! One mechanism serves both the live interactive view and PNG export: a
//! depth+normal G-buffer pre-pass, screen-space sampled occlusion, and a
//! depth-aware blur, all sampled directly inside the atom/cylinder
//! fragment shaders themselves (`fs_main_ao` in `sphere.wgsl`/
//! `cylinder.wgsl`) rather than a separate composite pass — the live view
//! is a callback inside egui's own render pass and can't open a second
//! pass to read back what it just drew, so "sample a precomputed texture
//! during the normal forward draw" is the one approach that works for
//! both. The SSAO pass's sample count is runtime-configurable (packed into
//! `screen.w`, not a shader constant) specifically so the live view can
//! request far fewer samples than export without needing a second shader
//! variant — quality dial, not a different code path.

use bytemuck::{Pod, Zeroable};

/// Upper bound on the kernel array size — export uses all of it; the live
/// view requests a much smaller `sample_count` (see `AoUniforms::new`) for
/// interactive framerate, reusing the same array/shader/pipeline.
pub const AO_KERNEL_SIZE: usize = 160;
/// Live-view sample count — small enough to hold interactive framerate on
/// typical hardware; unlike export this reruns every frame the camera
/// moves, so it can't afford anywhere near the export count.
pub const AO_LIVE_SAMPLE_COUNT: u32 = 32;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct AoUniforms {
    pub inv_view_proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub camera_eye: [f32; 4],
    /// radius (world units, angstrom), strength (0..1, blended against no
    /// occlusion when sampled), depth bias (angstrom), contrast power
    pub params: [f32; 4],
    /// screen width (px), screen height (px), outline strength, sample
    /// count (how many of `kernel`'s entries to actually use this pass —
    /// see the module doc)
    pub screen: [f32; 4],
    pub kernel: [[f32; 4]; AO_KERNEL_SIZE],
}

/// Tunables — kept separate from `AoUniforms` (the raw GPU layout) so call
/// sites don't need to know about padding/the kernel. Live-adjustable via
/// Style window sliders; shared by the live view and export so what you
/// tune is what you get in the exported PNG too. `PartialEq` is used by
/// the live view's settle-quality logic (see `App`) — dragging a slider
/// has to be able to trigger a recompute the exact same way moving the
/// camera does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AoSettings {
    pub radius: f32,
    pub strength: f32,
    pub bias: f32,
    pub contrast_power: f32,
    pub outline_strength: f32,
}

impl Default for AoSettings {
    fn default() -> Self {
        Self { radius: 1.5, strength: 1.0, bias: 0.015, contrast_power: 2.0, outline_strength: 2.5 }
    }
}

impl AoUniforms {
    pub fn new(
        inv_view_proj: glam::Mat4,
        view_proj: glam::Mat4,
        camera_eye: glam::Vec3,
        screen_width: u32,
        screen_height: u32,
        sample_count: u32,
        settings: &AoSettings,
    ) -> Self {
        Self {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            view_proj: view_proj.to_cols_array_2d(),
            camera_eye: [camera_eye.x, camera_eye.y, camera_eye.z, 0.0],
            params: [settings.radius, settings.strength, settings.bias, settings.contrast_power],
            screen: [screen_width as f32, screen_height as f32, settings.outline_strength, sample_count.min(AO_KERNEL_SIZE as u32) as f32],
            kernel: generate_hemisphere_kernel(),
        }
    }
}

/// GPU layout for the depth-aware separable blur pass — `direction` is a
/// texel step (1,0 for the horizontal pass, 0,1 for the vertical one),
/// packed into a `vec4` to satisfy WGSL's uniform alignment rules like
/// everywhere else in this module.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct BlurUniforms {
    pub direction: [f32; 4],
    pub screen: [f32; 4],
}

impl BlurUniforms {
    pub fn new(direction: [f32; 2], screen_width: u32, screen_height: u32) -> Self {
        Self { direction: [direction[0], direction[1], 0.0, 0.0], screen: [screen_width as f32, screen_height as f32, 0.0, 0.0] }
    }
}

/// GPU layout for sampling the finished AO+depth textures back inside the
/// atom/cylinder fragment shaders (`apply_ao` in `sphere.wgsl`/
/// `cylinder.wgsl`) — deliberately leaner than `AoUniforms` (no kernel, no
/// duplicate camera data already available from the shared scene uniform
/// at group 0) since this one gets bound on every shaded draw call.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct AoSampleUniforms {
    pub inv_view_proj: [[f32; 4]; 4],
    /// strength, outline_strength, unused, unused
    pub params: [f32; 4],
    /// width, height (of the AO/depth textures being sampled, i.e. the
    /// live 3D viewport's own size — *not* the full window)
    pub screen: [f32; 4],
    /// x, y, unused, unused — the live viewport rect's top-left, in
    /// physical pixels *within the full window*. `@builtin(position)` in
    /// `fs_main_ao` (see `sphere.wgsl`/`cylinder.wgsl`) is always relative
    /// to the whole shared render target the live view draws into, not
    /// the sub-rect egui allocated for the 3D viewport — subtracting this
    /// offset is what maps a window-space fragment coordinate back onto
    /// this viewport-sized AO texture's own (0,0)-origin coordinates.
    /// Always `[0,0]` for export, which has no such sub-rect at all (the
    /// render target *is* the whole image).
    pub offset: [f32; 4],
}

impl AoSampleUniforms {
    pub fn new(inv_view_proj: glam::Mat4, screen_width: u32, screen_height: u32, offset: [f32; 2], settings: &AoSettings) -> Self {
        Self {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            params: [settings.strength, settings.outline_strength, 0.0, 0.0],
            screen: [screen_width as f32, screen_height as f32, 0.0, 0.0],
            offset: [offset[0], offset[1], 0.0, 0.0],
        }
    }
}

/// A fixed-seed hemisphere sample kernel, biased toward the origin (more
/// samples close to the surface than far from it, the standard SSAO kernel
/// shape) — deterministic so repeated exports of the same view are
/// pixel-identical rather than flickering between runs.
fn generate_hemisphere_kernel() -> [[f32; 4]; AO_KERNEL_SIZE] {
    let mut state: u32 = 0x9E3779B9;
    let mut next_unit = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state >> 8) as f32 / (1u32 << 24) as f32
    };

    let mut kernel = [[0.0f32; 4]; AO_KERNEL_SIZE];
    for (i, slot) in kernel.iter_mut().enumerate() {
        let x = next_unit() * 2.0 - 1.0;
        let y = next_unit() * 2.0 - 1.0;
        let z = next_unit();
        let mut sample = glam::Vec3::new(x, y, z).normalize_or_zero() * next_unit();
        let t = i as f32 / AO_KERNEL_SIZE as f32;
        sample *= 0.1 + 0.9 * t * t;
        *slot = [sample.x, sample.y, sample.z, 0.0];
    }
    kernel
}
