// Shared shader code, part 2: the scene uniforms (camera, light, atom/bond
// material, style) at group 0, and what is computed from them. Prepended,
// after `common.wgsl`, to every shader that draws the scene itself.

struct SceneUniforms {
    view_proj: mat4x4<f32>,
    // w: 1.0 for an orthographic camera (parallel rays), 0.0 perspective
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

// The projection. Orthographic: every pixel's ray runs along the view
// direction; the eye in `camera_eye` is only a far point behind the scene
// (for depth order), so directions are taken from the view, not from it.
fn is_orthographic() -> bool {
    return scene.camera_eye.w > 0.5;
}

// Into the screen: up x right (right x up points at the viewer).
fn view_forward() -> vec3<f32> {
    return cross(scene.camera_up.xyz, scene.camera_right.xyz);
}

// Unit vector from `p` toward the viewer.
fn to_viewer(p: vec3<f32>) -> vec3<f32> {
    if (is_orthographic()) {
        return -view_forward();
    }
    return normalize(scene.camera_eye.xyz - p);
}

struct ViewRay {
    origin: vec3<f32>,
    dir: vec3<f32>,
};

// The ray of the pixel that sees `p` (a point on an impostor's billboard):
// from the eye, or, orthographic, along the view direction from `back`
// in front of `p` (enough to start in front of the impostor's shape).
fn view_ray(p: vec3<f32>, back: f32) -> ViewRay {
    if (is_orthographic()) {
        let dir = view_forward();
        return ViewRay(p - dir * back, dir);
    }
    let origin = scene.camera_eye.xyz;
    return ViewRay(origin, normalize(p - origin));
}

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
    let v = to_viewer(world_position);
    let l = normalize(scene.light_dir.xyz);
    let albedo = srgb_to_linear(color);
    let direct = ggx_direct(normal, v, l, albedo, scene.material);
    let ambient = hemisphere_ambient(normal, scene.world_up.xyz) * albedo * scene.material.x;
    return direct + ambient;
}
