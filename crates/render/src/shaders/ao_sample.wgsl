// Shared shader code, part 3: sampling the finished ambient occlusion
// inside the shaded passes (see `ao.rs`/`ao.wgsl`): a precomputed
// depth+normal G-buffer pass and blurred occlusion texture, sampled
// directly inside `fs_main_ao` rather than a separate composite pass — the
// live view is a callback inside egui's own render pass and can't open a
// second pass to read back what it just drew, so this is the one
// mechanism that works for both the live view and PNG export. Prepended
// after `scene.wgsl`.
//
// The three bindings this reads — `ao_sample`, `ao_depth`, `ao_texture` —
// are declared by each shader that uses it, since they sit at a different
// group there: group 1 for atoms and bonds, group 2 for the isosurface
// (whose group 1 is its material). Only the `_ao` pipeline variants bind
// them; the plain entry points never touch them.

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

// A floor under how dark contact shading alone can push a surface — real
// crowded molecules (many atoms with several close neighbors each) turned
// out to have much more raw occlusion than the sparse test cases this was
// first tuned against, and squaring that (`contrast_power`) with no floor
// crushed broad moderately-occluded regions toward near-black instead of
// reading as a sculpted mid-tone — the deep crevices this is actually
// meant to darken stayed the intended focal point either way, so this
// only holds back the wash-over-everything case.
const AO_SHADE_FLOOR: f32 = 0.32;

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
//
// The isosurface itself also writes into the same G-buffer this samples,
// so it both occludes and receives contact shading against atoms/bonds
// and against itself (e.g. where a lobe wraps around an atom, or two
// lobes meet).
fn apply_ao(lit_color: vec3<f32>, frag_coord: vec4<f32>, hit_point: vec3<f32>) -> vec3<f32> {
    let local_coord = frag_coord.xy - ao_sample.offset.xy;
    let pixel = vec2<i32>(local_coord);
    let width = i32(ao_sample.screen.x);
    let height = i32(ao_sample.screen.y);
    let ao_value = max(textureLoad(ao_texture, pixel, 0).r, AO_SHADE_FLOOR);
    let shade_factor = mix(1.0, ao_value, ao_sample.params.x);

    let uv = local_coord / ao_sample.screen.xy;
    let dist0 = length(hit_point - scene.camera_eye.xyz);
    var edge = 0.0;
    if (pixel.x + 1 < width) {
        let d1 = textureLoad(ao_depth, pixel + vec2<i32>(1, 0), 0);
        if (d1 < 0.9999) {
            let world1 = world_from_depth(ao_sample.inv_view_proj, uv + vec2<f32>(1.0 / ao_sample.screen.x, 0.0), d1);
            edge = max(edge, abs(length(world1 - scene.camera_eye.xyz) - dist0));
        }
    }
    if (pixel.y + 1 < height) {
        let d1 = textureLoad(ao_depth, pixel + vec2<i32>(0, 1), 0);
        if (d1 < 0.9999) {
            let world1 = world_from_depth(ao_sample.inv_view_proj, uv + vec2<f32>(0.0, 1.0 / ao_sample.screen.y), d1);
            edge = max(edge, abs(length(world1 - scene.camera_eye.xyz) - dist0));
        }
    }
    let outline = pow(clamp(1.0 - edge * 8.0, 0.0, 1.0), max(ao_sample.params.y, 0.001));
    return lit_color * shade_factor * outline;
}
