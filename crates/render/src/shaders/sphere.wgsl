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

struct InstanceInput {
    @location(0) center: vec3<f32>,
    @location(1) vdw_radius: f32,
    @location(2) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) center: vec3<f32>,
    @location(2) radius: f32,
    @location(3) color: vec3<f32>,
};

// A camera-facing quad, generated procedurally (two triangles, no vertex
// buffer needed) — the fragment shader below ray-traces the actual sphere
// surface within it, so this quad just needs to bound the sphere's
// silhouette.
const CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: InstanceInput) -> VertexOutput {
    let radius = instance.vdw_radius * scene.style.x;
    let corner = CORNERS[vertex_index];
    let world_position = instance.center
        + scene.camera_right.xyz * corner.x * radius
        + scene.camera_up.xyz * corner.y * radius;

    var out: VertexOutput;
    out.clip_position = scene.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.center = instance.center;
    out.radius = radius;
    out.color = instance.color;
    return out;
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);

    // Analytic ray-sphere intersection: exact round silhouette and normal
    // at any zoom, no polygon faceting.
    let oc = ray_origin - in.center;
    let b = dot(oc, ray_dir);
    let c = dot(oc, oc) - in.radius * in.radius;
    let discriminant = b * b - c;
    if (discriminant < 0.0) {
        discard;
    }
    let sqrt_disc = sqrt(discriminant);
    var t = -b - sqrt_disc;
    if (t < 0.0) {
        t = -b + sqrt_disc;
    }
    if (t < 0.0) {
        discard;
    }

    let hit_point = ray_origin + t * ray_dir;
    let normal = normalize(hit_point - in.center);

    let light_dir = normalize(scene.light_dir.xyz);
    let view_dir = normalize(ray_origin - hit_point);
    let half_dir = normalize(light_dir + view_dir);

    let ambient = scene.material.x;
    let diffuse_strength = scene.material.y * max(dot(normal, light_dir), 0.0);
    let specular_strength = scene.material.z * pow(max(dot(normal, half_dir), 0.0), scene.material.w);
    let lit_color = in.color * (ambient + diffuse_strength) + vec3<f32>(specular_strength);

    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);
    let ndc_depth = clip.z / clip.w;

    var out: FragmentOutput;
    out.color = vec4<f32>(lit_color, 1.0);
    out.depth = ndc_depth;
    return out;
}
