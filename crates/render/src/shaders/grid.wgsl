struct SceneUniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    // ambient, diffuse, specular, shininess
    material: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(in.position, 1.0);
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    // Ambient slider modulates grid brightness so the side panel visibly
    // does something before real shaded geometry exists.
    let brightness = clamp(scene.material.x, 0.0, 1.0);
    let color = vec3<f32>(0.35, 0.4, 0.48) * (0.4 + brightness);
    return vec4<f32>(color, 1.0);
}
