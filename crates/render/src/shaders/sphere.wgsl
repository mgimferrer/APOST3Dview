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

    // The billboard plane must be perpendicular to the ray from the eye to
    // *this atom's own center*, not to the camera's shared forward axis —
    // for an atom off to the side of the screen those two directions
    // differ, and orienting the quad to the shared axis leaves it tilted
    // relative to the true silhouette cone, clipping part of the sphere
    // even after correcting the quad's size. So the basis is rebuilt here
    // per instance instead of reusing scene.camera_right/camera_up.
    let to_eye = scene.camera_eye.xyz - instance.center;
    let distance_to_eye = max(length(to_eye), 0.0001);
    let forward_to_eye = to_eye / distance_to_eye;
    var right = cross(forward_to_eye, scene.camera_up.xyz);
    let right_len = length(right);
    if (right_len < 0.0001) {
        // Degenerate only when this atom sits exactly along the camera's
        // own up axis — fall back to the shared basis for that instant.
        right = scene.camera_right.xyz;
    } else {
        right = right / right_len;
    }
    let up = cross(right, forward_to_eye);

    // A billboard quad sized to exactly `radius` only bounds the sphere's
    // silhouette under an orthographic camera. Under this perspective one,
    // the tangent lines from the eye to the sphere flare wider than that
    // the closer the eye gets — the true half-angle is asin(radius /
    // distance), so the quad needs to be sized radius / cos(that angle) =
    // radius / sqrt(1 - (radius/distance)^2) to fully contain it. Without
    // this, atoms near the camera (e.g. terminal atoms pointing at the
    // viewer) get their silhouette clipped by the quad's own edge.
    let ratio = clamp(radius / distance_to_eye, 0.0, 0.999);
    let quad_radius = radius / sqrt(1.0 - ratio * ratio);

    let corner = CORNERS[vertex_index];
    let world_position = instance.center
        + right * corner.x * quad_radius
        + up * corner.y * quad_radius;

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

// Selection-highlight pass: same silhouette (drawn at a slightly larger
// instance radius so it peeks out as a rim), no lighting — just a flat
// translucent tint layered on top of the normal opaque render.
@fragment
fn fs_highlight(in: VertexOutput) -> FragmentOutput {
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);

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
    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);
    let ndc_depth = clip.z / clip.w;

    var out: FragmentOutput;
    out.color = vec4<f32>(in.color, 0.35);
    out.depth = ndc_depth;
    return out;
}
