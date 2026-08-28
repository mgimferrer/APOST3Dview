struct SceneUniforms {
    view_proj: mat4x4<f32>,
    camera_eye: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    light_dir: vec4<f32>,
    // ambient, diffuse, specular, shininess
    material: vec4<f32>,
    // atom_scale, bond_radius, unused, unused
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

// Blinn-Phong shading shared by `fs_main` and `fs_main_ao` — kept as one
// function so the two entry points can never drift apart on the base
// lighting, only on whether `apply_ao` runs afterward.
fn shade(world_position: vec3<f32>, normal: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let light_dir = normalize(scene.light_dir.xyz);
    let view_dir = normalize(scene.camera_eye.xyz - world_position);
    let half_dir = normalize(light_dir + view_dir);

    let ambient = scene.material.x;
    let diffuse_strength = scene.material.y * max(dot(normal, light_dir), 0.0);
    let specular_strength = scene.material.z * pow(max(dot(normal, half_dir), 0.0), scene.material.w);
    return color * (ambient + diffuse_strength) + vec3<f32>(specular_strength);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    apply_dash(in.distance_along, in.dashed);
    let normal = normalize(in.world_normal);
    let lit_color = shade(in.world_position, normal, in.color);
    return vec4<f32>(lit_color, 1.0);
}

// Same as `fs_main`, plus ambient occlusion + outline sampled from the
// precomputed textures at group 1 — see `apply_ao` above.
@fragment
fn fs_main_ao(in: VertexOutput) -> @location(0) vec4<f32> {
    apply_dash(in.distance_along, in.dashed);
    let normal = normalize(in.world_normal);
    var lit_color = shade(in.world_position, normal, in.color);
    lit_color = apply_ao(lit_color, in.clip_position, in.world_position);
    return vec4<f32>(lit_color, 1.0);
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
