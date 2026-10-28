use std::collections::HashSet;
use std::sync::Mutex;

use wgpu::util::DeviceExt;

use apost3dview_core::Molecule;

use crate::ao::{AoSettings, AoUniforms};
use crate::camera::OrbitCamera;
use crate::consts::{DEPTH_FORMAT, MSAA_SAMPLES};
use crate::dof::{DofBlurUniforms, DofCompositeUniforms, DofSettings};
use crate::glyphs::GlyphAtlas;
use crate::style::DrawnSizes;
use crate::instances::{
    build_atom_highlight_instances, build_atom_instances, build_bond_highlight_instances, build_bond_instances,
    build_measurement_instances, AtomInstance, BondInstance, BondVisualStyle,
};
use crate::isosurface_mesh::{is_translucent, sort_back_to_front, IsosurfaceMaterial, IsosurfaceVertex};
use crate::label::GlyphInstance;
use crate::material::Material;
use crate::mesh::{build_unit_cylinder, CylinderVertex};
use crate::shaders;
use crate::uniforms::SceneUniforms;

fn atom_instance_attributes() -> [wgpu::VertexAttribute; 3] {
    [
        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32 },
        wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
    ]
}

fn bond_instance_attributes() -> [wgpu::VertexAttribute; 4] {
    [
        wgpu::VertexAttribute { offset: 0, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
        wgpu::VertexAttribute { offset: 12, shader_location: 3, format: wgpu::VertexFormat::Float32 },
        wgpu::VertexAttribute { offset: 16, shader_location: 4, format: wgpu::VertexFormat::Float32x3 },
        wgpu::VertexAttribute { offset: 28, shader_location: 5, format: wgpu::VertexFormat::Float32 },
    ]
}

fn bond_color_attribute() -> wgpu::VertexAttribute {
    wgpu::VertexAttribute { offset: 32, shader_location: 6, format: wgpu::VertexFormat::Float32x3 }
}

fn bond_radius_scale_attribute() -> wgpu::VertexAttribute {
    wgpu::VertexAttribute { offset: 44, shader_location: 7, format: wgpu::VertexFormat::Float32 }
}

/// G-buffer normals, written by the depth+normal pre-pass that ambient
/// occlusion and depth of field share.
const GBUFFER_NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Raw and blurred ambient occlusion.
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

fn depth_state(write: bool, compare: wgpu::CompareFunction) -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    })
}

fn color_target(format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

fn depth_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

fn float_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn create_bind_group_layout(device: &wgpu::Device, label: &str, entries: &[wgpu::BindGroupLayoutEntry]) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries })
}

fn create_pipeline_layout(device: &wgpu::Device, label: &str, groups: &[&wgpu::BindGroupLayout]) -> wgpu::PipelineLayout {
    let groups: Vec<Option<&wgpu::BindGroupLayout>> = groups.iter().map(|group| Some(*group)).collect();
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &groups, immediate_size: 0 })
}

/// Where a pipeline's vertices come from: a shader module (which also
/// holds the fragment entry point), its vertex entry point and its vertex
/// buffers.
struct VertexSource<'a> {
    module: &'a wgpu::ShaderModule,
    entry: &'static str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
}

/// A render pipeline drawing into one color target; every pipeline here
/// has that shape, and none culls faces.
struct PipelineSpec<'a> {
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    vertex: &'a VertexSource<'a>,
    fragment: &'static str,
    target: wgpu::ColorTargetState,
    depth: Option<wgpu::DepthStencilState>,
    multisample: wgpu::MultisampleState,
}

impl PipelineSpec<'_> {
    fn build(self, device: &wgpu::Device) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(self.label),
            layout: Some(self.layout),
            vertex: wgpu::VertexState {
                module: self.vertex.module,
                entry_point: Some(self.vertex.entry),
                buffers: self.vertex.buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: self.vertex.module,
                entry_point: Some(self.fragment),
                targets: &[Some(self.target)],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: self.depth,
            multisample: self.multisample,
            multiview_mask: None,
            cache: None,
        })
    }
}

/// A 2D render-target texture, one mip level.
fn render_texture(
    device: &wgpu::Device,
    label: &str,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    sample_count: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | usage,
        view_formats: &[],
    })
}

/// A render pass into one color target (`resolve` receiving the resolved
/// samples of a multisampled one), with a depth buffer if `depth` is given
/// (cleared to 1.0, then stored or discarded).
fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    view: &wgpu::TextureView,
    resolve: Option<&wgpu::TextureView>,
    load: wgpu::LoadOp<wgpu::Color>,
    depth: Option<(&wgpu::TextureView, wgpu::StoreOp)>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: resolve,
            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            depth_slice: None,
        })],
        depth_stencil_attachment: depth.map(|(view, store)| wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// One full-screen pass: clears `target` to `clear` and draws the
/// full-screen triangle with `pipeline` and `bind_group`.
fn fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    clear: wgpu::Color,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut pass = begin_pass(encoder, label, target, None, wgpu::LoadOp::Clear(clear), None);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

/// The textures one depth-of-field run reads and writes: the sharp scene
/// and its depth, the two blur directions, and the output.
struct DofTargets<'a> {
    sharp: &'a wgpu::TextureView,
    depth: &'a wgpu::TextureView,
    blur_h: &'a wgpu::TextureView,
    blur_v: &'a wgpu::TextureView,
    output: &'a wgpu::TextureView,
}

/// Owns the GPU resources for the 3D viewport (pipelines, buffers). Lives
/// in egui-wgpu's `CallbackResources` so it shares the device/queue eframe
/// already created, rather than opening a second one.
/// One envelope's slice of the envelope vertex buffer, with its center,
/// so envelopes can be drawn farthest from the camera first.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeRange {
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub center: [f32; 3],
}

pub struct ViewportResources {
    target_format: wgpu::TextureFormat,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    atom_pipeline: wgpu::RenderPipeline,
    cylinder_pipeline: wgpu::RenderPipeline,
    atom_highlight_pipeline: wgpu::RenderPipeline,
    /// Translucent value halos around atoms (`fs_halo`), and their
    /// instances: kept on the CPU too, to re-sort farthest first whenever
    /// the camera moves.
    atom_halo_pipeline: wgpu::RenderPipeline,
    halo_instances: Option<(wgpu::Buffer, u32)>,
    halo_cpu: Mutex<Vec<AtomInstance>>,
    cylinder_highlight_pipeline: wgpu::RenderPipeline,

    cylinder_vertex_buffer: wgpu::Buffer,
    cylinder_index_buffer: wgpu::Buffer,
    cylinder_index_count: u32,

    // True 3D labels: real depth-tested billboard glyph quads (not a 2D
    // UI overlay), so a bond or atom nearer the camera correctly, even
    // partially, occludes them — a flat overlay can only ever be
    // all-or-nothing about that.
    glyph_bind_group: wgpu::BindGroup,
    text_pipeline: wgpu::RenderPipeline,
    text_instances: Option<(wgpu::Buffer, u32)>,
    /// Labels drawn over everything (values that must stay readable),
    /// from glyphs with `on_top` set.
    text_overlay_pipeline: wgpu::RenderPipeline,
    text_overlay_pipeline_single: wgpu::RenderPipeline,
    overlay_text_instances: Option<(wgpu::Buffer, u32)>,
    envelope_pipeline: wgpu::RenderPipeline,
    envelope_vertices: Option<wgpu::Buffer>,
    envelope_ranges: Vec<EnvelopeRange>,
    /// Camera position of the last `write_uniforms`, to order envelopes.
    envelope_eye: Mutex<[f32; 3]>,

    atom_instances: Option<(wgpu::Buffer, u32)>,
    bond_instances: Option<(wgpu::Buffer, u32)>,
    atom_highlight_instances: Option<(wgpu::Buffer, u32)>,
    bond_highlight_instances: Option<(wgpu::Buffer, u32)>,
    measurement_instances: Option<(wgpu::Buffer, u32)>,

    // Isosurfaces (orbitals, densities, cube files): an ordinary
    // rasterized/lit triangle mesh (Surface Nets output), not a raymarched
    // impostor like atoms/bonds, with its own material uniform kept
    // entirely separate from the atom/bond one. Opaque surfaces
    // (`isosurface_vertices`) write depth and take part in ambient
    // occlusion like atoms; translucent ones (opacity below 1) are drawn
    // after them, without writing depth, their triangles re-sorted
    // farthest first whenever the camera moves, so every layer blends in
    // the right order.
    isosurface_pipeline: wgpu::RenderPipeline,
    isosurface_pipeline_ao: wgpu::RenderPipeline,
    isosurface_gbuffer_pipeline: wgpu::RenderPipeline,
    isosurface_material_buffer: wgpu::Buffer,
    isosurface_material_bind_group: wgpu::BindGroup,
    isosurface_vertices: Option<(wgpu::Buffer, u32)>,
    translucent_pipeline: wgpu::RenderPipeline,
    translucent_vertices: Option<(wgpu::Buffer, u32)>,
    translucent_cpu: Mutex<TranslucentTriangles>,

    // Ambient occlusion (see `ao.rs`): a depth+normal G-buffer pre-pass
    // reusing the atom/cylinder geometry, then two full-screen passes
    // (SSAO, blur) producing a texture sampled directly inside
    // `atom_pipeline_ao`/`cylinder_pipeline_ao` (see those shaders'
    // `apply_ao`) — one mechanism shared by the live view and PNG export.
    // Export builds fresh G-buffer/AO textures per call (`run_ao_passes`);
    // the live view keeps persistent, resize-aware ones (`ao_live`,
    // `ensure_ao_textures`), since it can't afford to reallocate every
    // frame.
    atom_pipeline_ao: wgpu::RenderPipeline,
    cylinder_pipeline_ao: wgpu::RenderPipeline,
    ao_sample_bind_group_layout: wgpu::BindGroupLayout,
    atom_gbuffer_pipeline: wgpu::RenderPipeline,
    cylinder_gbuffer_pipeline: wgpu::RenderPipeline,
    ssao_bind_group_layout: wgpu::BindGroupLayout,
    ssao_pipeline: wgpu::RenderPipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    blur_pipeline: wgpu::RenderPipeline,
    ao_live: Option<AoLiveTextures>,

