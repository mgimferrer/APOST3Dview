// Depth-of-field post-process: two fullscreen passes (a plain, non-depth-
// aware separable blur, unlike ambient occlusion's — see `dof.rs`'s module
// doc for why DoF's blur should bleed across depth edges) plus a composite
// pass that mixes the sharp and blurred scene textures per pixel based on
// how far that pixel sits from the focal plane. `vs_fullscreen` is the same
// "oversized triangle" trick as `ao.wgsl`'s — duplicated rather than shared
// since each `include_str!`'d shader module is compiled standalone.
// Everything reads via `textureLoad` (exact texel fetch); no sampler is
// bound anywhere in this file, matching `ao.wgsl`.

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

// ---- Separable Gaussian blur ------------------------------------------

struct DofBlurUniforms {
    // direction.xy (texel step, (1,0) or (0,1)), direction.z = radius in
    // texels (0 = no-op — how `strength = 0` disables the effect), unused.
    direction: vec4<f32>,
    // width, height, unused, unused
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> blur: DofBlurUniforms;
@group(0) @binding(1) var blur_input: texture_2d<f32>;

@fragment
fn fs_blur(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(frag_coord.xy);
    let dims = vec2<i32>(i32(blur.screen.x) - 1, i32(blur.screen.y) - 1);
    let dir = vec2<i32>(blur.direction.xy);
    let radius_f = blur.direction.z;
    let radius = i32(ceil(radius_f));

    if (radius <= 0) {
        return textureLoad(blur_input, pixel, 0);
    }

    // Gaussian falloff sized to the requested radius — sigma = radius/2
    // puts the tail (3 sigma) right around the edge of the kernel.
    let sigma = max(radius_f * 0.5, 0.5);
    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var i = -radius; i <= radius; i = i + 1) {
        let p = clamp(pixel + dir * i, vec2<i32>(0, 0), dims);
        let weight = exp(-0.5 * f32(i * i) / (sigma * sigma));
        sum += textureLoad(blur_input, p, 0) * weight;
        weight_sum += weight;
    }
    return sum / weight_sum;
}

// ---- Composite ---------------------------------------------------------

struct DofCompositeUniforms {
    inv_view_proj: mat4x4<f32>,
    camera_eye: vec4<f32>,
    // focus_distance, focus_range (both world units, angstrom), unused, unused
    params: vec4<f32>,
    // width, height, unused, unused
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> dof: DofCompositeUniforms;
@group(0) @binding(1) var dof_depth: texture_depth_2d;
@group(0) @binding(2) var dof_sharp: texture_2d<f32>;
@group(0) @binding(3) var dof_blurred: texture_2d<f32>;

fn dof_reconstruct_world(pixel_uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(pixel_uv.x * 2.0 - 1.0, 1.0 - pixel_uv.y * 2.0, depth, 1.0);
    let world4 = dof.inv_view_proj * ndc;
    return world4.xyz / world4.w;
}

@fragment
fn fs_composite(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(frag_coord.xy);
    let sharp = textureLoad(dof_sharp, pixel, 0);
    let depth = textureLoad(dof_depth, pixel, 0);
    // No real geometry here (background) — stays sharp; nothing to focus
    // through, and a fully-blurred background makes empty space read as
    // fogged rather than as a lens effect.
    if (depth >= 0.9999) {
        return sharp;
    }

    let uv = frag_coord.xy / dof.screen.xy;
    let world_pos = dof_reconstruct_world(uv, depth);
    let dist = length(world_pos - dof.camera_eye.xyz);

    let focus_distance = dof.params.x;
    let focus_range = dof.params.y;
    // Perfectly sharp within `focus_range` of the focal plane, ramping
    // linearly to fully blurred by `2 * focus_range` beyond it.
    let coc = clamp((abs(dist - focus_distance) - focus_range) / focus_range, 0.0, 1.0);

    let blurred = textureLoad(dof_blurred, pixel, 0);
    return mix(sharp, blurred, coc);
}

// ---- Blit ----------------------------------------------------------------
// Samples the already-composited DoF result — used only by the live view's
// `paint()`, whose render pass is the shared window target: unlike the
// passes above (each rendering into its own viewport-sized texture), a
// window-relative `@builtin(position)` would need the same offset
// correction ambient occlusion's inline sampling needs (see `ao.rs`'s
// module doc). This avoids that entirely by deriving UV purely from
// `vertex_index` in the vertex shader instead of from `frag_coord` in the
// fragment shader — wgpu's active viewport transform (set by egui_wgpu to
// this callback's own on-screen rect) already maps this quad's clip-space
// output to the right place, and the interpolated UV varying goes along
// for the ride correctly regardless of where that rect sits in the window.

struct BlitOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) vertex_index: u32) -> BlitOutput {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    var out: BlitOutput;
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@group(0) @binding(0) var blit_source: texture_2d<f32>;

@fragment
fn fs_blit(in: BlitOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(blit_source));
    let pixel = vec2<i32>(clamp(in.uv * dims, vec2<f32>(0.0), dims - vec2<f32>(1.0)));
    return textureLoad(blit_source, pixel, 0);
}
