// Ambient-occlusion pre-passes: two full-screen passes (no vertex buffer,
// the "oversized triangle" trick) sharing one G-buffer produced by the
// atoms/cylinder `fs_gbuffer` entry points. All texture reads use
// `textureLoad` (exact texel fetch) rather than `textureSample` — nothing
// here is filtered, so no sampler is bound anywhere in this file. The
// result (a blurred occlusion texture + the G-buffer depth) is sampled
// back inside the atom/cylinder shaders themselves (`apply_ao` in
// `sphere.wgsl`/`cylinder.wgsl`) rather than composited here — see the
// module doc in `ao.rs` for why.

const KERNEL_SIZE: u32 = 160u;

struct AoUniforms {
    inv_view_proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    camera_eye: vec4<f32>,
    // radius, strength, bias, contrast_power
    params: vec4<f32>,
    // screen_width, screen_height, outline_strength, sample_count (how
    // many of `kernel`'s entries this pass actually uses — the live view
    // requests far fewer than export, see `ao.rs`)
    screen: vec4<f32>,
    kernel: array<vec4<f32>, KERNEL_SIZE>,
};

struct FullscreenOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> FullscreenOutput {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    var out: FullscreenOutput;
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}

// ---- SSAO ------------------------------------------------------------

@group(0) @binding(0) var<uniform> ao: AoUniforms;
@group(0) @binding(1) var ao_gbuffer_depth: texture_depth_2d;
@group(0) @binding(2) var ao_gbuffer_normal: texture_2d<f32>;

fn reconstruct_world(pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = ao.inv_view_proj * ndc;
    return world4.xyz / world4.w;
}

@fragment
fn fs_ssao(@builtin(position) frag_coord: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<i32>(frag_coord.xy);
    let d0 = textureLoad(ao_gbuffer_depth, pixel, 0);
    if (d0 >= 0.9999) {
        return 1.0;
    }

    let uv = frag_coord.xy / ao.screen.xy;
    let world_pos = reconstruct_world(uv, d0);
    let normal = normalize(textureLoad(ao_gbuffer_normal, pixel, 0).xyz);

    // Per-pixel pseudo-random rotation of the kernel — avoids needing a
    // tiling noise texture for a one-shot, not-real-time pass.
    let rand_angle = fract(sin(dot(frag_coord.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453) * 6.28318530718;
    let rvec = vec3<f32>(cos(rand_angle), sin(rand_angle), 0.0);
    let tangent = normalize(rvec - normal * dot(rvec, normal));
    let bitangent = cross(normal, tangent);

    let radius = ao.params.x;
    let bias = ao.params.z;
    let sample_count = u32(ao.screen.w);

    var occlusion = 0.0;
    for (var i = 0u; i < sample_count; i = i + 1u) {
        let k = ao.kernel[i].xyz;
        let sample_dir = tangent * k.x + bitangent * k.y + normal * k.z;
        let candidate_world = world_pos + sample_dir * radius;

        let candidate_clip = ao.view_proj * vec4<f32>(candidate_world, 1.0);
        if (candidate_clip.w <= 0.0) {
            continue;
        }
        let candidate_ndc = candidate_clip.xyz / candidate_clip.w;
        let sample_uv = vec2<f32>(candidate_ndc.x * 0.5 + 0.5, 0.5 - candidate_ndc.y * 0.5);
        if (sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0) {
            continue;
        }
        let sample_pixel = vec2<i32>(sample_uv * ao.screen.xy);
        let actual_d = textureLoad(ao_gbuffer_depth, sample_pixel, 0);
        if (actual_d >= 0.9999) {
            continue;
        }
        let actual_world = reconstruct_world(sample_uv, actual_d);
        let candidate_dist = length(candidate_world - ao.camera_eye.xyz);
        let actual_dist = length(actual_world - ao.camera_eye.xyz);
        let range_check = smoothstep(0.0, 1.0, radius / max(length(actual_world - candidate_world), 0.0001));
        let occluded = select(0.0, 1.0, actual_dist < candidate_dist - bias);
        occlusion += occluded * range_check;
    }
    let ao_value = clamp(1.0 - occlusion / f32(max(sample_count, 1u)), 0.0, 1.0);
    return pow(ao_value, ao.params.w);
}

// ---- Depth-aware separable blur --------------------------------------

struct BlurUniforms {
    // direction.xy (texel step), unused, unused
    direction: vec4<f32>,
    // width, height, unused, unused
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> blur: BlurUniforms;
@group(0) @binding(1) var blur_gbuffer_depth: texture_depth_2d;
@group(0) @binding(2) var blur_input: texture_2d<f32>;
// See the composite pass below for why this reuses `AoUniforms` — the
// depth-aware blur weight needs the same real (angstrom) distance
// comparison, not raw non-linear NDC depth.
@group(0) @binding(3) var<uniform> blur_ao_uniforms: AoUniforms;

fn blur_reconstruct_world(pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = blur_ao_uniforms.inv_view_proj * ndc;
    return world4.xyz / world4.w;
}

@fragment
fn fs_blur(@builtin(position) frag_coord: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<i32>(frag_coord.xy);
    let center_depth = textureLoad(blur_gbuffer_depth, pixel, 0);
    if (center_depth >= 0.9999) {
        return 1.0;
    }
    let dir = vec2<i32>(blur.direction.xy);
    let width = i32(blur.screen.x);
    let height = i32(blur.screen.y);
    let center_uv = frag_coord.xy / blur.screen.xy;
    let center_world = blur_reconstruct_world(center_uv, center_depth);
    let center_dist = length(center_world - blur_ao_uniforms.camera_eye.xyz);

    var sum = 0.0;
    var weight_sum = 0.0;
    // Wider at the cheap live-preview sample count (32, vs. 160 once
    // settled/exporting) — fewer SSAO samples means more per-pixel
    // variance, which reads as visible grain especially on a large,
    // smoothly-colored surface like a big isosurface lobe (small atom
    // spheres hide the same noise magnitude far better just by being
    // small and already colorful). A wider blur trades a little spatial
    // sharpness in the occlusion pattern for real denoising, cheap either
    // way since this is already a small fullscreen pass.
    let radius = select(4, 9, blur_ao_uniforms.screen.w < 64.0);
    for (var i = -radius; i <= radius; i = i + 1) {
        let p = pixel + dir * i;
        if (p.x < 0 || p.y < 0 || p.x >= width || p.y >= height) {
            continue;
        }
        let d = textureLoad(blur_gbuffer_depth, p, 0);
        if (d >= 0.9999) {
            continue;
        }
        let p_uv = vec2<f32>(p) / blur.screen.xy;
        let p_world = blur_reconstruct_world(p_uv, d);
        let p_dist = length(p_world - blur_ao_uniforms.camera_eye.xyz);
        // Angstrom-scale falloff — neighbors within a fraction of an
        // angstrom of the center pixel's own depth blend fully; a real
        // silhouette overlap (an order of magnitude farther) is excluded.
        let depth_weight = 1.0 / (1.0 + abs(p_dist - center_dist) * 30.0);
        let v = textureLoad(blur_input, p, 0).r;
        sum += v * depth_weight;
        weight_sum += depth_weight;
    }
    return select(1.0, sum / weight_sum, weight_sum > 0.0001);
}
