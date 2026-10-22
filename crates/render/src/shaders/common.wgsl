// Shared shader code, part 1: pure functions with no bindings, so this can
// be prepended to any shader module (see `shaders.rs` for which files each
// module is assembled from). WGSL has no include mechanism; every module
// is one source string, built at compile time with `concat!`.

// ---- Color pipeline: sRGB<->linear, filmic tone mapping, fill light ----

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Narkowicz's ACES filmic fit — cheap, no LUT, and gives bright highlights
// a soft shoulder instead of clipping straight to flat white the way
// unclamped linear output does.
fn aces_tonemap(c: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let cc = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((c * (a * c + vec3<f32>(b))) / (c * (cc * c + vec3<f32>(d)) + vec3<f32>(e)), vec3<f32>(0.0), vec3<f32>(1.0));
}

// A cheap image-based-lighting stand-in: tints the ambient term by world-
// space surface orientation (brighter/cooler facing up, dimmer/warmer
// facing down) instead of a single flat scalar, so the side of a sphere
// facing away from the key light still shows a gradient rather than
// crushing to a flat near-black — a fixed, not user-tunable, "fill light"
// in effect (see the module doc on why: one exposure slider is worth
// exposing, three more light-rig sliders aren't).
fn hemisphere_ambient(normal: vec3<f32>, up: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(1.05, 1.05, 1.1);
    let ground = vec3<f32>(0.65, 0.62, 0.6);
    return mix(ground, sky, dot(normal, up) * 0.5 + 0.5);
}

// ---- Cook-Torrance/GGX BRDF ------------------------------------------
// Standard formulation (Trowbridge-Reitz normal distribution, Smith
// geometry term with Karis/Epic's direct-lighting `k` remap, Schlick
// Fresnel) — the same shape every modern real-time renderer (Blender's
// Principled BSDF included) uses. Replaced Blinn-Phong 2026-08-29: a real
// side-by-side preview showed the isosurface in particular reading as
// noticeably more dimensional under GGX (a defined, roughness-shaped
// highlight instead of a flatter response on a large curved surface) —
// see `Material`'s doc. No separate ad hoc Fresnel rim needed on top of
// this the way Blinn-Phong had one: GGX's own Fresnel term already
// brightens grazing angles correctly as part of the specular response.
const PI: f32 = 3.14159265359;

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denom * denom, 1e-6);
}

// Height-correlated Smith visibility (Filament/Frostbite's formulation),
// not the separable Schlick-GGX approximation this started with — it
// already folds in the `1 / (4 * n_dot_v * n_dot_l)` term from the
// specular denominator, and does so in a way that stays numerically
// bounded as either angle approaches grazing, instead of the naive
// separable version's denominator collapsing toward zero there. Found by
// a real hands-on report (2026-08-29): the isosurface showed an
// occasional too-bright, partly desaturated hotspot depending on the
// exact camera/light geometry — consistent with exactly this class of
// grazing-angle spike, not (just) the missing AO dampening fixed
// alongside this.
fn visibility_smith_ggx_correlated(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a2 = roughness * roughness * roughness * roughness;
    let ggx_v = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    let ggx_l = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(ggx_v + ggx_l, 1e-5);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Direct light from the one key light: GGX specular plus energy-conserving
// diffuse, in linear light. `material` is (ambient, roughness, reflectance
// F0, light intensity) — the layout of both the atom/bond material and
// the isosurface material. Callers add their own ambient term.
fn ggx_direct(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, albedo: vec3<f32>, material: vec4<f32>) -> vec3<f32> {
    let h = normalize(v + l);

    let n_dot_v = max(dot(n, v), 1e-4);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let roughness = material.y;
    let f0 = vec3<f32>(material.z);

    let d = distribution_ggx(n_dot_h, roughness);
    let vis = visibility_smith_ggx_correlated(n_dot_v, n_dot_l, roughness);
    let f = fresnel_schlick(v_dot_h, f0);

    let specular = d * vis * f;
    // Energy conservation: light not reflected specularly (1 - F) is
    // what's left over for diffuse — the real reason a rough dielectric's
    // diffuse dims slightly at grazing angles, where F is largest.
    let kd = vec3<f32>(1.0) - f;
    let diffuse = kd * albedo / PI;

    return (diffuse + specular) * n_dot_l * material.w;
}

// ---- Translucent clouds (envelopes and atom halos) --------------------
// Soft lighting: wrapped diffuse (no hard terminator) plus a faint sheen,
// enough to read as a rounded volume without competing with the atoms'
// own highlights. Linear light; callers pass it through `finalize_color`.
fn cloud_color(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let wrap = clamp((dot(n, l) + 0.6) / 1.6, 0.0, 1.0);
    let h = normalize(v + l);
    let sheen = pow(max(dot(n, h), 0.0), 40.0) * 0.12;
    return srgb_to_linear(color) * (0.55 + 0.55 * wrap) + vec3<f32>(sheen);
}

// ---- Depth to world ----------------------------------------------------
// The world-space point behind a depth-buffer value, for full-screen and
// screen-space passes: `pixel_uv` is in [0, 1] with y down, `depth` the
// stored NDC depth.
fn world_from_depth(inv_view_proj: mat4x4<f32>, pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = inv_view_proj * ndc;
    return world4.xyz / world4.w;
}
