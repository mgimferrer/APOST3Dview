use std::collections::HashSet;

use wgpu::util::DeviceExt;

use apost3dview_core::Molecule;

use crate::camera::OrbitCamera;
use crate::consts::{DEPTH_FORMAT, MSAA_SAMPLES};
use crate::glyphs::GlyphAtlas;
use crate::instances::{
    build_atom_highlight_instances, build_atom_instances, build_bond_highlight_instances, build_bond_instances,
    build_measurement_instances, AtomInstance, BondInstance, BondVisualStyle,
};
use crate::label::GlyphInstance;
use crate::material::Material;
use crate::mesh::{build_unit_cylinder, CylinderVertex};
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

/// Owns the GPU resources for the 3D viewport (pipelines, buffers). Lives
/// in egui-wgpu's `CallbackResources` so it shares the device/queue eframe
/// already created, rather than opening a second one.
pub struct ViewportResources {
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    atom_pipeline: wgpu::RenderPipeline,
    cylinder_pipeline: wgpu::RenderPipeline,
    atom_highlight_pipeline: wgpu::RenderPipeline,
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

    atom_instances: Option<(wgpu::Buffer, u32)>,
    bond_instances: Option<(wgpu::Buffer, u32)>,
    atom_highlight_instances: Option<(wgpu::Buffer, u32)>,
    bond_highlight_instances: Option<(wgpu::Buffer, u32)>,
    measurement_instances: Option<(wgpu::Buffer, u32)>,
}

