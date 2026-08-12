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

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct InstanceInput {
    @location(2) center: vec3<f32>,
    @location(3) length: f32,
    @location(4) axis: vec3<f32>,
    @location(5) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
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

    let radius = scene.style.y;
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
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let light_dir = normalize(scene.light_dir.xyz);
    let view_dir = normalize(scene.camera_eye.xyz - in.world_position);
    let half_dir = normalize(light_dir + view_dir);

    let ambient = scene.material.x;
    let diffuse_strength = scene.material.y * max(dot(normal, light_dir), 0.0);
    let specular_strength = scene.material.z * pow(max(dot(normal, half_dir), 0.0), scene.material.w);

    let lit_color = in.color * (ambient + diffuse_strength) + vec3<f32>(specular_strength);
    return vec4<f32>(lit_color, 1.0);
}
