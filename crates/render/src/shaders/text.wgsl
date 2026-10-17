// Labels: camera-facing glyph quads sampling the signed-distance-field
// atlas. Assembled after `common.wgsl` and `scene.wgsl` (see
// `shaders.rs`).

@group(1) @binding(0)
var glyph_texture: texture_2d<f32>;
@group(1) @binding(1)
var glyph_sampler: sampler;

struct InstanceInput {
    @location(0) label_anchor: vec3<f32>,
    @location(1) local_offset: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
    @location(5) color: vec3<f32>,
    @location(6) edge_bias: f32,
    @location(7) halo_color: vec3<f32>,
    @location(8) halo_width: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) edge_bias: f32,
    @location(3) halo_color: vec3<f32>,
    @location(4) halo_width: f32,
};

// Camera-facing quad, same procedural-corner trick as the atom impostors —
// no vertex buffer needed for the quad shape itself.
const CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);
// Texture V is top-down; quad corners are bottom-up — flip.
const UVS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: InstanceInput) -> VertexOutput {
    let corner = CORNERS[vertex_index];
    let world_position = instance.label_anchor
        + scene.camera_right.xyz * (instance.local_offset.x + corner.x * instance.half_size.x)
        + scene.camera_up.xyz * (instance.local_offset.y + corner.y * instance.half_size.y);

    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(world_position, 1.0);
    out.uv = mix(instance.uv_min, instance.uv_max, UVS[vertex_index]);
    out.color = instance.color;
    out.edge_bias = instance.edge_bias;
    out.halo_color = instance.halo_color;
    out.halo_width = instance.halo_width;
    return out;
}

// The glyph texture stores a signed distance field, not raw coverage: 0.5
// sits exactly on the glyph edge. Thresholding it with smoothstep, sized
// by the texture's own screen-space rate of change (fwidth), reconstructs
// a crisp edge at any zoom level — magnifying a fixed-resolution coverage
// bitmap directly (the previous approach) went blurry/blocky once a label
// covered much more than the atlas's native raster resolution on screen.
//
// Alpha-to-coverage (see the pipeline's MultisampleState) rather than
// traditional blending: each MSAA sample either fully passes or is
// discarded based on the computed coverage, giving smooth antialiased
// edges while still writing real depth — so labels are correctly,
// precisely occluded by anything nearer the camera, including partially
// (a bond crossing over just part of a label only covers that part), not
// the all-or-nothing a 2D screen overlay could manage.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sdf = textureSample(glyph_texture, glyph_sampler, in.uv).r;
    let aa = max(fwidth(sdf), 0.0001);
    let coverage = smoothstep(in.edge_bias - aa, in.edge_bias + aa, sdf);
    // The halo is the same field thresholded further out: text color
    // inside the glyph, halo color in the band around it.
    let halo_edge = in.edge_bias - in.halo_width;
    let halo_coverage = smoothstep(halo_edge - aa, halo_edge + aa, sdf);
    let color = mix(in.halo_color, in.color, coverage);
    return vec4<f32>(color, max(coverage, halo_coverage));
}