impl ViewportResources {
    /// `glyph_atlas` is constructed and owned by the caller (it's also
    /// needed CPU-side, for laying out label text, independent of this
    /// GPU-resources struct) — this just builds a bind group referencing
    /// its texture.
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, glyph_atlas: &GlyphAtlas) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene_uniforms"),
            size: std::mem::size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let opaque_depth = Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        // Highlight overlays reuse the exact geometry (or a very slightly
        // enlarged one for atoms) of what's already drawn, so an exact
        // `Less` comparison risks failing on floating-point ties — hence
        // `LessEqual` and no depth write, so the overlay reliably shows up
        // without disturbing depth for anything drawn after it.
        let highlight_depth = Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let multisample = wgpu::MultisampleState { count: MSAA_SAMPLES, ..Default::default() };

        let sphere_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sphere_impostor_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sphere.wgsl").into()),
        });
        let cylinder_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cylinder_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/cylinder.wgsl").into()),
        });

        let atom_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<AtomInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &atom_instance_attributes(),
        };
        let bond_attrs = bond_instance_attributes();
        let bond_color_attr = bond_color_attribute();
        let bond_instance_attrs =
            [bond_attrs[0], bond_attrs[1], bond_attrs[2], bond_attrs[3], bond_color_attr];
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

        let make_atom_pipeline = |label: &str, entry_point: &'static str, blend, depth_stencil: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &sphere_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(atom_instance_layout.clone())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &sphere_shader,
                    entry_point: Some(entry_point),
                    targets: &[Some(wgpu::ColorTargetState { format: target_format, blend, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil,
                multisample,
                multiview_mask: None,
                cache: None,
            })
        };

        let atom_pipeline = make_atom_pipeline("atom_pipeline", "fs_main", Some(wgpu::BlendState::REPLACE), opaque_depth.clone());
        let atom_highlight_pipeline = make_atom_pipeline(
            "atom_highlight_pipeline",
            "fs_highlight",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            highlight_depth.clone(),
        );

        let make_cylinder_pipeline = |label: &str, entry_point: &'static str, blend, depth_stencil: Option<wgpu::DepthStencilState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &cylinder_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(cylinder_vertex_layout.clone()), Some(bond_instance_layout.clone())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &cylinder_shader,
                    entry_point: Some(entry_point),
                    targets: &[Some(wgpu::ColorTargetState { format: target_format, blend, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil,
                multisample,
                multiview_mask: None,
                cache: None,
            })
        };

        let cylinder_pipeline =
            make_cylinder_pipeline("cylinder_pipeline", "fs_main", Some(wgpu::BlendState::REPLACE), opaque_depth.clone());
        let cylinder_highlight_pipeline = make_cylinder_pipeline(
            "cylinder_highlight_pipeline",
            "fs_highlight",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            highlight_depth,
        );

        let (cylinder_vertices, cylinder_indices) = build_unit_cylinder(16);
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
        let glyph_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let glyph_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph_bind_group"),
            layout: &glyph_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&glyph_atlas.texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&glyph_atlas.sampler) },
            ],
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&glyph_bind_group_layout)],
            immediate_size: 0,
        });
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
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
            ],
        };
        // Alpha-to-coverage, not blending: each MSAA sample is dithered
        // in/out based on the glyph's coverage, giving antialiased edges
        // while still writing real per-sample depth.
        let text_multisample = wgpu::MultisampleState { count: MSAA_SAMPLES, alpha_to_coverage_enabled: true, ..Default::default() };
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text_pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(glyph_instance_layout)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: opaque_depth,
            multisample: text_multisample,
            multiview_mask: None,
            cache: None,
        });

        Self {
            uniform_buffer,
            bind_group,
            atom_pipeline,
            cylinder_pipeline,
            atom_highlight_pipeline,
            cylinder_highlight_pipeline,
            cylinder_vertex_buffer,
            cylinder_index_buffer,
            cylinder_index_count: cylinder_indices.len() as u32,
            glyph_bind_group,
            text_pipeline,
            text_instances: None,
            atom_instances: None,
            bond_instances: None,
            atom_highlight_instances: None,
            bond_highlight_instances: None,
            measurement_instances: None,
        }
    }

    /// Initial upload when a molecule is opened — everything visible, no
    /// selection, all bonds solid, no measurements.
    pub fn load_molecule(&mut self, device: &wgpu::Device, molecule: &Molecule) {
        self.update_geometry(device, molecule, &HashSet::new(), &HashSet::new(), &[]);
        self.update_highlights(device, molecule, &[], &[]);
        self.update_measurements(device, molecule, &[], [0.0, 0.0, 0.0]);
    }

    /// Rebuilds the atom/bond instance buffers from current hide/style
    /// state. A rare, user-triggered event (a button click), not a
    /// per-frame cost — cheap to just rebuild outright rather than track
    /// incremental diffs, even for much larger molecules than this project
    /// currently targets.
    pub fn update_geometry(
        &mut self,
        device: &wgpu::Device,
        molecule: &Molecule,
        hidden_atoms: &HashSet<usize>,
        hidden_bonds: &HashSet<usize>,
        bond_styles: &[BondVisualStyle],
    ) {
        let atom_data = build_atom_instances(molecule, hidden_atoms);
        let bond_data = build_bond_instances(molecule, hidden_atoms, hidden_bonds, bond_styles);

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
    ) {
        let atom_data = build_atom_highlight_instances(molecule, selected_atoms);
        let bond_data = build_bond_highlight_instances(molecule, selected_bonds);

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

    /// Rebuilds the Analysis-panel measurement-line buffer. `segments` are
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
    pub fn update_labels(&mut self, device: &wgpu::Device, instances: &[GlyphInstance]) {
        if instances.is_empty() {
            self.text_instances = None;
            return;
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text_instance_buffer"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.text_instances = Some((buffer, instances.len() as u32));
    }

    fn write_uniforms(&self, queue: &wgpu::Queue, uniforms: &SceneUniforms) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
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
        let uniforms = SceneUniforms::new(&self.camera, self.aspect_ratio, &self.material);
        resources.write_uniforms(queue, &uniforms);
        resources.update_labels(device, &self.label_instances);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let resources: &ViewportResources = callback_resources.get().expect("ViewportResources not registered");

        render_pass.set_bind_group(0, &resources.bind_group, &[]);

        if let Some((buffer, count)) = &resources.bond_instances {
            render_pass.set_pipeline(&resources.cylinder_pipeline);
            render_pass.set_vertex_buffer(0, resources.cylinder_vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, buffer.slice(..));
            render_pass.set_index_buffer(resources.cylinder_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..resources.cylinder_index_count, 0, 0..*count);
        }

        if let Some((buffer, count)) = &resources.atom_instances {
            render_pass.set_pipeline(&resources.atom_pipeline);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..6, 0..*count);
        }

        // Analysis-panel measurement lines — opaque and depth-tested like
        // real bonds (via the same pipeline, reusing its dashed-thin-line
        // path), so they correctly interleave with the molecule.
        if let Some((buffer, count)) = &resources.measurement_instances {
            if *count > 0 {
                render_pass.set_pipeline(&resources.cylinder_pipeline);
                render_pass.set_vertex_buffer(0, resources.cylinder_vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, buffer.slice(..));
                render_pass.set_index_buffer(resources.cylinder_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..resources.cylinder_index_count, 0, 0..*count);
            }
        }

        // 3D labels (atom tags, measurement values) — real depth-tested
        // billboard glyph quads, drawn opaque-ish (alpha-to-coverage, see
        // the pipeline) so they're correctly, precisely occluded by
        // whatever's actually in front of them.
        if let Some((buffer, count)) = &resources.text_instances {
            if *count > 0 {
                render_pass.set_pipeline(&resources.text_pipeline);
                render_pass.set_bind_group(1, &resources.glyph_bind_group, &[]);
                render_pass.set_vertex_buffer(0, buffer.slice(..));
                render_pass.draw(0..6, 0..*count);
            }
        }

        // Highlight overlays last, alpha-blended on top of the opaque pass.
        if let Some((buffer, count)) = &resources.bond_highlight_instances {
            if *count > 0 {
                render_pass.set_pipeline(&resources.cylinder_highlight_pipeline);
                render_pass.set_vertex_buffer(0, resources.cylinder_vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, buffer.slice(..));
                render_pass.set_index_buffer(resources.cylinder_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..resources.cylinder_index_count, 0, 0..*count);
            }
        }

        if let Some((buffer, count)) = &resources.atom_highlight_instances {
            if *count > 0 {
                render_pass.set_pipeline(&resources.atom_highlight_pipeline);
                render_pass.set_vertex_buffer(0, buffer.slice(..));
                render_pass.draw(0..6, 0..*count);
            }
        }
    }
}