    // Depth of field (see `dof.rs`): render the scene to an offscreen
    // texture, blur it, then composite sharp/blurred per pixel by distance
    // from the focal plane — see `run_live_dof_pass`/`render_offscreen`.
    dof_blur_bind_group_layout: wgpu::BindGroupLayout,
    dof_blur_pipeline: wgpu::RenderPipeline,
    dof_composite_bind_group_layout: wgpu::BindGroupLayout,
    dof_composite_pipeline: wgpu::RenderPipeline,
    dof_blit_bind_group_layout: wgpu::BindGroupLayout,
    dof_blit_pipeline: wgpu::RenderPipeline,
    dof_live: Option<DofLiveTextures>,
}

/// The translucent surfaces' triangles, and the camera position they were
/// last sorted for (sorting again only when it moves).
#[derive(Default)]
struct TranslucentTriangles {
    triangles: Vec<IsosurfaceVertex>,
    sorted_for: Option<[f32; 3]>,
}

/// Persistent, resize-aware G-buffer/AO textures for the live view —
/// export doesn't use this at all (fresh textures per call, see
/// `render_offscreen`), since export resolution is one-shot and often
/// unrelated to the live viewport's own size.
struct AoLiveTextures {
    width: u32,
    height: u32,
    gbuffer_normal_view: wgpu::TextureView,
    gbuffer_depth_view: wgpu::TextureView,
    ao_raw_view: wgpu::TextureView,
    blur_h_view: wgpu::TextureView,
    blur_v_view: wgpu::TextureView,
    /// The bind group `draw_into_pass` needs at group 1 for the `_ao`
    /// pipelines — rebuilt (cheap) whenever the textures are recreated,
    /// stored here since the views it references live on this struct.
    sample_bind_group: wgpu::BindGroup,
}

/// Persistent, resize-aware textures for the live view's depth-of-field
/// pipeline — see `run_live_dof_pass`. Export builds its own fresh set per
/// call (one-shot, see `render_offscreen`), same division as AO's.
///
/// Entirely independent of `AoLiveTextures`/`ao_live`, even though DoF
/// needs its own ambient-occlusion pre-passes when AO is also enabled:
/// `ao_live`'s AO bind group is sampled with `frag_coord` relative to the
/// *whole window* (see `AoSampleUniforms::offset`'s doc), because the
/// normal (non-DoF) live path draws geometry directly into the shared
/// egui pass. DoF instead draws that same geometry into its own private,
/// viewport-sized texture — so AO run for DoF's purposes needs `offset =
/// [0, 0]` instead, and can't share `ao_live`'s already-window-offset
/// bind group without the two paths fighting over what that struct holds.
/// A G-buffer depth+normal pass runs here unconditionally (cheap, and
/// depth is needed for the DoF composite regardless of whether AO
/// shading itself is on); the SSAO+blur passes only run on top of it when
/// AO is actually enabled.
struct DofLiveTextures {
    width: u32,
    height: u32,
    scene_msaa_color_view: wgpu::TextureView,
    scene_resolve_view: wgpu::TextureView,
    scene_msaa_depth_view: wgpu::TextureView,
    gbuffer_normal_view: wgpu::TextureView,
    gbuffer_depth_view: wgpu::TextureView,
    ao_raw_view: wgpu::TextureView,
    ao_blur_h_view: wgpu::TextureView,
    ao_blur_v_view: wgpu::TextureView,
    dof_blur_h_view: wgpu::TextureView,
    dof_blur_v_view: wgpu::TextureView,
    dof_output_view: wgpu::TextureView,
    /// Bound once at texture-creation time, not rebuilt every frame like
    /// AO's `sample_bind_group` — it only references textures (no
    /// per-frame-changing uniform buffer), and this struct always writes
    /// into the same persistent `dof_output_view` object, so the bind
    /// group stays valid until the next resize recreates it.
    blit_bind_group: wgpu::BindGroup,
}

