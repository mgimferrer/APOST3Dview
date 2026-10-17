// Isosurfaces (orbitals, densities, cube files): an ordinary rasterized,
// lit triangle mesh with its own material. Assembled after `common.wgsl`,
// `scene.wgsl` and `ao_sample.wgsl` (see `shaders.rs`).

// Isosurface-only lighting response, deliberately separate from the
// atom/bond material (`scene.material`) so tuning one never touches the
// other.
struct IsosurfaceMaterial {
    // ambient, roughness, reflectance (F0), light_intensity
    material: vec4<f32>,
    // fresnel power, fresnel/rim strength, unused, unused
    fresnel: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> iso_material: IsosurfaceMaterial;

// Ambient occlusion, read by `apply_ao` (see `ao_sample.wgsl`): bound at
// group 2 here rather than group 1, since group 1 is already the
// isosurface material above — only the `_ao` pipeline variant binds this
// at all.
@group(2) @binding(0) var<uniform> ao_sample: AoSampleUniforms;
@group(2) @binding(1) var ao_depth: texture_depth_2d;
@group(2) @binding(2) var ao_texture: texture_2d<f32>;

// The artistic rim-glow term (see `shade_isosurface` below) — scalar
// power-based, distinct from `fresnel_schlick`'s real F0-based Fresnel
// used inside the GGX BRDF itself.
fn fresnel_rim(n_dot_v: f32, power: f32) -> f32 {
    return pow(clamp(1.0 - n_dot_v, 0.0, 1.0), power);
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

// GGX (plus hemisphere fill + an artistic rim glow tinted toward the
// surface's own color, on top of the BRDF's own physically-correct
// Fresnel — the "glowing energy field" look, meant to read as light
// escaping a translucent field rather than surface reflectance, which is
// why it's added to the ambient term here rather than folded into the
// BRDF's Fresnel). Returns linear-light color; callers must pass it
// through `finalize_color`.
fn shade_isosurface(world_position: vec3<f32>, normal: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let v = normalize(scene.camera_eye.xyz - world_position);
    let l = normalize(scene.light_dir.xyz);
    let albedo = srgb_to_linear(color);
    let direct = ggx_direct(normal, v, l, albedo, iso_material.material);
    let n_dot_v = max(dot(normal, v), 1e-4);
    let rim = fresnel_rim(n_dot_v, iso_material.fresnel.x) * iso_material.fresnel.y;
    let ambient = hemisphere_ambient(normal) * albedo * (iso_material.material.x + rim);
    return direct + ambient;
}

// Ordinary rasterized/lit triangle mesh — unlike the raymarched sphere and
// cylinder impostors elsewhere in this renderer, an isosurface really is
// a polygon mesh (Surface Nets), so there's no analytic surface to
// ray-intersect and no need to override the fragment depth; the
// rasterizer's own interpolated depth is exactly correct already.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = facing_normal(in.normal, in.world_position);
    let lit_color = shade_isosurface(in.world_position, normal, in.color);
    return vec4<f32>(finalize_color(lit_color), in.opacity);
}

// Same as `fs_main`, plus ambient occlusion + outline sampled from the
// precomputed textures at group 2 — see `apply_ao` (`ao_sample.wgsl`).
@fragment
fn fs_main_ao(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = facing_normal(in.normal, in.world_position);
    var lit_color = shade_isosurface(in.world_position, normal, in.color);
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
