// Atoms: raymarched sphere impostors (an exact sphere at any zoom, no
// polygon faceting), plus their selection highlight, ambient-occlusion
// G-buffer pass and translucent value halos. Assembled after
// `common.wgsl`, `scene.wgsl` and `ao_sample.wgsl` (see `shaders.rs`).

// Ambient occlusion, read by `apply_ao` (see `ao_sample.wgsl`).
@group(1) @binding(0) var<uniform> ao_sample: AoSampleUniforms;
@group(1) @binding(1) var ao_depth: texture_depth_2d;
@group(1) @binding(2) var ao_texture: texture_2d<f32>;

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
    var forward_to_eye = to_eye / distance_to_eye;
    if (is_orthographic()) {
        // Parallel rays: every billboard faces the view direction.
        forward_to_eye = -view_forward();
    }
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
    // Orthographic: the silhouette is exactly `radius` wide.
    let ratio = select(clamp(radius / distance_to_eye, 0.0, 0.999), 0.0, is_orthographic());
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

// Analytic ray-sphere intersection: exact round silhouette and normal at
// any zoom, no polygon faceting. Returns the hit point in `.xyz`; `.w` is
// 1.0 on a hit, 0.0 on a miss (WGSL has no Option type) — callers must
// check it before using `.xyz`, since a miss leaves it as whatever `t`
// happened to compute.
fn intersect_sphere(ray_origin: vec3<f32>, ray_dir: vec3<f32>, center: vec3<f32>, radius: f32) -> vec4<f32> {
    let oc = ray_origin - center;
    let b = dot(oc, ray_dir);
    let c = dot(oc, oc) - radius * radius;
    let discriminant = b * b - c;
    if (discriminant < 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let sqrt_disc = sqrt(discriminant);
    var t = -b - sqrt_disc;
    if (t < 0.0) {
        t = -b + sqrt_disc;
    }
    if (t < 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(ray_origin + t * ray_dir, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    let ray = view_ray(in.world_position, 2.0 * in.radius + 1.0);
    let ray_origin = ray.origin;
    let ray_dir = ray.dir;

    let hit = intersect_sphere(ray_origin, ray_dir, in.center, in.radius);
    if (hit.w < 0.5) {
        discard;
    }
    let hit_point = hit.xyz;
    let normal = normalize(hit_point - in.center);
    let lit_color = shade(hit_point, normal, in.color);

    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);
    let ndc_depth = clip.z / clip.w;

    var out: FragmentOutput;
    out.color = vec4<f32>(finalize_color(lit_color), 1.0);
    out.depth = ndc_depth;
    return out;
}

// Same as `fs_main`, plus ambient occlusion + outline sampled from the
// precomputed textures at group 1 — see `apply_ao` (`ao_sample.wgsl`).
@fragment
fn fs_main_ao(in: VertexOutput) -> FragmentOutput {
    let ray = view_ray(in.world_position, 2.0 * in.radius + 1.0);
    let ray_origin = ray.origin;
    let ray_dir = ray.dir;

    let hit = intersect_sphere(ray_origin, ray_dir, in.center, in.radius);
    if (hit.w < 0.5) {
        discard;
    }
    let hit_point = hit.xyz;
    let normal = normalize(hit_point - in.center);
    var lit_color = shade(hit_point, normal, in.color);

    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);
    let ndc_depth = clip.z / clip.w;
    lit_color = apply_ao(lit_color, in.clip_position, hit_point);

    var out: FragmentOutput;
    out.color = vec4<f32>(finalize_color(lit_color), 1.0);
    out.depth = ndc_depth;
    return out;
}

// Selection-highlight pass: same silhouette (drawn at a slightly larger
// instance radius so it peeks out as a rim), no lighting — just a flat
// translucent tint layered on top of the normal opaque render.
@fragment
fn fs_highlight(in: VertexOutput) -> FragmentOutput {
    let ray = view_ray(in.world_position, 2.0 * in.radius + 1.0);
    let ray_origin = ray.origin;
    let ray_dir = ray.dir;

    let hit = intersect_sphere(ray_origin, ray_dir, in.center, in.radius);
    if (hit.w < 0.5) {
        discard;
    }
    let hit_point = hit.xyz;
    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);
    let ndc_depth = clip.z / clip.w;

    var out: FragmentOutput;
    out.color = vec4<f32>(in.color, 0.35);
    out.depth = ndc_depth;
    return out;
}

// Ambient-occlusion G-buffer pass (export-only, see `ao.rs`): same
// intersection, but outputs world-space normal instead of shaded color, so
// a later full-screen pass can compute real per-pixel occlusion against
// this depth+normal buffer. Depth is written the same way as `fs_main` so
// the two passes agree exactly on where each sphere's surface actually is.
struct GBufferOutput {
    @location(0) normal: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_gbuffer(in: VertexOutput) -> GBufferOutput {
    let ray = view_ray(in.world_position, 2.0 * in.radius + 1.0);
    let ray_origin = ray.origin;
    let ray_dir = ray.dir;

    let hit = intersect_sphere(ray_origin, ray_dir, in.center, in.radius);
    if (hit.w < 0.5) {
        discard;
    }
    let hit_point = hit.xyz;
    let normal = normalize(hit_point - in.center);
    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);

    var out: GBufferOutput;
    out.normal = vec4<f32>(normal, 1.0);
    out.depth = clip.z / clip.w;
    return out;
}

// Atom halos: a translucent sphere around an atom, colored by the atom's
// value — the single-atom version of a fragment envelope, with the same
// look as `envelope.wgsl` (soft wrapped lighting, opacity rising toward
// the silhouette so the atom inside keeps its color). Exact and round like
// the atoms themselves; drawn without writing depth, after the opaque
// scene, farthest first.
const HALO_STRENGTH: f32 = 0.55;
const HALO_CENTER_OPACITY: f32 = 0.26;
const HALO_EDGE_POWER: f32 = 2.2;

@fragment
fn fs_halo(in: VertexOutput) -> FragmentOutput {
    let ray = view_ray(in.world_position, 2.0 * in.radius + 1.0);
    let ray_origin = ray.origin;
    let ray_dir = ray.dir;
    let hit = intersect_sphere(ray_origin, ray_dir, in.center, in.radius);
    if (hit.w < 0.5) {
        discard;
    }
    let n = normalize(hit.xyz - in.center);
    let v = -ray_dir;
    let n_dot_v = max(dot(n, v), 0.0);
    let lit = cloud_color(n, v, normalize(scene.light_dir.xyz), in.color);
    let edge = pow(clamp(1.0 - n_dot_v, 0.0, 1.0), HALO_EDGE_POWER);
    let alpha = HALO_STRENGTH * mix(HALO_CENTER_OPACITY, 1.0, edge);

    let clip = scene.view_proj * vec4<f32>(hit.xyz, 1.0);
    var out: FragmentOutput;
    out.color = vec4<f32>(finalize_color(lit), alpha);
    out.depth = clip.z / clip.w;
    return out;
}
