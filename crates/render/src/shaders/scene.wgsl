// Shared shader code, part 2: the scene uniforms (camera, light, atom/bond
// material, style) at group 0, and what is computed from them. Prepended,
// after `common.wgsl`, to every shader that draws the scene itself.

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
    // "up" for the sky/ground ambient (world +Y unless aligned)
    world_up: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

// Exposure -> filmic tone map -> sRGB encode (only when the render target
// itself won't do that encode automatically — see `SceneUniforms::
// set_srgb_target`). The one place every fragment shader's lit color
// should pass through right before being written out.
fn finalize_color(linear_color: vec3<f32>) -> vec3<f32> {
    let mapped = aces_tonemap(linear_color * scene.style.z);
    return select(linear_to_srgb(mapped), mapped, scene.style.w > 0.5);
}

// Atoms and bonds: GGX direct light plus hemisphere ambient, with the
// atom/bond material. Shared by the plain and ambient-occlusion entry
// points so they can never drift apart on the base lighting, only on
// whether `apply_ao` runs afterward. Returns *linear-light* color, not a
// final pixel value — callers must pass it through `finalize_color`.
fn shade(world_position: vec3<f32>, normal: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let v = normalize(scene.camera_eye.xyz - world_position);
    let l = normalize(scene.light_dir.xyz);
    let albedo = srgb_to_linear(color);
    let direct = ggx_direct(normal, v, l, albedo, scene.material);
    let ambient = hemisphere_ambient(normal, scene.world_up.xyz) * albedo * scene.material.x;
    return direct + ambient;
}
