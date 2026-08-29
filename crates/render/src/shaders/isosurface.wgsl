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

// Isosurface-only lighting response, deliberately separate from the
// atom/bond material above so tuning one never touches the other.
struct IsosurfaceMaterial {
    // ambient, diffuse, specular, shininess
    material: vec4<f32>,
    // fresnel power, fresnel/rim strength, unused, unused
    fresnel: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> iso_material: IsosurfaceMaterial;

// Ambient occlusion (see `ao.rs`/`ao.wgsl`, and the identical group-1 block
// in `sphere.wgsl`): bound at group 2 here rather than group 1, since group
// 1 is already the isosurface material above — only the `_ao` pipeline
// variant binds this at all.
struct AoSampleUniforms {
    inv_view_proj: mat4x4<f32>,
    params: vec4<f32>,
    screen: vec4<f32>,
    offset: vec4<f32>,
};

@group(2) @binding(0) var<uniform> ao_sample: AoSampleUniforms;
@group(2) @binding(1) var ao_depth: texture_depth_2d;
@group(2) @binding(2) var ao_texture: texture_2d<f32>;

fn ao_reconstruct_world(pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = ao_sample.inv_view_proj * ndc;
    return world4.xyz / world4.w;
}

const AO_SHADE_FLOOR: f32 = 0.32;

// Same shape as `sphere.wgsl`'s `apply_ao` — see that file for the full
// explanation of the world-space outline comparison and the shade floor.
// The isosurface itself now also writes into the same G-buffer this
// samples (see `fs_gbuffer` below), so it both occludes and receives
// contact shading against atoms/bonds and against itself (e.g. where a
// lobe wraps around an atom, or two lobes meet).
fn apply_ao(lit_color: vec3<f32>, frag_coord: vec4<f32>, hit_point: vec3<f32>) -> vec3<f32> {
    let local_coord = frag_coord.xy - ao_sample.offset.xy;
    let pixel = vec2<i32>(local_coord);
    let width = i32(ao_sample.screen.x);
    let height = i32(ao_sample.screen.y);
    let ao_value = max(textureLoad(ao_texture, pixel, 0).r, AO_SHADE_FLOOR);
    let shade = mix(1.0, ao_value, ao_sample.params.x);

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
    return lit_color * shade * outline;
}

// ---- Color pipeline: identical to `sphere.wgsl`'s copy — see that file's
// comment for why this is duplicated rather than shared.

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

fn fresnel_schlick(n_dot_v: f32, power: f32) -> f32 {
    return pow(clamp(1.0 - n_dot_v, 0.0, 1.0), power);
}

fn finalize_color(linear_color: vec3<f32>) -> vec3<f32> {
    let mapped = aces_tonemap(linear_color * scene.style.z);
    return select(linear_to_srgb(mapped), mapped, scene.style.w > 0.5);
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
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

// Shared by `fs_main` and `fs_main_ao` (and used directly by `shade_only`,
// below, for `fs_gbuffer`'s normal-facing check) — not all triangles are
// guaranteed consistently wound (marching tetrahedra prioritizes
// simplicity over exact winding — see the core crate's isosurface
// module), so light both faces the same way by flipping the normal to
// face the camera when needed, rather than relying on `front_facing`
// winding-based logic.
fn facing_normal(raw_normal: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    var normal = normalize(raw_normal);
    let view_dir = normalize(scene.camera_eye.xyz - world_position);
    if (dot(normal, view_dir) < 0.0) {
        normal = -normal;
    }
    return normal;
}

// Blinn-Phong (hemisphere fill + Fresnel rim glow, tinted toward the
// surface's own color rather than white — the "glowing energy field"
// look, cheap and a big part of what sells a translucent probability
// cloud rather than a solid). Returns linear-light color; callers must
// pass it through `finalize_color`.
fn shade(world_position: vec3<f32>, normal: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let light_dir = normalize(scene.light_dir.xyz);
    let view_dir = normalize(scene.camera_eye.xyz - world_position);
    let half_dir = normalize(light_dir + view_dir);

    let albedo = srgb_to_linear(color);
    let ambient = iso_material.material.x * hemisphere_ambient(normal);
    let diffuse_strength = iso_material.material.y * max(dot(normal, light_dir), 0.0);
    let specular_strength = iso_material.material.z * pow(max(dot(normal, half_dir), 0.0), iso_material.material.w);
    let fresnel = fresnel_schlick(max(dot(normal, view_dir), 0.0), iso_material.fresnel.x) * iso_material.fresnel.y;
    return albedo * (ambient + diffuse_strength + fresnel) + vec3<f32>(specular_strength);
}

// Ordinary rasterized/lit triangle mesh — unlike the raymarched sphere and
// cylinder impostors elsewhere in this renderer, an isosurface really is
// a polygon mesh (Surface Nets), so there's no analytic surface to
// ray-intersect and no need to override the fragment depth; the
// rasterizer's own interpolated depth is exactly correct already.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = facing_normal(in.normal, in.world_position);
    let lit_color = shade(in.world_position, normal, in.color);
    return vec4<f32>(finalize_color(lit_color), in.opacity);
}

// Same as `fs_main`, plus ambient occlusion + outline sampled from the
// precomputed textures at group 2 — see `apply_ao` above.
@fragment
fn fs_main_ao(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = facing_normal(in.normal, in.world_position);
    var lit_color = shade(in.world_position, normal, in.color);
    lit_color = apply_ao(lit_color, in.clip_position, in.world_position);
    return vec4<f32>(finalize_color(lit_color), in.opacity);
}

// Ambient-occlusion G-buffer pass: outputs world-space normal instead of
// shaded color, exactly like the atom/cylinder `fs_gbuffer` entry points,
// so the isosurface participates in AO as both an occluder (atoms/bonds
// wrapped by a lobe correctly darken) and a receiver (`fs_main_ao` above).
// Depth write behaves the same as `fs_main`'s (real rasterized depth, no
// override) — for G-buffer purposes a translucent lobe is treated as an
// opaque occluder, the same simplification any screen-space AO scheme
// makes for every occluder, translucent or not.
@fragment
fn fs_gbuffer(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = facing_normal(in.normal, in.world_position);
    return vec4<f32>(normal, 1.0);
}
