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
//
// Assembled after `common.wgsl` and `scene.wgsl` (see `shaders.rs`).

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

// Opacity at the center of the cloud and at its silhouette, as fractions
// of the envelope's strength, and how quickly it rises between them.
const CENTER_OPACITY: f32 = 0.26;
const EDGE_OPACITY: f32 = 1.0;
const EDGE_POWER: f32 = 2.2;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = to_viewer(in.world_position);
    let n_dot_v = dot(n, v);
    if (n_dot_v <= 0.0) {
        discard;
    }

    let lit = cloud_color(n, v, normalize(scene.light_dir.xyz), in.color);

    let edge = pow(clamp(1.0 - n_dot_v, 0.0, 1.0), EDGE_POWER);
    let alpha = in.opacity * mix(CENTER_OPACITY, EDGE_OPACITY, edge);
    return vec4<f32>(finalize_color(lit), clamp(alpha, 0.0, 1.0));
}
