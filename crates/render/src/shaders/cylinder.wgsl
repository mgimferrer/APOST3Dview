struct SceneUniforms {
    view_proj: mat4x4<f32>,
    camera_eye: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    light_dir: vec4<f32>,
    // ambient, roughness, reflectance (F0), light_intensity
    material: vec4<f32>,
    // atom_scale, bond_radius, exposure, srgb_target
    style: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

// Ambient occlusion — see the identical block in `sphere.wgsl` for the
// full explanation of why this is sampled here rather than composited in
// a separate pass. Only bound by `fs_main_ao`; `fs_main`/`fs_highlight`
// never touch group 1.
struct AoSampleUniforms {
    inv_view_proj: mat4x4<f32>,
    // strength, outline_strength, unused, unused
    params: vec4<f32>,
    // width, height, unused, unused — of the AO/depth textures, i.e. the
    // live viewport's own size, not the full window
    screen: vec4<f32>,
    // x, y, unused, unused — see the Rust-side doc on
    // `AoSampleUniforms::offset` for why this exists at all
    offset: vec4<f32>,
};

@group(1) @binding(0) var<uniform> ao_sample: AoSampleUniforms;
@group(1) @binding(1) var ao_depth: texture_depth_2d;
@group(1) @binding(2) var ao_texture: texture_2d<f32>;

fn ao_reconstruct_world(pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = ao_sample.inv_view_proj * ndc;
    return world4.xyz / world4.w;
}

// `frag_coord` is relative to the whole shared render target (the full
// window for the live view), not this AO texture's own viewport-sized
// coordinates — see the matching comment in `sphere.wgsl`'s `apply_ao`.
//
// `AO_SHADE_FLOOR` — see `sphere.wgsl` for why this exists: without it,
// broad moderately-occluded regions on a real crowded molecule (not just
// the deep crevices this is meant to darken) crush toward near-black.
const AO_SHADE_FLOOR: f32 = 0.32;

fn apply_ao(lit_color: vec3<f32>, frag_coord: vec4<f32>, hit_point: vec3<f32>) -> vec3<f32> {
    let local_coord = frag_coord.xy - ao_sample.offset.xy;
    let pixel = vec2<i32>(local_coord);
    let width = i32(ao_sample.screen.x);
    let height = i32(ao_sample.screen.y);
    let ao_value = max(textureLoad(ao_texture, pixel, 0).r, AO_SHADE_FLOOR);
    let shade_factor = mix(1.0, ao_value, ao_sample.params.x);

    let uv = local_coord / ao_sample.screen.xy;
    let dist0 = length(hit_point - scene.camera_eye.xyz);
    var edge = 0.0;
    if (pixel.x + 1 < width) {
        let d1 = textureLoad(ao_depth, pixel + vec2<i32>(1, 0), 0);
        if (d1 < 0.9999) {
            let world1 = ao_reconstruct_world(uv + vec2<f32>(1.0 / ao_sample.screen.x, 0.0), d1);
            edge = max(edge, abs(length(world1 - scene.camera_eye.xyz) - dist0));
        }
    }
    if (pixel.y + 1 < height) {
        let d1 = textureLoad(ao_depth, pixel + vec2<i32>(0, 1), 0);
        if (d1 < 0.9999) {
            let world1 = ao_reconstruct_world(uv + vec2<f32>(0.0, 1.0 / ao_sample.screen.y), d1);
            edge = max(edge, abs(length(world1 - scene.camera_eye.xyz) - dist0));
        }
    }
    let outline = pow(clamp(1.0 - edge * 8.0, 0.0, 1.0), max(ao_sample.params.y, 0.001));
    return lit_color * shade_factor * outline;
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct InstanceInput {
    @location(2) center: vec3<f32>,
    @location(3) length: f32,
    @location(4) axis: vec3<f32>,
    // 0.0 = solid, 1.0 = dashed (transition-state bond)
    @location(5) dashed: f32,
    @location(6) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // Distance along the bond axis from this segment's start, in
    // angstrom — used to build a dash pattern with consistent physical
    // spacing regardless of zoom or bond length.
    @location(3) distance_along: f32,
    @location(4) dashed: f32,
};

@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    let axis = normalize(instance.axis);
    var reference = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(dot(axis, reference)) > 0.99) {
        reference = vec3<f32>(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(reference, axis));
    let bitangent = cross(axis, tangent);

    // Transition-state bonds render much thinner than a real bond — reads
    // as a dashed line rather than a stick, matching the CYLview/textbook
    // convention for a forming/breaking bond.
    let radius = select(scene.style.y, scene.style.y * 0.3, instance.dashed > 0.5);
    let world_position = instance.center
        + tangent * in.position.x * radius
        + axis * in.position.y * instance.length
        + bitangent * in.position.z * radius;
    let world_normal = normalize(tangent * in.normal.x + bitangent * in.normal.z);

    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.world_normal = world_normal;
    out.color = instance.color;
    out.distance_along = (in.position.y + 0.5) * instance.length;
    out.dashed = instance.dashed;
    return out;
}

// Discards fragments in the "gap" of a dash pattern with a fixed physical
// period, so dash spacing looks the same regardless of zoom or bond length.
fn apply_dash(distance_along: f32, dashed: f32) {
    if (dashed > 0.5) {
        let period = 0.35;
        let duty_cycle = 0.55;
        if (fract(distance_along / period) > duty_cycle) {
            discard;
        }
    }
}

// ---- Color pipeline: sRGB<->linear, hemisphere fill light, Fresnel,
// filmic tone mapping. Identical to `sphere.wgsl`'s copy — see that
// file's comment for why this is duplicated rather than shared.

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

fn aces_tonemap(c: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let cc = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((c * (a * c + vec3<f32>(b))) / (c * (cc * c + vec3<f32>(d)) + vec3<f32>(e)), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn hemisphere_ambient(normal: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(1.05, 1.05, 1.1);
    let ground = vec3<f32>(0.65, 0.62, 0.6);
    return mix(ground, sky, normal.y * 0.5 + 0.5);
}

fn finalize_color(linear_color: vec3<f32>) -> vec3<f32> {
    let mapped = aces_tonemap(linear_color * scene.style.z);
    return select(linear_to_srgb(mapped), mapped, scene.style.w > 0.5);
}

// ---- Cook-Torrance/GGX BRDF — identical to `sphere.wgsl`'s copy, see
// that file's comment for the full explanation and why this replaced
// Blinn-Phong.
const PI: f32 = 3.14159265359;

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denom * denom, 1e-6);
}

// Height-correlated Smith visibility — identical to `sphere.wgsl`'s copy,
// see that file's comment for why this replaced the separable Schlick-GGX
// approximation.
fn visibility_smith_ggx_correlated(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let a2 = roughness * roughness * roughness * roughness;
    let ggx_v = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    let ggx_l = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(ggx_v + ggx_l, 1e-5);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Shared by `fs_main` and `fs_main_ao` — kept as one function so the two
// entry points can never drift apart on the base lighting, only on
// whether `apply_ao` runs afterward. Returns *linear-light* color, not a
// final pixel value — callers must pass it through `finalize_color`.
fn shade(world_position: vec3<f32>, normal: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let n = normal;
    let v = normalize(scene.camera_eye.xyz - world_position);
    let l = normalize(scene.light_dir.xyz);
    let h = normalize(v + l);

    let n_dot_v = max(dot(n, v), 1e-4);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let roughness = scene.material.y;
    let f0 = vec3<f32>(scene.material.z);
    let albedo = srgb_to_linear(color);

    let d = distribution_ggx(n_dot_h, roughness);
    let vis = visibility_smith_ggx_correlated(n_dot_v, n_dot_l, roughness);
    let f = fresnel_schlick(v_dot_h, f0);

    let specular = d * vis * f;
    let kd = vec3<f32>(1.0) - f;
    let diffuse = kd * albedo / PI;

    let direct = (diffuse + specular) * n_dot_l * scene.material.w;
    let ambient = hemisphere_ambient(n) * albedo * scene.material.x;
    return direct + ambient;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    apply_dash(in.distance_along, in.dashed);
    let normal = normalize(in.world_normal);
    let lit_color = shade(in.world_position, normal, in.color);
    return vec4<f32>(finalize_color(lit_color), 1.0);
}

// Same as `fs_main`, plus ambient occlusion + outline sampled from the
// precomputed textures at group 1 — see `apply_ao` above.
@fragment
fn fs_main_ao(in: VertexOutput) -> @location(0) vec4<f32> {
    apply_dash(in.distance_along, in.dashed);
    let normal = normalize(in.world_normal);
    var lit_color = shade(in.world_position, normal, in.color);
    lit_color = apply_ao(lit_color, in.clip_position, in.world_position);
    return vec4<f32>(finalize_color(lit_color), 1.0);
}

// Selection-highlight pass: same geometry, no lighting — a flat
// translucent tint layered on top of the normal opaque render.
@fragment
fn fs_highlight(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 0.35);
}

// Ambient-occlusion G-buffer pass (export-only, see `ao.rs`): outputs
// world-space normal instead of shaded color, so a later full-screen pass
// can compute real per-pixel occlusion. No `frag_depth` override needed —
// this is a real rasterized mesh, so the hardware-interpolated depth
// already matches `fs_main` exactly. Dash cutouts still apply, so the
// G-buffer agrees with the main pass about which pixels are actually bond,
// not gap.
@fragment
fn fs_gbuffer(in: VertexOutput) -> @location(0) vec4<f32> {
    apply_dash(in.distance_along, in.dashed);
    let normal = normalize(in.world_normal);
    return vec4<f32>(normal, 1.0);
}
