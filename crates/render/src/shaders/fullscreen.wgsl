// Shared shader code for the full-screen passes (ambient occlusion, depth
// of field): one "oversized triangle" covering the whole target, no vertex
// buffer — draw 3 vertices.

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
