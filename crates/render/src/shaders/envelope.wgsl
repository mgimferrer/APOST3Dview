// Envelopes: soft translucent clouds around a fragment (or an atom),
// colored by the fragment's identity or by a value, drawn over the molecule
// so atoms keep their element colors underneath.
//
// Drawn after everything opaque, without writing depth and outside the
// ambient-occlusion pass, so a cloud never darkens or hides what is inside
// it. Only the side facing the camera is drawn (the surface normals point
// outward, so a back-facing fragment is discarded): one clean translucent
// layer per envelope, with no front/back interleaving. Envelopes are drawn
// farthest first (see `ViewportResources::draw_envelopes`).
//
// The opacity rises toward the silhouette: the middle stays clear enough
// to see the atoms' true colors, while the edge carries the cloud's color
// and shape.

struct SceneUniforms {
    view_proj: mat4x4<f32>,
    camera_eye: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    light_dir: vec4<f32>,
    material: vec4<f32>,
    style: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // Strength of this envelope (0..1): scales the whole opacity profile.
    @location(3) opacity: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) opacity: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.normal = in.normal;
    out.color = in.color;
    out.opacity = in.opacity;
    return out;
}

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

fn finalize_color(linear_color: vec3<f32>) -> vec3<f32> {
    let mapped = aces_tonemap(linear_color * scene.style.z);
    return select(linear_to_srgb(mapped), mapped, scene.style.w > 0.5);
}

// Opacity at the center of the cloud and at its silhouette, as fractions
// of the envelope's strength, and how quickly it rises between them.
const CENTER_OPACITY: f32 = 0.26;
const EDGE_OPACITY: f32 = 1.0;
const EDGE_POWER: f32 = 2.2;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = normalize(scene.camera_eye.xyz - in.world_position);
    let n_dot_v = dot(n, v);
    if (n_dot_v <= 0.0) {
        discard;
    }

    // Soft lighting: wrapped diffuse (no hard terminator) plus a faint
    // sheen, enough to read as a rounded volume without competing with the
    // atoms' own highlights.
    let l = normalize(scene.light_dir.xyz);
    let wrap = clamp((dot(n, l) + 0.6) / 1.6, 0.0, 1.0);
    let h = normalize(v + l);
    let sheen = pow(max(dot(n, h), 0.0), 40.0) * 0.12;
    let albedo = srgb_to_linear(in.color);
    let lit = albedo * (0.55 + 0.55 * wrap) + vec3<f32>(sheen);

    let edge = pow(clamp(1.0 - n_dot_v, 0.0, 1.0), EDGE_POWER);
    let alpha = in.opacity * mix(CENTER_OPACITY, EDGE_OPACITY, edge);
    return vec4<f32>(finalize_color(lit), clamp(alpha, 0.0, 1.0));
}
