// Bonds (and measurement lines): a real cylinder mesh, instanced per
// bond, solid or dashed, plus its selection highlight and ambient-
// occlusion G-buffer pass. Assembled after `common.wgsl`, `scene.wgsl` and
// `ao_sample.wgsl` (see `shaders.rs`).

// Ambient occlusion, read by `apply_ao` (see `ao_sample.wgsl`).
@group(1) @binding(0) var<uniform> ao_sample: AoSampleUniforms;
@group(1) @binding(1) var ao_depth: texture_depth_2d;
@group(1) @binding(2) var ao_texture: texture_2d<f32>;

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
    // Multiplies the bond radius (bond-order-driven display).
    @location(7) radius_scale: f32,
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
    let base_radius = scene.style.y * instance.radius_scale;
    let radius = select(base_radius, base_radius * 0.3, instance.dashed > 0.5);
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    apply_dash(in.distance_along, in.dashed);
    let normal = normalize(in.world_normal);
    let lit_color = shade(in.world_position, normal, in.color);
    return vec4<f32>(finalize_color(lit_color), 1.0);
}

// Same as `fs_main`, plus ambient occlusion + outline sampled from the
// precomputed textures at group 1 — see `apply_ao` (`ao_sample.wgsl`).
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