impl ViewportResources {
    /// `glyph_atlas` is constructed and owned by the caller (it's also
    /// needed CPU-side, for laying out label text, independent of this
    /// GPU-resources struct) — this just builds a bind group referencing
    /// its texture.
    /// The color format every pipeline here was built against — needed by
    /// callers (`ViewportCallback::prepare`, `App`'s export path) to tell
    /// `SceneUniforms::set_srgb_target` whether the hardware will encode
    /// linear output to sRGB automatically on write.
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, glyph_atlas: &GlyphAtlas) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_uniforms"),
            size: std::mem::size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = create_bind_group_layout(
            device,
            "scene_bind_group_layout",
            &[wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ..uniform_entry(0) }],
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });

        let pipeline_layout = create_pipeline_layout(device, "viewport_pipeline_layout", &[&bind_group_layout]);

        let opaque_depth = depth_state(true, wgpu::CompareFunction::Less);
        // Highlight overlays reuse the exact geometry (or a very slightly
        // enlarged one for atoms) of what's already drawn, so an exact
        // `Less` comparison risks failing on floating-point ties — hence
        // `LessEqual` and no depth write, so the overlay reliably shows up
        // without disturbing depth for anything drawn after it.
        let highlight_depth = depth_state(false, wgpu::CompareFunction::LessEqual);
        // Translucent layers over the finished opaque scene (halos,
        // envelopes): depth-tested against it, never writing depth.
        let translucent_depth = depth_state(false, wgpu::CompareFunction::Less);
        let multisample = wgpu::MultisampleState { count: MSAA_SAMPLES, ..Default::default() };
        let replace = Some(wgpu::BlendState::REPLACE);
        let alpha_blend = Some(wgpu::BlendState::ALPHA_BLENDING);

        let sphere_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sphere_impostor_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::SPHERE.into()),
        });
        let cylinder_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cylinder_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::CYLINDER.into()),
        });

        let atom_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<AtomInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &atom_instance_attributes(),
        };
        let bond_attrs = bond_instance_attributes();
        let bond_color_attr = bond_color_attribute();
        let bond_instance_attrs =
            [bond_attrs[0], bond_attrs[1], bond_attrs[2], bond_attrs[3], bond_color_attr, bond_radius_scale_attribute()];
        let bond_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BondInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &bond_instance_attrs,
        };
        let cylinder_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CylinderVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
            ],
        };
        let atom_buffers = [Some(atom_instance_layout)];
        let atoms = VertexSource { module: &sphere_shader, entry: "vs_main", buffers: &atom_buffers };
        let bond_buffers = [Some(cylinder_vertex_layout), Some(bond_instance_layout)];
        let bonds = VertexSource { module: &cylinder_shader, entry: "vs_main", buffers: &bond_buffers };

        let shaded = |label, layout, vertex, fragment, blend, depth| PipelineSpec {
            label,
            layout,
            vertex,
            fragment,
            target: color_target(target_format, blend),
            depth,
            multisample,
        };

        let atom_pipeline = shaded("atom_pipeline", &pipeline_layout, &atoms, "fs_main", replace, opaque_depth.clone()).build(device);
        let atom_highlight_pipeline =
            shaded("atom_highlight_pipeline", &pipeline_layout, &atoms, "fs_highlight", alpha_blend, highlight_depth.clone()).build(device);
        let atom_halo_pipeline = shaded("atom_halo_pipeline", &pipeline_layout, &atoms, "fs_halo", alpha_blend, translucent_depth.clone()).build(device);
        let cylinder_pipeline = shaded("cylinder_pipeline", &pipeline_layout, &bonds, "fs_main", replace, opaque_depth.clone()).build(device);
        let cylinder_highlight_pipeline =
            shaded("cylinder_highlight_pipeline", &pipeline_layout, &bonds, "fs_highlight", alpha_blend, highlight_depth).build(device);

        // Ambient-occlusion-sampling variants of the two shaded pipelines
        // above — a separate pipeline layout (group 0 unchanged, plus a
        // new group 1: the AO uniform + depth + occlusion textures) and
        // entry point (`fs_main_ao`) rather than modifying `atom_pipeline`/
        // `cylinder_pipeline` in place, so the always-used non-AO path is
        // completely untouched — zero risk of regressing it. Highlight
        // pipelines don't get an AO variant; they're a flat translucent
        // overlay with no lighting to occlude.
        let ao_sample_bind_group_layout =
            create_bind_group_layout(device, "ao_sample_bind_group_layout", &[uniform_entry(0), depth_texture_entry(1), float_texture_entry(2)]);
        let shaded_ao_pipeline_layout =
            create_pipeline_layout(device, "shaded_ao_pipeline_layout", &[&bind_group_layout, &ao_sample_bind_group_layout]);
        let atom_pipeline_ao =
            shaded("atom_pipeline_ao", &shaded_ao_pipeline_layout, &atoms, "fs_main_ao", replace, opaque_depth.clone()).build(device);
        let cylinder_pipeline_ao =
            shaded("cylinder_pipeline_ao", &shaded_ao_pipeline_layout, &bonds, "fs_main_ao", replace, opaque_depth.clone()).build(device);

        // Bonds are a real triangle mesh (unlike atoms, which are
        // analytically raymarched and so perfectly round at any zoom) —
        // 16 sides read as visibly faceted, especially the silhouette and
        // the specular highlight (a low-poly smooth-shaded cylinder's
        // highlight tends to show as distinct streaks per face rather
        // than one continuous one). 48 is still a trivial triangle count
        // for any real molecule's bond count.
        let (cylinder_vertices, cylinder_indices) = build_unit_cylinder(48);
        let cylinder_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cylinder_vertex_buffer"),
            contents: bytemuck::cast_slice(&cylinder_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let cylinder_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cylinder_index_buffer"),
            contents: bytemuck::cast_slice(&cylinder_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // True 3D text: billboard glyph quads through the same depth
        // buffer as everything else, so labels get correct — including
        // partial — occlusion instead of a 2D overlay's all-or-nothing.
        let glyph_bind_group_layout = create_bind_group_layout(
            device,
            "glyph_bind_group_layout",
            &[
                float_texture_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        );
        let glyph_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph_bind_group"),
            layout: &glyph_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&glyph_atlas.texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&glyph_atlas.sampler) },
            ],
        });
        let text_pipeline_layout = create_pipeline_layout(device, "text_pipeline_layout", &[&bind_group_layout, &glyph_bind_group_layout]);
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::TEXT.into()),
        });
        let glyph_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 16, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 32, shader_location: 3, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 40, shader_location: 4, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 48, shader_location: 5, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 60, shader_location: 6, format: wgpu::VertexFormat::Float32 },
                wgpu::VertexAttribute { offset: 64, shader_location: 7, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 76, shader_location: 8, format: wgpu::VertexFormat::Float32 },
            ],
        };
        let glyph_buffers = [Some(glyph_instance_layout)];
        let glyphs = VertexSource { module: &text_shader, entry: "vs_main", buffers: &glyph_buffers };
        // Alpha-to-coverage, not blending: each MSAA sample is dithered
        // in/out based on the glyph's coverage, giving antialiased edges
        // while still writing real per-sample depth.
        let text_multisample = wgpu::MultisampleState { count: MSAA_SAMPLES, alpha_to_coverage_enabled: true, ..Default::default() };
        let text_pipeline = PipelineSpec {
            multisample: text_multisample,
            ..shaded("text_pipeline", &text_pipeline_layout, &glyphs, "fs_main", replace, opaque_depth.clone())
        }
        .build(device);
        // The same glyphs drawn regardless of depth, for value labels that
        // must stay readable even when an atom is in front of their anchor.
        let text_overlay_pipeline = PipelineSpec {
            multisample: text_multisample,
            ..shaded(
                "text_overlay_pipeline",
                &text_pipeline_layout,
                &glyphs,
                "fs_main",
                replace,
                depth_state(false, wgpu::CompareFunction::Always),
            )
        }
        .build(device);
        // The same, for a single-sampled target without depth (drawn onto an
        // export's depth-of-field output): blended instead of
        // alpha-to-coverage, which needs MSAA.
        let text_overlay_pipeline_single = PipelineSpec {
            multisample: wgpu::MultisampleState::default(),
            ..shaded("text_overlay_pipeline_single", &text_pipeline_layout, &glyphs, "fs_main", alpha_blend, None)
        }
        .build(device);

        // Isosurfaces: an ordinary rasterized/lit triangle mesh, not a
        // raymarched impostor, so no custom fragment depth override is
        // needed. Opaque surfaces write depth like any solid object.
        // Translucent ones don't: their triangles are blended farthest
        // first instead (`sort_back_to_front`), so both walls of a lobe and
        // every surface behind it show, in the right order. Writing depth
        // there with unsorted triangles (as before 2026-10-17) let the GPU's
        // arbitrary triangle order decide which layers survived, pixel by
        // pixel, which showed as mottled patches.
        let isosurface_depth = depth_state(true, wgpu::CompareFunction::Less);

        let isosurface_material_layout = create_bind_group_layout(device, "isosurface_material_bind_group_layout", &[uniform_entry(0)]);
        let isosurface_material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("isosurface_material_buffer"),
            contents: bytemuck::bytes_of(&IsosurfaceMaterial::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let isosurface_material_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("isosurface_material_bind_group"),
            layout: &isosurface_material_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: isosurface_material_buffer.as_entire_binding() }],
        });
        let isosurface_pipeline_layout =
            create_pipeline_layout(device, "isosurface_pipeline_layout", &[&bind_group_layout, &isosurface_material_layout]);
        let isosurface_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("isosurface_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::ISOSURFACE.into()),
        });
        let isosurface_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<IsosurfaceVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 16, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 32, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
                wgpu::VertexAttribute { offset: 44, shader_location: 3, format: wgpu::VertexFormat::Float32 },
            ],
        };
        let mesh_buffers = [Some(isosurface_vertex_layout)];
        let isosurface_mesh = VertexSource { module: &isosurface_shader, entry: "vs_main", buffers: &mesh_buffers };
        // Envelopes (see `envelope.wgsl`): translucent, depth-tested against
        // the opaque scene but never writing depth, so they neither hide
        // nor darken what is inside them.
        let envelope_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("envelope_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::ENVELOPE.into()),
        });
        let envelope_mesh = VertexSource { module: &envelope_shader, entry: "vs_main", buffers: &mesh_buffers };
        let envelope_pipeline_layout = create_pipeline_layout(device, "envelope_pipeline_layout", &[&bind_group_layout]);
        let envelope_pipeline =
            shaded("envelope_pipeline", &envelope_pipeline_layout, &envelope_mesh, "fs_main", alpha_blend, translucent_depth.clone()).build(device);

        let isosurface_pipeline =
            shaded("isosurface_pipeline", &isosurface_pipeline_layout, &isosurface_mesh, "fs_main", alpha_blend, isosurface_depth.clone())
                .build(device);
        // Translucent surfaces: no ambient-occlusion variant, since they
        // neither cast nor receive it (the occlusion and outline at a
        // translucent pixel belong to whatever lies behind it).
        let translucent_pipeline =
            shaded("translucent_pipeline", &isosurface_pipeline_layout, &isosurface_mesh, "fs_main", alpha_blend, translucent_depth.clone())
                .build(device);

        // AO-sampling variant of the isosurface pipeline — same shape as
        // `atom_pipeline_ao`/`cylinder_pipeline_ao`, but AO has to bind at
        // group 2 here (not group 1, already the isosurface material) and
        // so needs its own pipeline layout. See `isosurface.wgsl`'s
        // `fs_main_ao`.
        let isosurface_ao_pipeline_layout = create_pipeline_layout(
            device,
            "isosurface_ao_pipeline_layout",
            &[&bind_group_layout, &isosurface_material_layout, &ao_sample_bind_group_layout],
        );
        let isosurface_pipeline_ao =
            shaded("isosurface_pipeline_ao", &isosurface_ao_pipeline_layout, &isosurface_mesh, "fs_main_ao", alpha_blend, isosurface_depth)
                .build(device);

        // Ambient occlusion (export-only, see `ao.rs`): a depth+normal
        // pre-pass reusing the same atom/cylinder geometry and instance
        // data as the main draw, rendered single-sampled — SSAO gets
        // blurred afterward anyway, and the composite pass runs against
        // the already-MSAA-resolved scene color, so the G-buffer itself
        // doesn't need MSAA.
        let gbuffer = |label, layout, vertex| PipelineSpec {
            label,
            layout,
            vertex,
            fragment: "fs_gbuffer",
            target: color_target(GBUFFER_NORMAL_FORMAT, None),
            depth: depth_state(true, wgpu::CompareFunction::Less),
            multisample: wgpu::MultisampleState::default(),
        };
        let atom_gbuffer_pipeline = gbuffer("atom_gbuffer_pipeline", &pipeline_layout, &atoms).build(device);
        let cylinder_gbuffer_pipeline = gbuffer("cylinder_gbuffer_pipeline", &pipeline_layout, &bonds).build(device);
        // Isosurface's own G-buffer pass — see `isosurface.wgsl`'s
        // `fs_gbuffer` doc for why this matters (without it, the
        // isosurface neither occludes atoms/bonds in AO nor receives any
        // AO shading itself). Reuses `isosurface_pipeline_layout` (group 0
        // scene + group 1 material) even though `fs_gbuffer` doesn't touch
        // group 1 at all — simpler than a third pipeline layout just for
        // this, and `draw_gbuffer_pass` already has nothing else it would
        // need group 1 bound to at that point.
        let isosurface_gbuffer_pipeline = gbuffer("isosurface_gbuffer_pipeline", &isosurface_pipeline_layout, &isosurface_mesh).build(device);

        // Full-screen passes (ambient occlusion and depth of field): one
        // triangle, single-sampled, no depth, writing their own textures.
        let fullscreen = |label, layout, vertex, fragment, format| PipelineSpec {
            label,
            layout,
            vertex,
            fragment,
            target: color_target(format, None),
            depth: None,
            multisample: wgpu::MultisampleState::default(),
        };

        let ao_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ao_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::AO.into()),
        });
        let ao_fullscreen = VertexSource { module: &ao_shader, entry: "vs_fullscreen", buffers: &[] };

        // SSAO: uniform + G-buffer depth + G-buffer normal -> raw occlusion.
        let ssao_bind_group_layout =
            create_bind_group_layout(device, "ssao_bind_group_layout", &[uniform_entry(0), depth_texture_entry(1), float_texture_entry(2)]);
        let ssao_pipeline_layout = create_pipeline_layout(device, "ssao_pipeline_layout", &[&ssao_bind_group_layout]);
        let ssao_pipeline = fullscreen("ssao_pipeline", &ssao_pipeline_layout, &ao_fullscreen, "fs_ssao", AO_FORMAT).build(device);

        // Depth-aware separable blur: uniform (direction) + G-buffer depth
        // + the SSAO pass's raw output -> denoised occlusion. Run twice
        // (horizontal, vertical) against the same pipeline/layout.
        let blur_bind_group_layout = create_bind_group_layout(
            device,
            "blur_bind_group_layout",
            &[uniform_entry(0), depth_texture_entry(1), float_texture_entry(2), uniform_entry(3)],
        );
        let blur_pipeline_layout = create_pipeline_layout(device, "blur_pipeline_layout", &[&blur_bind_group_layout]);
        let blur_pipeline = fullscreen("blur_pipeline", &blur_pipeline_layout, &ao_fullscreen, "fs_blur", AO_FORMAT).build(device);

        // Depth of field (see `dof.rs`): a plain (non-depth-aware)
        // separable blur over the resolved scene color, then a composite
        // pass mixing sharp/blurred by world-space distance from the focal
        // plane, then (live view only) a blit into the shared window pass.
        let dof_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dof_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::DOF.into()),
        });
        let dof_fullscreen = VertexSource { module: &dof_shader, entry: "vs_fullscreen", buffers: &[] };

        let dof_blur_bind_group_layout = create_bind_group_layout(device, "dof_blur_bind_group_layout", &[uniform_entry(0), float_texture_entry(1)]);
        let dof_blur_pipeline_layout = create_pipeline_layout(device, "dof_blur_pipeline_layout", &[&dof_blur_bind_group_layout]);
        let dof_blur_pipeline = fullscreen("dof_blur_pipeline", &dof_blur_pipeline_layout, &dof_fullscreen, "fs_blur", target_format).build(device);

        let dof_composite_bind_group_layout = create_bind_group_layout(
            device,
            "dof_composite_bind_group_layout",
            &[uniform_entry(0), depth_texture_entry(1), float_texture_entry(2), float_texture_entry(3)],
        );
        let dof_composite_pipeline_layout = create_pipeline_layout(device, "dof_composite_pipeline_layout", &[&dof_composite_bind_group_layout]);
        let dof_composite_pipeline =
            fullscreen("dof_composite_pipeline", &dof_composite_pipeline_layout, &dof_fullscreen, "fs_composite", target_format).build(device);

        // Blit: samples the finished DoF composite into the live view's
        // own shared window pass — see `dof.wgsl`'s module doc for why
        // this uses a vertex-shader UV instead of `frag_coord`. Export
        // doesn't need this at all — it reads the composite texture back
        // directly (see `render_offscreen`).
        let dof_blit_bind_group_layout = create_bind_group_layout(device, "dof_blit_bind_group_layout", &[float_texture_entry(0)]);
        let dof_blit_pipeline_layout = create_pipeline_layout(device, "dof_blit_pipeline_layout", &[&dof_blit_bind_group_layout]);
        // Unlike the blur/composite passes (which render into their own
        // single-sampled private textures with no depth attachment at
        // all), this draws into the live view's shared window pass
        // alongside the atom/cylinder pipelines — which is both
        // multisampled (`MSAA_SAMPLES`) *and* carries a real depth
        // attachment (`DEPTH_FORMAT`, since the direct-draw path depth-
        // tests atoms/bonds/labels against each other there). A render
        // pipeline's sample count and depth-stencil format both have to
        // match the pass it's used in exactly, or wgpu rejects it at draw
        // time — this pipeline doesn't need to depth-test anything itself
        // (it's one full-viewport quad, nothing else drawn in that rect
        // this frame), hence `Always`/no write, but it still has to
        // declare the same format to be pass-compatible at all.
        let dof_blit = VertexSource { module: &dof_shader, entry: "vs_blit", buffers: &[] };
        let dof_blit_pipeline = shaded(
            "dof_blit_pipeline",
            &dof_blit_pipeline_layout,
            &dof_blit,
            "fs_blit",
            replace,
            depth_state(false, wgpu::CompareFunction::Always),
        )
        .build(device);

        Self {
            target_format,
            uniform_buffer,
            bind_group,
            atom_pipeline,
            cylinder_pipeline,
            atom_highlight_pipeline,
            atom_halo_pipeline,
            halo_instances: None,
            halo_cpu: Mutex::new(Vec::new()),
            cylinder_highlight_pipeline,
            cylinder_vertex_buffer,
            cylinder_index_buffer,
            cylinder_index_count: cylinder_indices.len() as u32,
            glyph_bind_group,
            text_pipeline,
            text_instances: None,
            text_overlay_pipeline,
            text_overlay_pipeline_single,
            overlay_text_instances: None,
            envelope_pipeline,
            envelope_vertices: None,
            envelope_ranges: Vec::new(),
            envelope_eye: Mutex::new([0.0; 3]),
            atom_instances: None,
            bond_instances: None,
            atom_highlight_instances: None,
            bond_highlight_instances: None,
            measurement_instances: None,
            isosurface_pipeline,
            isosurface_pipeline_ao,
            isosurface_gbuffer_pipeline,
            isosurface_material_buffer,
            isosurface_material_bind_group,
            isosurface_vertices: None,
            translucent_pipeline,
            translucent_vertices: None,
            translucent_cpu: Mutex::new(TranslucentTriangles::default()),
            atom_pipeline_ao,
            cylinder_pipeline_ao,
            ao_sample_bind_group_layout,
            atom_gbuffer_pipeline,
            cylinder_gbuffer_pipeline,
            ssao_bind_group_layout,
            ssao_pipeline,
            blur_bind_group_layout,
            blur_pipeline,
            ao_live: None,
            dof_blur_bind_group_layout,
            dof_blur_pipeline,
            dof_composite_bind_group_layout,
            dof_composite_pipeline,
            dof_blit_bind_group_layout,
            dof_blit_pipeline,
            dof_live: None,
        }
    }

    /// Initial upload when a molecule is opened — everything visible, no
    /// selection, all bonds solid, no measurements.
    pub fn load_molecule(&mut self, device: &wgpu::Device, molecule: &Molecule) {
        let sizes = DrawnSizes::ball_and_stick(molecule, 1.0, 1.0);
        self.update_geometry(device, molecule, &HashSet::new(), &HashSet::new(), &[], None, None, &sizes);
        self.update_highlights(device, molecule, &[], &[], &sizes);
        self.update_measurements(device, molecule, &[], [0.0, 0.0, 0.0]);
    }

    /// Rebuilds the atom/bond instance buffers from current hide/style
    /// state. A rare, user-triggered event (a button click), not a
    /// per-frame cost — cheap to just rebuild outright rather than track
    /// incremental diffs, even for much larger molecules than this project
    /// currently targets.
    #[allow(clippy::too_many_arguments)]
    pub fn update_geometry(
        &mut self,
        device: &wgpu::Device,
        molecule: &Molecule,
        hidden_atoms: &HashSet<usize>,
        hidden_bonds: &HashSet<usize>,
        bond_styles: &[BondVisualStyle],
        atom_colors: Option<&[[f32; 3]]>,
        bond_scales: Option<&[f32]>,
        sizes: &DrawnSizes,
    ) {
        let atom_data = build_atom_instances(molecule, hidden_atoms, atom_colors, &sizes.base);
        let bond_data = if sizes.bond_radius.is_some() {
            build_bond_instances(molecule, hidden_atoms, hidden_bonds, bond_styles, atom_colors, bond_scales)
        } else {
            Vec::new()
        };

        let atom_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atom_instance_buffer"),
            contents: bytemuck::cast_slice(&atom_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let bond_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bond_instance_buffer"),
            contents: bytemuck::cast_slice(&bond_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        self.atom_instances = Some((atom_buffer, atom_data.len() as u32));
        self.bond_instances = Some((bond_buffer, bond_data.len() as u32));
    }

    /// Rebuilds the small overlay buffers for the current selection.
    pub fn update_highlights(
        &mut self,
        device: &wgpu::Device,
        molecule: &Molecule,
        selected_atoms: &[usize],
        selected_bonds: &[usize],
        sizes: &DrawnSizes,
    ) {
        let atom_data = build_atom_highlight_instances(molecule, selected_atoms, sizes);
        let bond_data = if sizes.bond_radius.is_some() { build_bond_highlight_instances(molecule, selected_bonds) } else { Vec::new() };

        let atom_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atom_highlight_instance_buffer"),
            contents: bytemuck::cast_slice(&atom_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let bond_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bond_highlight_instance_buffer"),
            contents: bytemuck::cast_slice(&bond_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        self.atom_highlight_instances = Some((atom_buffer, atom_data.len() as u32));
        self.bond_highlight_instances = Some((bond_buffer, bond_data.len() as u32));
    }

    /// Rebuilds the Measure-window line buffer. `segments` are
    /// atom-index pairs (see `MeasurementKind::segments`), flattened across
    /// every committed measurement by the caller.
    pub fn update_measurements(&mut self, device: &wgpu::Device, molecule: &Molecule, segments: &[(usize, usize)], color: [f32; 3]) {
        let data = build_measurement_instances(molecule, segments, color);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("measurement_instance_buffer"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.measurement_instances = Some((buffer, data.len() as u32));
    }

    /// Rebuilds the label glyph-instance buffer. Called every frame
    /// (unlike the other `update_*` methods, which only run on a discrete
    /// state change) — label world size depends on distance from the
    /// current camera position for constant apparent size, so it has to
    /// be recomputed whenever the camera might have moved. Only actually
    /// runs when a repaint happens at all (egui only repaints when
    /// something's interacting), so this doesn't mean constant background
    /// work — just cheap work exactly when there's already a frame to draw.
    /// Uploads this frame's labels: depth-tested ones, and those marked
    /// `on_top` (drawn over everything, last).
    pub fn update_labels(&mut self, device: &wgpu::Device, instances: &[GlyphInstance]) {
        let upload = |label: &str, glyphs: Vec<GlyphInstance>| {
            (!glyphs.is_empty()).then(|| {
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(&glyphs),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                (buffer, glyphs.len() as u32)
            })
        };
        let (on_top, in_scene): (Vec<GlyphInstance>, Vec<GlyphInstance>) = instances.iter().partition(|g| g.on_top > 0.5);
        self.text_instances = upload("text_instance_buffer", in_scene);
        self.overlay_text_instances = upload("overlay_text_instance_buffer", on_top);
    }

    /// Uploads the envelopes (see `envelope.wgsl`): all their triangles in
    /// one buffer, with one range per envelope.
    pub fn update_envelopes(&mut self, device: &wgpu::Device, vertices: &[IsosurfaceVertex], ranges: &[EnvelopeRange]) {
        if vertices.is_empty() {
            self.envelope_vertices = None;
            self.envelope_ranges.clear();
            return;
        }
        self.envelope_vertices = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("envelope_vertex_buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        self.envelope_ranges = ranges.to_vec();
    }

    /// Uploads the atom halos: one instance per atom, `vdw_radius` such
    /// that `vdw_radius * atom_scale` is the halo's radius.
    pub fn update_halos(&mut self, device: &wgpu::Device, halos: &[AtomInstance]) {
        *self.halo_cpu.lock().unwrap() = halos.to_vec();
        self.halo_instances = (!halos.is_empty()).then(|| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("halo_instance_buffer"),
                contents: bytemuck::cast_slice(halos),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            (buffer, halos.len() as u32)
        });
    }

    /// Envelopes farthest from the camera first, so overlapping clouds
    /// blend in the right order.
    fn draw_envelopes(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        let Some(buffer) = &self.envelope_vertices else { return };
        let eye = *self.envelope_eye.lock().unwrap();
        let distance = |r: &EnvelopeRange| (0..3).map(|i| (r.center[i] - eye[i]).powi(2)).sum::<f32>();
        let mut order: Vec<&EnvelopeRange> = self.envelope_ranges.iter().collect();
        order.sort_by(|a, b| distance(b).total_cmp(&distance(a)));
        render_pass.set_pipeline(&self.envelope_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        for range in order {
            render_pass.draw(range.first_vertex..range.first_vertex + range.vertex_count, 0..1);
        }
    }

    /// Rebuilds the isosurface vertex buffer — called whenever the caller
    /// (`App`) changes anything that affects what should be drawn: the
    /// active structure's own isovalue/refinement/colors/opacity, whether
    /// it's shown at all, or the "kept" surfaces list. `vertices` is
    /// already the full combined set (live + kept) — see
    /// `crate::isosurface_mesh::push_isosurface_vertices`.
    /// Uploads every shown surface (a triangle soup), split into the
    /// opaque ones and the translucent ones; the latter are sorted for the
    /// camera at the next `write_uniforms`.
    pub fn update_isosurface(&mut self, device: &wgpu::Device, vertices: &[IsosurfaceVertex]) {
        let (translucent, opaque): (Vec<IsosurfaceVertex>, Vec<IsosurfaceVertex>) = vertices.iter().partition(|v| is_translucent(v));
        let upload = |label: &str, vertices: &[IsosurfaceVertex], usage: wgpu::BufferUsages| {
            (!vertices.is_empty()).then(|| {
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX | usage,
                });
                (buffer, vertices.len() as u32)
            })
        };
        self.isosurface_vertices = upload("isosurface_vertex_buffer", &opaque, wgpu::BufferUsages::empty());
        self.translucent_vertices = upload("translucent_vertex_buffer", &translucent, wgpu::BufferUsages::COPY_DST);
        *self.translucent_cpu.lock().unwrap() = TranslucentTriangles { triangles: translucent, sorted_for: None };
    }

    pub fn update_isosurface_material(&self, queue: &wgpu::Queue, material: &IsosurfaceMaterial) {
        queue.write_buffer(&self.isosurface_material_buffer, 0, bytemuck::bytes_of(material));
    }

    /// Public alongside `draw_into_pass` for the same reason — a headless
    /// test simulating the live view's `prepare`/`paint` split needs to
    /// upload the scene uniforms itself, same as `ViewportCallback` does.
    pub fn write_uniforms(&self, queue: &wgpu::Queue, uniforms: &SceneUniforms) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
        let e = uniforms.camera_eye;
        let e3 = [e[0], e[1], e[2]];
        *self.envelope_eye.lock().unwrap() = e3;
        // Halos farthest first, for this camera.
        if let Some((buffer, _)) = &self.halo_instances {
            let mut halos = self.halo_cpu.lock().unwrap();
            let distance = |h: &AtomInstance| (0..3).map(|i| (h.center[i] - e[i]).powi(2)).sum::<f32>();
            halos.sort_by(|a, b| distance(b).total_cmp(&distance(a)));
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&halos));
        }
        // Translucent surface triangles farthest first, for this camera.
        if let Some((buffer, _)) = &self.translucent_vertices {
            let mut translucent = self.translucent_cpu.lock().unwrap();
            if translucent.sorted_for != Some(e3) {
                let sorted = sort_back_to_front(&translucent.triangles, glam::Vec3::from_array(e3));
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(&sorted));
                translucent.sorted_for = Some(e3);
            }
        }
    }

    /// The actual draw call sequence — shared by the live egui-wgpu
    /// callback (`ViewportCallback::paint`, below) and the offscreen PNG
    /// export path (`render_offscreen`), so the two can never drift apart.
    /// `ao`, when `Some`, switches the atom/cylinder draws to the
    /// AO-sampling pipelines (`atom_pipeline_ao`/`cylinder_pipeline_ao`)
    /// and binds it at group 1 — everything else (highlights, text,
    /// isosurfaces) is unaffected either way, matching the feature's scope.
    /// Public so a headless test can exercise exactly the same live-view
    /// draw path `ViewportCallback::paint` uses, without needing egui.
    pub fn draw_into_pass(&self, render_pass: &mut wgpu::RenderPass<'_>, ao: Option<&wgpu::BindGroup>) {
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        if let Some(ao_bind_group) = ao {
            render_pass.set_bind_group(1, ao_bind_group, &[]);
        }

        self.draw_cylinders(render_pass, if ao.is_some() { &self.cylinder_pipeline_ao } else { &self.cylinder_pipeline }, &self.bond_instances);
        self.draw_spheres(render_pass, if ao.is_some() { &self.atom_pipeline_ao } else { &self.atom_pipeline }, &self.atom_instances);

        // Measure-window lines — opaque and depth-tested like
        // real bonds (via the same pipeline, reusing its dashed-thin-line
        // path), so they correctly interleave with the molecule.
        self.draw_cylinders(render_pass, &self.cylinder_pipeline, &self.measurement_instances);

        // 3D labels (atom tags, measurement values) — real depth-tested
        // billboard glyph quads, drawn opaque-ish (alpha-to-coverage, see
        // the pipeline) so they're correctly, precisely occluded by
        // whatever's actually in front of them.
        if let Some((buffer, count)) = &self.text_instances
            && *count > 0
        {
            render_pass.set_pipeline(&self.text_pipeline);
            render_pass.set_bind_group(1, &self.glyph_bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..6, 0..*count);
        }

        // Isosurfaces (Phase 2 .cube orbitals/densities) — translucent lit
        // mesh, drawn after the opaque/text passes so it's correctly
        // depth-tested against them, before the highlight overlay so a
        // selection highlight (if any) still reads on top.
        if let Some((buffer, count)) = &self.isosurface_vertices
            && *count > 0
        {
            render_pass.set_pipeline(if ao.is_some() { &self.isosurface_pipeline_ao } else { &self.isosurface_pipeline });
            render_pass.set_bind_group(1, &self.isosurface_material_bind_group, &[]);
            if let Some(ao_bind_group) = ao {
                render_pass.set_bind_group(2, ao_bind_group, &[]);
            }
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..*count, 0..1);
        }
        // Translucent surfaces over everything opaque, already sorted
        // farthest first (`write_uniforms`).
        if let Some((buffer, count)) = &self.translucent_vertices {
            render_pass.set_pipeline(&self.translucent_pipeline);
            render_pass.set_bind_group(1, &self.isosurface_material_bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..*count, 0..1);
        }

        // Envelopes and halos over the finished opaque scene and isosurfaces.
        self.draw_envelopes(render_pass);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        self.draw_spheres(render_pass, &self.atom_halo_pipeline, &self.halo_instances);

        // Highlight overlays last, alpha-blended on top of the opaque pass.
        self.draw_cylinders(render_pass, &self.cylinder_highlight_pipeline, &self.bond_highlight_instances);
        self.draw_spheres(render_pass, &self.atom_highlight_pipeline, &self.atom_highlight_instances);
    }

    /// Draws cylinder instances (bonds, measurement lines, their
    /// highlights): the shared unit cylinder mesh, once per instance.
    fn draw_cylinders(&self, render_pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, instances: &Option<(wgpu::Buffer, u32)>) {
        let Some((buffer, count)) = instances else { return };
        if *count == 0 {
            return;
        }
        render_pass.set_pipeline(pipeline);
        render_pass.set_vertex_buffer(0, self.cylinder_vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, buffer.slice(..));
        render_pass.set_index_buffer(self.cylinder_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.cylinder_index_count, 0, 0..*count);
    }

    /// Draws sphere instances (atoms, halos, highlights): one impostor quad
    /// (6 vertices, no vertex buffer) per instance.
    fn draw_spheres(&self, render_pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, instances: &Option<(wgpu::Buffer, u32)>) {
        let Some((buffer, count)) = instances else { return };
        if *count == 0 {
            return;
        }
        render_pass.set_pipeline(pipeline);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..6, 0..*count);
    }

    /// Value labels, over everything: drawn after the scene and after
    /// depth of field (which would otherwise blur them with whatever lies
    /// behind them), so they are always sharp and never cut by an atom.
    /// `single_sample` selects the pipeline for a pass without MSAA or
    /// depth (an export's depth-of-field output).
    pub fn draw_overlay_labels(&self, render_pass: &mut wgpu::RenderPass<'_>, single_sample: bool) {
        let Some((buffer, count)) = &self.overlay_text_instances else { return };
        render_pass.set_pipeline(if single_sample { &self.text_overlay_pipeline_single } else { &self.text_overlay_pipeline });
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &self.glyph_bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..6, 0..*count);
    }

    /// The depth+normal G-buffer pre-pass (for ambient occlusion and depth
    /// of field) into `normal_view`/`depth_view` — atoms, bonds, and the
    /// isosurface (text and highlights still don't participate: text is a
    /// billboard overlay with no meaningful occlusion contribution, and
    /// highlights are a flat translucent tint drawn on top of everything
    /// else, not real geometry), using the `fs_gbuffer` pipelines instead
    /// of the normal shaded ones.
    fn run_gbuffer_pass(&self, encoder: &mut wgpu::CommandEncoder, normal_view: &wgpu::TextureView, depth_view: &wgpu::TextureView) {
        let mut render_pass = begin_pass(
            encoder,
            "gbuffer_pass",
            normal_view,
            None,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            Some((depth_view, wgpu::StoreOp::Store)),
        );
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        self.draw_cylinders(&mut render_pass, &self.cylinder_gbuffer_pipeline, &self.bond_instances);
        self.draw_spheres(&mut render_pass, &self.atom_gbuffer_pipeline, &self.atom_instances);

        // Isosurface — see `isosurface.wgsl`'s `fs_gbuffer` doc for why
        // this participates too. `isosurface_gbuffer_pipeline` reuses the
        // 2-group isosurface pipeline layout even though `fs_gbuffer`
        // doesn't read group 1 at all, so it still needs *some* compatible
        // bind group there — the real material bind group is as good as
        // any, and avoids a third pipeline layout just for this.
        if let Some((buffer, count)) = &self.isosurface_vertices
            && *count > 0
        {
            render_pass.set_pipeline(&self.isosurface_gbuffer_pipeline);
            render_pass.set_bind_group(1, &self.isosurface_material_bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..*count, 0..1);
        }
    }

    /// Runs the ambient-occlusion pre-passes — depth+normal G-buffer,
    /// screen-space sampled occlusion, depth-aware blur — into the given
    /// (already sized and created) textures, and returns a bind group
    /// ready to pass as `draw_into_pass`'s `ao` argument. Shared by the
    /// live view (persistent textures, see `ensure_ao_textures`) and PNG
    /// export (fresh textures each call, see `render_offscreen`) — the
    /// two differ only in texture lifetime and `sample_count` (export can
    /// afford far more since it's one-shot and blocking; the live view
    /// reruns this every frame the camera moves). All work is recorded
    /// into `encoder`, not submitted here.
    #[allow(clippy::too_many_arguments)]
    fn run_ao_passes(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        gbuffer_normal_view: &wgpu::TextureView,
        gbuffer_depth_view: &wgpu::TextureView,
        ao_raw_view: &wgpu::TextureView,
        blur_h_view: &wgpu::TextureView,
        blur_v_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        view_proj: glam::Mat4,
        camera_eye: glam::Vec3,
        sample_count: u32,
        ao_settings: &AoSettings,
        // Top-left of these textures' region within the shared render
        // target `fs_main_ao` actually draws into, in physical pixels —
        // `[0.0, 0.0]` for export (no such sub-region), the live
        // viewport's own on-screen position for the live view. See
        // `AoSampleUniforms::offset`.
        sample_offset: [f32; 2],
    ) -> wgpu::BindGroup {
        self.run_gbuffer_pass(encoder, gbuffer_normal_view, gbuffer_depth_view);

        let ao_uniforms = AoUniforms::new(view_proj.inverse(), view_proj, camera_eye, width, height, sample_count, ao_settings);
        let ao_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ao_uniform_buffer"),
            contents: bytemuck::bytes_of(&ao_uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let ssao_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssao_bind_group"),
            layout: &self.ssao_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ao_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(gbuffer_depth_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(gbuffer_normal_view) },
            ],
        });
        fullscreen_pass(encoder, "ao_ssao_pass", ao_raw_view, wgpu::Color::WHITE, &self.ssao_pipeline, &ssao_bind_group);

        // Separable depth-aware blur: two passes (horizontal, then
        // vertical) — a render pass can't read and write the same
        // texture, so each direction needs its own destination
        // (`ao_raw` -> `blur_h` -> `blur_v`).
        let run_blur_pass = |encoder: &mut wgpu::CommandEncoder, direction: [f32; 2], input_view: &wgpu::TextureView, output_view: &wgpu::TextureView| {
            let blur_uniforms = crate::ao::BlurUniforms::new(direction, width, height);
            let blur_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("blur_uniform_buffer"),
                contents: bytemuck::bytes_of(&blur_uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let blur_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur_bind_group"),
                layout: &self.blur_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: blur_uniform_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(gbuffer_depth_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(input_view) },
                    wgpu::BindGroupEntry { binding: 3, resource: ao_uniform_buffer.as_entire_binding() },
                ],
            });
            fullscreen_pass(encoder, "ao_blur_pass", output_view, wgpu::Color::WHITE, &self.blur_pipeline, &blur_bind_group);
        };
        run_blur_pass(encoder, [1.0, 0.0], ao_raw_view, blur_h_view);
        run_blur_pass(encoder, [0.0, 1.0], blur_h_view, blur_v_view);

        let sample_uniforms = crate::ao::AoSampleUniforms::new(view_proj.inverse(), width, height, sample_offset, ao_settings);
        let sample_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ao_sample_uniform_buffer"),
            contents: bytemuck::bytes_of(&sample_uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ao_sample_bind_group"),
            layout: &self.ao_sample_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sample_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(gbuffer_depth_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(blur_v_view) },
            ],
        })
    }

    /// Runs depth of field over an already rendered scene: a separable
    /// blur of `targets.sharp` — plain Gaussian, not depth-aware (see
    /// `dof.rs`'s module doc for why it should bleed across depth edges,
    /// unlike AO's own blur) — then a composite into `targets.output`
    /// mixing sharp and fully blurred per pixel by world-space distance
    /// from the focal plane. Shared by the live view and export.
    #[allow(clippy::too_many_arguments)]
    fn run_dof_passes(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        targets: &DofTargets<'_>,
        width: u32,
        height: u32,
        view_proj: glam::Mat4,
        camera_eye: glam::Vec3,
        focus_distance: f32,
        dof_settings: &DofSettings,
        clear_color: wgpu::Color,
    ) {
        let run_blur = |encoder: &mut wgpu::CommandEncoder, direction: [f32; 2], input_view: &wgpu::TextureView, output_view: &wgpu::TextureView| {
            let blur_uniforms = DofBlurUniforms::new(direction, width, height, dof_settings.strength);
            let blur_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dof_blur_uniform_buffer"),
                contents: bytemuck::bytes_of(&blur_uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let blur_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dof_blur_bind_group"),
                layout: &self.dof_blur_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: blur_uniform_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(input_view) },
                ],
            });
            fullscreen_pass(encoder, "dof_blur_pass", output_view, wgpu::Color::TRANSPARENT, &self.dof_blur_pipeline, &blur_bind_group);
        };
        run_blur(encoder, [1.0, 0.0], targets.sharp, targets.blur_h);
        run_blur(encoder, [0.0, 1.0], targets.blur_h, targets.blur_v);

        let composite_uniforms = DofCompositeUniforms::new(view_proj.inverse(), camera_eye, focus_distance, width, height, dof_settings);
        let composite_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dof_composite_uniform_buffer"),
            contents: bytemuck::bytes_of(&composite_uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dof_composite_bind_group"),
            layout: &self.dof_composite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: composite_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(targets.depth) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(targets.sharp) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(targets.blur_v) },
            ],
        });
        fullscreen_pass(encoder, "dof_composite_pass", targets.output, clear_color, &self.dof_composite_pipeline, &composite_bind_group);
    }

    /// (Re)creates the live view's persistent G-buffer/AO textures if
    /// they don't exist yet or the viewport was resized — cheap to check,
    /// only actually reallocates on a real size change. Unlike export
    /// (fresh textures every call, since export resolution is one-shot
    /// and often unrelated to the live viewport size), the live view
    /// reruns the AO passes every frame the camera moves and can't afford
    /// to reallocate that often.
    fn ensure_ao_textures(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if let Some(existing) = &self.ao_live
            && existing.width == width && existing.height == height
        {
            return;
        }
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let make_texture = |label: &str, format: wgpu::TextureFormat| {
            render_texture(device, label, size, format, 1, wgpu::TextureUsages::TEXTURE_BINDING).create_view(&wgpu::TextureViewDescriptor::default())
        };

        let gbuffer_normal_view = make_texture("ao_live_gbuffer_normal", GBUFFER_NORMAL_FORMAT);
        let gbuffer_depth_view = make_texture("ao_live_gbuffer_depth", DEPTH_FORMAT);
        let ao_raw_view = make_texture("ao_live_raw", AO_FORMAT);
        let blur_h_view = make_texture("ao_live_blur_h", AO_FORMAT);
        let blur_v_view = make_texture("ao_live_blur_v", AO_FORMAT);

        // A throwaway bind group — real one gets built fresh every frame
        // in `run_live_ao_pass` (the uniform buffer's contents change
        // with the camera), this just satisfies the struct until then.
        let placeholder_uniforms = crate::ao::AoSampleUniforms::new(glam::Mat4::IDENTITY, width, height, [0.0, 0.0], &AoSettings::default());
        let placeholder_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ao_live_placeholder_uniform_buffer"),
            contents: bytemuck::bytes_of(&placeholder_uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let sample_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ao_live_sample_bind_group"),
            layout: &self.ao_sample_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: placeholder_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&gbuffer_depth_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&blur_v_view) },
            ],
        });

        self.ao_live =
            Some(AoLiveTextures { width, height, gbuffer_normal_view, gbuffer_depth_view, ao_raw_view, blur_h_view, blur_v_view, sample_bind_group });
    }

    /// Runs the live view's AO pre-passes for this frame into the
    /// persistent textures (creating/resizing them first if needed) and
    /// updates `ao_live`'s bind group so `draw_into_pass` picks it up.
    /// Called from `ViewportCallback::prepare`, which runs before the
    /// paint pass egui hands the callback — the one place the live view
    /// can open extra render passes at all (`paint` is a callback inside
    /// egui's own pass and cannot).
    ///
    /// `sample_count` is the caller's call — `App` runs this at a cheap
    /// tier (`AO_LIVE_SAMPLE_COUNT`) every frame the camera is actually
    /// moving, then once at full quality (`AO_KERNEL_SIZE`, the same
    /// export uses) the moment it settles, and skips calling this at all
    /// on subsequent idle frames — see the "Phase C" settle logic in
    /// `App`'s viewport code.
    #[allow(clippy::too_many_arguments)]
    pub fn run_live_ao_pass(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        view_proj: glam::Mat4,
        camera_eye: glam::Vec3,
        ao_settings: &AoSettings,
        sample_count: u32,
        // The live 3D viewport's own top-left position within the full
        // window, in physical pixels — see `AoSampleUniforms::offset`
        // for why this matters (the viewport is a sub-rect of a larger
        // shared window, not the whole render target).
        viewport_offset_px: [f32; 2],
    ) {
        self.ensure_ao_textures(device, width, height);
        let Some(live) = &self.ao_live else { return };
        let bind_group = self.run_ao_passes(
            device,
            encoder,
            &live.gbuffer_normal_view,
            &live.gbuffer_depth_view,
            &live.ao_raw_view,
            &live.blur_h_view,
            &live.blur_v_view,
            width,
            height,
            view_proj,
            camera_eye,
            sample_count,
            ao_settings,
            viewport_offset_px,
        );
        if let Some(live) = &mut self.ao_live {
            live.sample_bind_group = bind_group;
        }
    }

    /// The live view's current-frame AO bind group, if `run_live_ao_pass`
    /// has populated one — `None` before the first frame with AO enabled,
    /// or whenever it's off (the caller shouldn't call this then, but
    /// returning `None` either way is harmless).
    pub fn live_ao_bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.ao_live.as_ref().map(|live| &live.sample_bind_group)
    }

    /// (Re)creates the live view's persistent depth-of-field textures if
    /// they don't exist yet or the viewport was resized.
    fn ensure_dof_textures(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if let Some(existing) = &self.dof_live
            && existing.width == width && existing.height == height
        {
            return;
        }
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let color_format = self.target_format;

        let make_texture = |label: &str, format: wgpu::TextureFormat, sample_count: u32, extra_usage: wgpu::TextureUsages| {
            render_texture(device, label, size, format, sample_count, extra_usage).create_view(&wgpu::TextureViewDescriptor::default())
        };

        let scene_msaa_color_view = make_texture("dof_live_scene_msaa_color", color_format, MSAA_SAMPLES, wgpu::TextureUsages::empty());
        let scene_resolve_view = make_texture("dof_live_scene_resolve", color_format, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let scene_msaa_depth_view = make_texture("dof_live_scene_msaa_depth", DEPTH_FORMAT, MSAA_SAMPLES, wgpu::TextureUsages::empty());
        let gbuffer_normal_view = make_texture("dof_live_gbuffer_normal", GBUFFER_NORMAL_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let gbuffer_depth_view = make_texture("dof_live_gbuffer_depth", DEPTH_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let ao_raw_view = make_texture("dof_live_ao_raw", AO_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let ao_blur_h_view = make_texture("dof_live_ao_blur_h", AO_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let ao_blur_v_view = make_texture("dof_live_ao_blur_v", AO_FORMAT, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let dof_blur_h_view = make_texture("dof_live_blur_h", color_format, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let dof_blur_v_view = make_texture("dof_live_blur_v", color_format, 1, wgpu::TextureUsages::TEXTURE_BINDING);
        let dof_output_view = make_texture("dof_live_output", color_format, 1, wgpu::TextureUsages::TEXTURE_BINDING);

        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dof_live_blit_bind_group"),
            layout: &self.dof_blit_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&dof_output_view) }],
        });

        self.dof_live = Some(DofLiveTextures {
            width,
            height,
            scene_msaa_color_view,
            scene_resolve_view,
            scene_msaa_depth_view,
            gbuffer_normal_view,
            gbuffer_depth_view,
            ao_raw_view,
            ao_blur_h_view,
            ao_blur_v_view,
            dof_blur_h_view,
            dof_blur_v_view,
            dof_output_view,
            blit_bind_group,
        });
    }

    /// Runs the live view's full depth-of-field pipeline for this frame:
    /// (optionally) AO's G-buffer/SSAO/blur passes at `offset = [0, 0]`
    /// (see `DofLiveTextures`'s doc for why this can't reuse `ao_live`),
    /// the whole scene draw into a private offscreen texture, a separable
    /// blur of that result, and a final composite mixing sharp/blurred by
    /// distance from the focal plane — everything `paint`'s blit later
    /// needs is left ready in `dof_live.dof_output_view`. Called from
    /// `ViewportCallback::prepare` instead of `run_live_ao_pass` whenever
    /// depth of field is enabled (DoF subsumes AO's own live pass in that
    /// case — see `App`'s wiring).
    #[allow(clippy::too_many_arguments)]
    pub fn run_live_dof_pass(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        view_proj: glam::Mat4,
        camera_eye: glam::Vec3,
        background: [f32; 3],
        ao_settings: Option<&AoSettings>,
        ao_sample_count: u32,
        dof_settings: &DofSettings,
        focus_distance: f32,
    ) {
        self.ensure_dof_textures(device, width, height);
        let Some(live) = &self.dof_live else { return };

        let ao_bind_group = if let Some(ao_settings) = ao_settings {
            Some(self.run_ao_passes(
                device,
                encoder,
                &live.gbuffer_normal_view,
                &live.gbuffer_depth_view,
                &live.ao_raw_view,
                &live.ao_blur_h_view,
                &live.ao_blur_v_view,
                width,
                height,
                view_proj,
                camera_eye,
                ao_sample_count,
                ao_settings,
                [0.0, 0.0],
            ))
        } else {
            // Still need scene depth for the DoF composite even without AO
            // shading — a bare depth+normal prepass, the same pipelines
            // AO's own G-buffer pass uses, just without the SSAO/blur that
            // would otherwise follow it.
            self.run_gbuffer_pass(encoder, &live.gbuffer_normal_view, &live.gbuffer_depth_view);
            None
        };

        let clear_color = wgpu::Color { r: background[0] as f64, g: background[1] as f64, b: background[2] as f64, a: 1.0 };
        {
            let mut render_pass = begin_pass(
                encoder,
                "dof_scene_pass",
                &live.scene_msaa_color_view,
                Some(&live.scene_resolve_view),
                wgpu::LoadOp::Clear(clear_color),
                Some((&live.scene_msaa_depth_view, wgpu::StoreOp::Discard)),
            );
            self.draw_into_pass(&mut render_pass, ao_bind_group.as_ref());
        }

        let targets = DofTargets {
            sharp: &live.scene_resolve_view,
            depth: &live.gbuffer_depth_view,
            blur_h: &live.dof_blur_h_view,
            blur_v: &live.dof_blur_v_view,
            output: &live.dof_output_view,
        };
        self.run_dof_passes(device, encoder, &targets, width, height, view_proj, camera_eye, focus_distance, dof_settings, clear_color);
    }

    /// The live view's current-frame DoF composite, if `run_live_dof_pass`
    /// has populated one — same `None`-before-first-frame-or-when-off
    /// shape as `live_ao_bind_group`.
    pub fn live_dof_bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.dof_live.as_ref().map(|live| &live.blit_bind_group)
    }

    /// Draws the single fullscreen-triangle quad that blits a finished DoF
    /// composite into the live view's shared window pass — see
    /// `dof.wgsl`'s module doc for why this is safe to do without any
    /// window-offset correction.
    pub fn blit_dof_output(&self, render_pass: &mut wgpu::RenderPass<'_>, dof_bind_group: &wgpu::BindGroup) {
        render_pass.set_pipeline(&self.dof_blit_pipeline);
        render_pass.set_bind_group(0, dof_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    /// Renders the scene already uploaded into this `ViewportResources`
    /// (atom/bond/highlight geometry is pure world-space data, independent
    /// of camera or output size, so it doesn't need re-uploading here) to
    /// an offscreen texture and reads it back as RGBA8 pixels — for PNG
    /// export, not the live view. `uniforms` should already reflect the
    /// camera/material/aspect-ratio to render with, and `label_instances`
    /// the already-laid-out glyph quads (the caller — `App`, in the app
    /// crate — is what knows about atom-label mode, measurement text, etc,
    /// same division of responsibility as the live view).
    ///
    /// Blocks the calling thread until the GPU finishes and the readback
    /// completes. That's fine for a one-shot, explicitly user-triggered
    /// export — this is not called every frame.
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        uniforms: &SceneUniforms,
        label_instances: &[GlyphInstance],
        settings: &crate::export::ExportSettings,
    ) -> Result<Vec<u8>, String> {
        self.write_uniforms(queue, uniforms);
        self.update_labels(device, label_instances);

        let supersample = settings.supersample.max(1);
        let render_width = settings.width * supersample;
        let render_height = settings.height * supersample;
        if render_width == 0 || render_height == 0 {
            return Err("export resolution must be non-zero".to_string());
        }

        let size = wgpu::Extent3d { width: render_width, height: render_height, depth_or_array_layers: 1 };

        let msaa_view = render_texture(device, "export_msaa_color", size, target_format, MSAA_SAMPLES, wgpu::TextureUsages::empty())
            .create_view(&wgpu::TextureViewDescriptor::default());

        // `TEXTURE_BINDING` is only actually needed when depth of field is
        // on (it becomes the "sharp" input to the DoF blur/composite
        // passes) — harmless to always request, and simpler than branching
        // the descriptor on `settings.depth_of_field`.
        let resolve_color = render_texture(
            device,
            "export_resolve_color",
            size,
            target_format,
            1,
            wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let resolve_view = resolve_color.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_view = render_texture(device, "export_depth", size, DEPTH_FORMAT, MSAA_SAMPLES, wgpu::TextureUsages::empty())
            .create_view(&wgpu::TextureViewDescriptor::default());

        let clear_color = settings.background.map_or(wgpu::Color::TRANSPARENT, |[r, g, b, a]| wgpu::Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: a as f64,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("export_encoder") });

        // Returns the texture too, not just its view — most call sites
        // only need the view, but the final DoF-composite output needs
        // the underlying `wgpu::Texture` for the readback copy below
        // (hence `COPY_SRC` on every texture this makes — harmless to
        // request even where unused).
        let make_texture = |label: &str, format: wgpu::TextureFormat| -> (wgpu::Texture, wgpu::TextureView) {
            let texture = render_texture(device, label, size, format, 1, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let make_view = |label: &str, format: wgpu::TextureFormat| make_texture(label, format).1;

        let view_proj = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
        let camera_eye = glam::Vec3::new(uniforms.camera_eye[0], uniforms.camera_eye[1], uniforms.camera_eye[2]);

        // AO's pre-passes must run before the main draw now — the main
        // atom/cylinder shaders sample their finished output directly
        // (see `ao.rs`'s module doc for why), rather than a separate pass
        // compositing over the main draw's result afterward. Also returns
        // the G-buffer depth view when AO ran, so depth of field (below)
        // can reuse it instead of paying for a second depth prepass.
        let (ao_bind_group, ao_gbuffer_depth_view) = if let Some(ao_settings) = &settings.ambient_occlusion {
            let gbuffer_normal_view = make_view("export_ao_gbuffer_normal", GBUFFER_NORMAL_FORMAT);
            let gbuffer_depth_view = make_view("export_ao_gbuffer_depth", DEPTH_FORMAT);
            let ao_raw_view = make_view("export_ao_raw", AO_FORMAT);
            let blur_h_view = make_view("export_ao_blur_h", AO_FORMAT);
            let blur_v_view = make_view("export_ao_blur_v", AO_FORMAT);

            let bind_group = self.run_ao_passes(
                device,
                &mut encoder,
                &gbuffer_normal_view,
                &gbuffer_depth_view,
                &ao_raw_view,
                &blur_h_view,
                &blur_v_view,
                render_width,
                render_height,
                view_proj,
                camera_eye,
                crate::ao::AO_KERNEL_SIZE as u32,
                ao_settings,
                [0.0, 0.0],
            );
            (Some(bind_group), Some(gbuffer_depth_view))
        } else {
            (None, None)
        };

        {
            let mut render_pass = begin_pass(
                &mut encoder,
                "export_render_pass",
                &msaa_view,
                Some(&resolve_view),
                wgpu::LoadOp::Clear(clear_color),
                Some((&depth_view, wgpu::StoreOp::Discard)),
            );
            self.draw_into_pass(&mut render_pass, ao_bind_group.as_ref());
            if settings.depth_of_field.is_none() {
                self.draw_overlay_labels(&mut render_pass, false);
            }
        }

        // Depth of field (see `dof.rs`): blur the resolved scene color,
        // then composite sharp/blurred by distance from the focal plane,
        // into its own texture — read back from that instead of the plain
        // `resolve_color` below when it ran.
        let dof_output: Option<wgpu::Texture> = if let Some(dof_settings) = &settings.depth_of_field {
            let dof_depth_view = ao_gbuffer_depth_view.unwrap_or_else(|| {
                let normal_view = make_view("export_dof_gbuffer_normal", GBUFFER_NORMAL_FORMAT);
                let depth_view = make_view("export_dof_gbuffer_depth", DEPTH_FORMAT);
                self.run_gbuffer_pass(&mut encoder, &normal_view, &depth_view);
                depth_view
            });

            let blur_h_view = make_view("export_dof_blur_h", target_format);
            let blur_v_view = make_view("export_dof_blur_v", target_format);
            let (dof_output_texture, dof_output_view) = make_texture("export_dof_output", target_format);

            let targets =
                DofTargets { sharp: &resolve_view, depth: &dof_depth_view, blur_h: &blur_h_view, blur_v: &blur_v_view, output: &dof_output_view };
            self.run_dof_passes(
                device,
                &mut encoder,
                &targets,
                render_width,
                render_height,
                view_proj,
                camera_eye,
                settings.dof_focus_distance,
                dof_settings,
                clear_color,
            );

            // Value labels, sharp, over the blurred image.
            let mut label_pass = begin_pass(&mut encoder, "export_overlay_label_pass", &dof_output_view, None, wgpu::LoadOp::Load, None);
            self.draw_overlay_labels(&mut label_pass, true);
            drop(label_pass);

            Some(dof_output_texture)
        } else {
            None
        };

        // Row byte stride for a buffer copy must be padded to wgpu's
        // required alignment — the actual pixel data stays tightly packed
        // once we strip that padding back out below.
        let bytes_per_pixel = 4u32;
        let unpadded_bytes_per_row = render_width * bytes_per_pixel;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("export_readback_buffer"),
            size: (padded_bytes_per_row * render_height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let readback_source_texture = dof_output.as_ref().unwrap_or(&resolve_color);

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: readback_source_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback_buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_bytes_per_row), rows_per_image: Some(render_height) },
            },
            size,
        );

        queue.submit(Some(encoder.finish()));

        let slice = readback_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).map_err(|err| format!("GPU poll failed: {err}"))?;
        rx.recv().map_err(|_| "readback channel closed unexpectedly".to_string())?.map_err(|err| format!("failed to map readback buffer: {err}"))?;

        let padded = slice.get_mapped_range().map_err(|err| format!("failed to get mapped range: {err}"))?;
        let mut rendered = vec![0u8; (unpadded_bytes_per_row * render_height) as usize];
        for row in 0..render_height as usize {
            let src_start = row * padded_bytes_per_row as usize;
            let dst_start = row * unpadded_bytes_per_row as usize;
            rendered[dst_start..dst_start + unpadded_bytes_per_row as usize]
                .copy_from_slice(&padded[src_start..src_start + unpadded_bytes_per_row as usize]);
        }
        drop(padded);
        readback_buffer.unmap();

        let (final_pixels, _, _) = crate::export::downsample_rgba(&rendered, render_width, render_height, supersample);
        Ok(final_pixels)
    }
}

