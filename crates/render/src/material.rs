use glam::Vec3;

/// Live-bound material/lighting parameters, driven by the side panel.
/// Mirrors CYLview's Styles panel.
///
/// Cook-Torrance/GGX (see `sphere.wgsl`'s `shade`), not Blinn-Phong —
/// `roughness`/`reflectance` replace the old `specular`/`shininess` pair,
/// and `light_intensity` replaces `diffuse` (in a physically-based model
/// there's one light intensity driving both the diffuse and specular
/// terms together, not two independently-tunable strengths). Switched
/// 2026-08-29 after a real side-by-side preview (`test_ggx_ao_preview.rs`,
/// since removed) showed the isosurface in particular reading as
/// noticeably more dimensional under GGX — a real, defined highlight
/// instead of a flatter, more matte response on that large a surface.
#[derive(Debug, Clone, Copy)]
pub struct Material {
    /// Hemisphere-ambient strength (see `hemisphere_ambient` in the
    /// shaders) — unrelated to the BRDF swap, kept as-is.
    pub ambient: f32,
    /// 0.0 = mirror-smooth, 1.0 = fully matte. Drives the GGX normal
    /// distribution directly, so highlight *shape* scales correctly with
    /// this instead of an arbitrary shininess exponent.
    pub roughness: f32,
    /// F0 — reflectance at normal incidence (0 degrees, straight-on).
    /// ~0.02-0.06 is the real-world range for dielectrics (CPK balls,
    /// glass, plastic — anything non-metal); pushed higher it starts
    /// reading as a burnished/metallic surface, which is a legitimate
    /// look to dial in on purpose but isn't the CPK-accurate default.
    pub reflectance: f32,
    /// Overall key-light brightness — multiplies the combined diffuse +
    /// specular contribution (see `shade`), since both come from the same
    /// light in a physically-based model.
    pub light_intensity: f32,
    pub light_yaw: f32,
    pub light_pitch: f32,
    pub background: [f32; 3],
    /// Multiplier on each atom's van der Waals radius (VMD "Sphere Scale").
    pub atom_scale: f32,
    /// Bond cylinder radius, angstrom (VMD "Bond Radius" — an absolute
    /// value there, not a multiplier).
    pub bond_radius: f32,
    /// Linear-light multiplier applied before the filmic tone-mapping curve
    /// (see `sphere.wgsl`'s `finalize_color`) — 1.0 is neutral. This is the
    /// one user-facing knob for the whole tone-mapping pipeline; everything
    /// else (the ACES-style rolloff curve itself, the sRGB/linear
    /// conversion) is fixed, not exposed, to keep the Style panel from
    /// growing controls nobody needs to touch day to day.
    pub exposure: f32,
}

impl Default for Material {
    /// The reference look — matches what Martí tuned to and confirmed
    /// against the VMD/CYLview reference images (2026-08-12, atom/bond
    /// scale revised 2026-08-28 once ambient occlusion made a slightly
    /// fuller geometry read better). This is what the style panel's
    /// "Default" button resets to.
    fn default() -> Self {
        Self {
            ambient: 0.30,
            // Tightened from 0.42 (2026-08-29): with AO on, the dampened
            // reflectance/light_intensity below already pulls the
            // highlight way down — a broader roughness on top of that
            // left atoms/bonds reading noticeably flatter than the
            // isosurface right next to them, once the isosurface actually
            // got a defined GGX highlight of its own. A touch tighter
            // gives them back some visible material presence without
            // sliding back toward a hot plastic look.
            roughness: 0.32,
            reflectance: 0.045,
            light_intensity: 3.0,
            // Small offset from the camera (see light_dir below) — Martí
            // tuned this by eye (2026-08-12): a slight angle here reads
            // better than a dead-on flash, while staying subtle enough
            // that no part of the visible hemisphere goes into shadow.
            light_yaw: -0.5,
            light_pitch: 0.20,
            background: [1.0, 1.0, 1.0],
            atom_scale: 0.24,
            bond_radius: 0.16,
            exposure: 1.0,
        }
    }
}

impl Material {
    /// Same as [`Default`] but with the light dead-on the camera (zero
    /// yaw/pitch offset) — for image export rather than interactive
    /// viewing. The small default offset reads better on screen, but it
    /// means two different orientations of the same molecule can show
    /// visibly different highlight placement, which is undesirable when
    /// the images are meant to sit side by side in a publication. Zero
    /// offset removes that last bit of orientation-dependence entirely.
    ///
    /// Reflectance is also knocked down a bit further here — a dead-on
    /// light isn't a neutral choice for a GGX material: it puts the
    /// specular peak exactly where the surface normal points back at the
    /// camera (half-vector ≈ normal there), concentrating it into a
    /// harsher "flash" hotspot than the default's off-axis light ever
    /// produces, for any roughness. Found by hands-on testing
    /// (2026-08-29) — the same molecule looked noticeably shinier under
    /// Publication than Default for exactly this reason, not a bug in
    /// the BRDF itself, just the geometry this preset deliberately
    /// chooses. Compensating here keeps publication renders from
    /// reading shinier than what's tuned live on screen.
    pub fn publication() -> Self {
        Self { light_yaw: 0.0, light_pitch: 0.0, reflectance: Self::default().reflectance * 0.6, ..Self::default() }
    }

    /// Light direction as an offset from camera-forward, in the camera's
    /// own (right, up, forward) basis — a "headlight" rig. Rotates with
    /// the camera so the lit/shadowed pattern on the molecule stays fixed
    /// relative to the screen as you orbit, instead of sweeping across the
    /// molecule the way a world-fixed light direction would: since the
    /// offset itself is defined relative to the camera, not the world,
    /// shading is rotation-invariant regardless of how far off-axis it is
    /// — a small offset (the default) just reads better visually than a
    /// dead-on flash, without reintroducing the original problem.
    pub fn light_dir(&self, camera_right: Vec3, camera_up: Vec3, camera_forward: Vec3) -> Vec3 {
        // Subtract the forward component: a headlight sits near the
        // camera and shines into the scene, so the surface-to-light
        // direction points back out toward the viewer, not further in.
        (camera_right * self.light_pitch.cos() * self.light_yaw.sin()
            + camera_up * self.light_pitch.sin()
            - camera_forward * self.light_pitch.cos() * self.light_yaw.cos())
        .normalize()
    }
}
