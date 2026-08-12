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
};

@group(1) @binding(0)
var<uniform> iso_material: IsosurfaceMaterial;

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

// Ordinary rasterized/lit triangle mesh — unlike the raymarched sphere and
// cylinder impostors elsewhere in this renderer, an isosurface really is
// a polygon mesh (from marching tetrahedra), so there's no analytic
// surface to ray-intersect and no need to override the fragment depth;
// the rasterizer's own interpolated depth is exactly correct already.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Not all triangles are guaranteed consistently wound (marching
    // tetrahedra prioritizes simplicity over exact winding — see the core
    // crate's isosurface module), so light both faces the same way by
    // flipping the normal to face the camera when needed, rather than
    // relying on `front_facing` winding-based logic.
    var normal = normalize(in.normal);
    let view_dir = normalize(scene.camera_eye.xyz - in.world_position);
    if (dot(normal, view_dir) < 0.0) {
        normal = -normal;
    }

    let light_dir = normalize(scene.light_dir.xyz);
    let half_dir = normalize(light_dir + view_dir);

    let ambient = iso_material.material.x;
    let diffuse_strength = iso_material.material.y * max(dot(normal, light_dir), 0.0);
    let specular_strength = iso_material.material.z * pow(max(dot(normal, half_dir), 0.0), iso_material.material.w);
    let lit_color = in.color * (ambient + diffuse_strength) + vec3<f32>(specular_strength);

    return vec4<f32>(lit_color, in.opacity);
}
