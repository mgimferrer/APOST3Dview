//! Depth of field: a cheap, standard post-process technique — blur the
//! whole rendered frame once at a fixed radius, then mix the sharp and
//! blurred versions per pixel based on how far that pixel's world-space
//! depth sits from the focal plane. Not physically-accurate bokeh (no
//! per-CoC variable-radius sampling, no aperture shape) — for a molecule
//! viewer's "hero shot" use case, a uniform blur mixed by distance already
//! reads as real depth of field and is far cheaper than a proper circle-
//! of-confusion pass.
//!
//! Architecturally this is a bigger change than ambient occlusion (see
//! `ao.rs`'s module doc for why AO could get away with sampling inline
//! during the normal forward draw): DoF's blur pass needs the *already
//! composited* neighboring pixels of the finished frame, which don't exist
//! yet mid-draw. So unlike AO, DoF genuinely needs the scene rendered to
//! an offscreen texture first, blurred, composited, and only then blitted
//! into whatever the caller is actually drawing into (the live view's
//! shared egui pass, or the export readback). See `viewport.rs`'s
//! `run_live_dof_pass`/`render_offscreen` for where that happens.
//!
//! The focal plane always sits at the camera's own orbit distance (`OrbitCamera::distance`)
//! — i.e. whatever the camera is currently orbiting/zoomed around, which in
//! practice is the molecule (or the point last framed on) — so there's no
//! separate "focus point" control to expose; only how wide the sharp zone
//! is (`focus_range`, relative to that same distance so it scales
//! automatically with molecule size/zoom) and how strong the blur gets
//! beyond it (`strength`).

use bytemuck::{Pod, Zeroable};

/// Live-tunable depth-of-field parameters, driven by the Style panel.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DofSettings {
    /// Half-width of the perfectly-sharp zone around the focal plane, as a
    /// fraction of the camera's orbit distance — e.g. 0.15 means +/-15% of
    /// the current zoom distance stays fully sharp. Deliberately tuned
    /// against `OrbitCamera::frame_bounds`'s own math (distance ≈
    /// radius / sin(fov/2), so at the default ~45° fov distance ≈ 2.6 ×
    /// the molecule's bounding radius): a value much above ~0.2 here means
    /// the sharp zone alone already covers the molecule's entire depth
    /// extent on a freshly-framed view, and the whole effect reads as
    /// completely invisible regardless of `strength` — confirmed with
    /// `examples/test_dof.rs` against a real 83-atom molecule, where 0.35
    /// (the first value tried) produced a literal zero-pixel-changed
    /// result. Expressed relative to distance (not an absolute Angstrom
    /// value) so the same default looks right whether the molecule on
    /// screen is a triatomic or an 83-atom
    /// complex, without per-molecule retuning.
    pub focus_range: f32,
    /// Overall blur amount, 0.0..=1.0 — both the blur radius (as a
    /// fraction of the viewport height) and, since a strength of 0
    /// produces a literal zero-radius "blur" pass (a no-op), the entire
    /// visible strength of the effect. This is the control that "makes it
    /// zero" per Martí's request — no separate on/off needed at this
    /// level (though `App` also keeps a checkbox, matching the AO panel's
    /// shape, for skipping the extra passes' GPU cost entirely).
    pub strength: f32,
}

impl Default for DofSettings {
    fn default() -> Self {
        Self { focus_range: 0.15, strength: 0.5 }
    }
}

/// Maximum blur radius, in pixels, at `strength = 1.0` — as a fraction of
/// the viewport's own height, so live view and a much-higher-resolution
/// DPI export both read as the same visual strength of blur rather than
/// the export needing a far larger `strength` value to look the same.
const MAX_BLUR_FRACTION_OF_HEIGHT: f32 = 0.05;

/// Uniforms for the separable blur pass (run twice: horizontal, then
/// vertical) — a plain (non-depth-aware) Gaussian over the resolved scene
/// color, unlike AO's depth-aware blur, since DoF's blur is meant to
/// bleed across depth discontinuities (that's what a real out-of-focus
/// background behind a sharp foreground edge looks like).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct DofBlurUniforms {
    /// x, y = texel-space blur direction (1,0 or 0,1), z = radius in
    /// texels, w unused.
    pub direction: [f32; 4],
    /// width, height, unused, unused.
    pub screen: [f32; 4],
}

impl DofBlurUniforms {
    pub fn new(direction: [f32; 2], width: u32, height: u32, strength: f32) -> Self {
        let radius_px = strength.max(0.0) * height as f32 * MAX_BLUR_FRACTION_OF_HEIGHT;
        Self { direction: [direction[0], direction[1], radius_px, 0.0], screen: [width as f32, height as f32, 0.0, 0.0] }
    }
}

/// Uniforms for the composite pass: mixes the sharp and fully-blurred
/// scene textures per pixel based on world-space distance from the focal
/// plane.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct DofCompositeUniforms {
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_eye: [f32; 4],
    /// focus_distance, focus_range (both world units), unused, unused.
    pub params: [f32; 4],
    /// width, height, unused, unused.
    pub screen: [f32; 4],
}

impl DofCompositeUniforms {
    pub fn new(inv_view_proj: glam::Mat4, camera_eye: glam::Vec3, focus_distance: f32, width: u32, height: u32, settings: &DofSettings) -> Self {
        let focus_range_world = (settings.focus_range.max(0.0) * focus_distance).max(0.001);
        Self {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_eye: [camera_eye.x, camera_eye.y, camera_eye.z, 1.0],
            params: [focus_distance, focus_range_world, 0.0, 0.0],
            screen: [width as f32, height as f32, 0.0, 0.0],
        }
    }
}