/// Per-frame paint callback: computes the current camera/material state and
/// draws the viewport contents into the rect egui allocated for it.
pub struct ViewportCallback {
    pub camera: OrbitCamera,
    pub material: Material,
    pub aspect_ratio: f32,
    /// Already laid out (see `layout_label`) by the caller, which knows
    /// about atoms/measurements/style — this crate just draws whatever
    /// glyph quads it's handed.
    pub label_instances: Vec<GlyphInstance>,
    /// `Some` runs the AO pre-passes for this frame at the given settings
    /// (see `ao.rs`) before the main draw — `None` skips them entirely,
    /// same real-vs-no-cost distinction as export's `ExportSettings`.
    pub ambient_occlusion: Option<AoSettings>,
    /// The 3D viewport's own rect, in physical pixels (not egui's logical
    /// points) — needed to size the AO G-buffer/textures to match exactly
    /// what `paint` will actually rasterize into. Unused when
    /// `ambient_occlusion` is `None`.
    pub viewport_size_px: [u32; 2],
    /// That same rect's top-left position within the *full window* —
    /// `paint`'s draw calls share the whole window's render target with
    /// every other egui panel, so `@builtin(position)` in `fs_main_ao` is
    /// in full-window coordinates, not viewport-relative. Without this,
    /// AO sampling reads the wrong pixels of the (viewport-sized) AO
    /// texture whenever the 3D view isn't flush with the window's
    /// top-left corner — which in practice is always, given the toolbar
    /// and any floating panels. Unused when `ambient_occlusion` is `None`.
    pub viewport_offset_px: [f32; 2],
    /// `Some(n)` reruns the AO pre-passes this frame at `n` samples;
    /// `None` skips them entirely and reuses whatever `ao_live`'s
    /// textures already hold (the previous frame's result — correct as
    /// long as the camera hasn't moved since). `App` drives this: cheap
    /// `n` every frame the camera is actively moving, `AO_KERNEL_SIZE`
    /// once the instant it settles, then `None` on every subsequent idle
    /// frame — the "Phase C" progressive-quality behavior. Unused when
    /// `ambient_occlusion` is `None`. Ignored (treated as "use full
    /// quality") when `depth_of_field` is `Some` — see that field's doc.
    pub ao_recompute_samples: Option<u32>,
    /// `Some` runs the full depth-of-field pipeline this frame (see
    /// `dof.rs`/`ViewportResources::run_live_dof_pass`) instead of
    /// drawing geometry directly into the shared pass — `paint` then just
    /// blits the finished composite. `None` skips it entirely and falls
    /// back to the direct draw (with `ambient_occlusion` applied inline
    /// as before, if set). Unlike AO's own settle-based skip, this reruns
    /// every frame the callback runs at all — there's no cheap way to
    /// reuse a stale composite the way `ao_live`'s bind group can be
    /// reused across idle frames, since `paint` has nothing else to draw
    /// when DoF is on.
    pub depth_of_field: Option<DofSettings>,
    /// The focal plane's distance from the camera, world units — always
    /// the camera's own orbit distance (see `dof.rs`'s module doc for why
    /// there's no separate focus-point control). Unused when
    /// `depth_of_field` is `None`.
    pub dof_focus_distance: f32,
    /// The live background color, needed to clear DoF's private offscreen
    /// texture — the direct-draw path instead relies on egui's own panel
    /// background fill, already drawn before this callback runs within
    /// the shared pass. Unused when `depth_of_field` is `None`.
    pub background: [f32; 3],
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut ViewportResources = callback_resources.get_mut().expect("ViewportResources not registered");
        let mut uniforms = SceneUniforms::new(&self.camera, self.aspect_ratio, &self.material);
        uniforms.set_srgb_target(resources.target_format().is_srgb());
        resources.write_uniforms(queue, &uniforms);
        resources.update_labels(device, &self.label_instances);

