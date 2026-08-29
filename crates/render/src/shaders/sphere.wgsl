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

// Ambient occlusion (see `ao.rs`/`ao.wgsl`): a precomputed depth+normal
// G-buffer pass and blurred occlusion texture, sampled directly inside
// `fs_main_ao` below rather than a separate composite pass — the live
// view is a callback inside egui's own render pass and can't open a
// second pass to read back what it just drew, so this is the one
// mechanism that works for both the live view and PNG export. Only bound
// by the `_ao` pipeline variants; the plain `fs_main`/`fs_highlight`
// pipelines never touch group 1 at all.
struct AoSampleUniforms {
    inv_view_proj: mat4x4<f32>,
    // strength, outline_strength, unused, unused
    params: vec4<f32>,
    // width, height, unused, unused — of the AO/depth textures, i.e. the
    // live viewport's own size, not the full window
    screen: vec4<f32>,
    // x, y, unused, unused — see the Rust-side doc on
    // `AoSampleUniforms::offset` for why this exists at all
    offset: vec4<f32>,
};

@group(1) @binding(0) var<uniform> ao_sample: AoSampleUniforms;
@group(1) @binding(1) var ao_depth: texture_depth_2d;
@group(1) @binding(2) var ao_texture: texture_2d<f32>;

fn ao_reconstruct_world(pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = ao_sample.inv_view_proj * ndc;
    return world4.xyz / world4.w;
}

// Darkens `lit_color` by the precomputed occlusion at this fragment, plus
// a depth-gradient outline (a real silhouette overlap, compared in
// reconstructed world distance rather than raw NDC depth — see `ao.wgsl`
// for why that distinction matters). `hit_point` is this fragment's own
// exact world-space surface point, already known to the caller — reused
// directly rather than reconstructed from the G-buffer, both cheaper and
// immune to any precision mismatch between the two passes.
//
// `frag_coord` (from `@builtin(position)`) is always relative to the
// *whole* render target this fragment shader draws into — for the live
// view that's the entire window (shared with every other egui panel),
// not just the 3D viewport's own on-screen rect, since the AO/depth
// textures bound here are sized to the viewport alone, `ao_sample.offset`
// (the viewport's own top-left in that same window) has to be subtracted
// first to land back in this texture's own (0,0)-origin coordinates.
// Always `[0,0]` for export, which has no such sub-rect at all.
// A floor under how dark contact shading alone can push a surface — real
// crowded molecules (many atoms with several close neighbors each) turned
// out to have much more raw occlusion than the sparse test cases this was
// first tuned against, and squaring that (`contrast_power`) with no floor
// crushed broad moderately-occluded regions toward near-black instead of
// reading as a sculpted mid-tone — the deep crevices this is actually
// meant to darken stayed the intended focal point either way, so this
// only holds back the wash-over-everything case.
const AO_SHADE_FLOOR: f32 = 0.32;

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

// ---- Color pipeline: sRGB<->linear, hemisphere fill light, Fresnel,
// filmic tone mapping. Duplicated identically in `cylinder.wgsl` and
// `isosurface.wgsl` — each `include_str!`'d shader module compiles
// standalone, so there's no way to share a real module between them.

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

// Narkowicz's ACES filmic fit — cheap, no LUT, and gives bright highlights
// a soft shoulder instead of clipping straight to flat white the way
// unclamped linear output does.
fn aces_tonemap(c: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let cc = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((c * (a * c + vec3<f32>(b))) / (c * (cc * c + vec3<f32>(d)) + vec3<f32>(e)), vec3<f32>(0.0), vec3<f32>(1.0));
}

// A cheap image-based-lighting stand-in: tints the ambient term by world-
// space surface orientation (brighter/cooler facing up, dimmer/warmer
// facing down) instead of a single flat scalar, so the side of a sphere
// facing away from the key light still shows a gradient rather than
// crushing to a flat near-black — a fixed, not user-tunable, "fill light"
// in effect (see the module doc on why: one exposure slider is worth
// exposing, three more light-rig sliders aren't).
fn hemisphere_ambient(normal: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(1.05, 1.05, 1.1);
    let ground = vec3<f32>(0.65, 0.62, 0.6);
    return mix(ground, sky, normal.y * 0.5 + 0.5);
}

const FRESNEL_POWER: f32 = 5.0;
const FRESNEL_STRENGTH: f32 = 0.08;

fn fresnel_schlick(n_dot_v: f32, power: f32) -> f32 {
    return pow(clamp(1.0 - n_dot_v, 0.0, 1.0), power);
}

// Exposure -> filmic tone map -> sRGB encode (only when the render target
// itself won't do that encode automatically — see `SceneUniforms::
// set_srgb_target`). The one place every fragment shader's lit color
// should pass through right before being written out.
fn finalize_color(linear_color: vec3<f32>) -> vec3<f32> {
    let mapped = aces_tonemap(linear_color * scene.style.z);
    return select(linear_to_srgb(mapped), mapped, scene.style.w > 0.5);
}

// Blinn-Phong shading (plus a hemisphere fill term and a subtle Fresnel
// rim) shared by `fs_main` and `fs_main_ao` — kept as one function so the
// two entry points can never drift apart on the base lighting, only on
// whether `apply_ao` runs afterward. Returns *linear-light* color, not a
// final pixel value — callers must pass it through `finalize_color`.
fn shade(hit_point: vec3<f32>, normal: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let light_dir = normalize(scene.light_dir.xyz);
    let view_dir = normalize(scene.camera_eye.xyz - hit_point);
    let half_dir = normalize(light_dir + view_dir);

    let albedo = srgb_to_linear(color);
    let ambient = scene.material.x * hemisphere_ambient(normal);
    let diffuse_strength = scene.material.y * max(dot(normal, light_dir), 0.0);
    let specular_strength = scene.material.z * pow(max(dot(normal, half_dir), 0.0), scene.material.w);
    let fresnel = fresnel_schlick(max(dot(normal, view_dir), 0.0), FRESNEL_POWER) * FRESNEL_STRENGTH;
    return albedo * (ambient + diffuse_strength) + vec3<f32>(specular_strength + fresnel);
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);

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
// precomputed textures at group 1 — see `apply_ao` above.
@fragment
fn fs_main_ao(in: VertexOutput) -> FragmentOutput {
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);

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
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);

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
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);

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