        let [width, height] = self.viewport_size_px;
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let view_proj = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
        let camera_eye = glam::Vec3::new(uniforms.camera_eye[0], uniforms.camera_eye[1], uniforms.camera_eye[2]);

        if let Some(dof_settings) = &self.depth_of_field {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("live_dof_encoder") });
            resources.run_live_dof_pass(
                device,
                &mut encoder,
                width,
                height,
                view_proj,
                camera_eye,
                self.background,
                self.ambient_occlusion.as_ref(),
                self.ao_recompute_samples.unwrap_or(crate::ao::AO_KERNEL_SIZE as u32),
                dof_settings,
                self.dof_focus_distance,
            );
            return vec![encoder.finish()];
        }

        let Some(ao_settings) = &self.ambient_occlusion else { return Vec::new() };
        let Some(sample_count) = self.ao_recompute_samples else { return Vec::new() };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("live_ao_encoder") });
        resources.run_live_ao_pass(device, &mut encoder, width, height, view_proj, camera_eye, ao_settings, sample_count, self.viewport_offset_px);
        vec![encoder.finish()]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let resources: &ViewportResources = callback_resources.get().expect("ViewportResources not registered");
        if self.depth_of_field.is_some() {
            if let Some(dof_bind_group) = resources.live_dof_bind_group() {
                resources.blit_dof_output(render_pass, dof_bind_group);
            }
            resources.draw_overlay_labels(render_pass, false);
            return;
        }
        let ao_bind_group = if self.ambient_occlusion.is_some() { resources.live_ao_bind_group() } else { None };
        resources.draw_into_pass(render_pass, ao_bind_group);
        resources.draw_overlay_labels(render_pass, false);
    }
}
