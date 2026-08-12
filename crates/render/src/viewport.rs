use wgpu::util::DeviceExt;

use apost3dview_core::Molecule;

use crate::camera::OrbitCamera;
use crate::consts::{DEPTH_FORMAT, MSAA_SAMPLES};
use crate::instances::{build_atom_instances, build_bond_instances, AtomInstance, BondInstance};
use crate::material::Material;
use crate::mesh::{build_unit_cylinder, CylinderVertex};
use crate::uniforms::SceneUniforms;

/// Owns the GPU resources for the 3D viewport (pipelines, buffers). Lives
/// in egui-wgpu's `CallbackResources` so it shares the device/queue eframe
/// already created, rather than opening a second one.
pub struct ViewportResources {
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,

    atom_pipeline: wgpu::RenderPipeline,
    cylinder_pipeline: wgpu::RenderPipeline,

    cylinder_vertex_buffer: wgpu::Buffer,
    cylinder_index_buffer: wgpu::Buffer,
    cylinder_index_count: u32,

    atom_instances: Option<(wgpu::Buffer, u32)>,
    bond_instances: Option<(wgpu::Buffer, u32)>,
}

impl ViewportResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
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

        let depth_stencil = Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let multisample = wgpu::MultisampleState { count: MSAA_SAMPLES, ..Default::default() };

        let atom_pipeline = {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sphere_impostor_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sphere.wgsl").into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("atom_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<AtomInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32 },
                            wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
                        ],
                    })],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: depth_stencil.clone(),
                multisample,
                multiview_mask: None,
                cache: None,
            })
        };

        let cylinder_pipeline = {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cylinder_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/cylinder.wgsl").into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("cylinder_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<CylinderVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[
                                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                                wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
                            ],
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<BondInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &[
                                wgpu::VertexAttribute { offset: 0, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
                                wgpu::VertexAttribute { offset: 12, shader_location: 3, format: wgpu::VertexFormat::Float32 },
                                wgpu::VertexAttribute { offset: 16, shader_location: 4, format: wgpu::VertexFormat::Float32x3 },
                                wgpu::VertexAttribute { offset: 32, shader_location: 5, format: wgpu::VertexFormat::Float32x3 },
                            ],
                        }),
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil,
                multisample,
                multiview_mask: None,
                cache: None,
            })
        };

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

        Self {
            uniform_buffer,
            bind_group,
            atom_pipeline,
            cylinder_pipeline,
            cylinder_vertex_buffer,
            cylinder_index_buffer,
            cylinder_index_count: cylinder_indices.len() as u32,
            atom_instances: None,
            bond_instances: None,
        }
    }

    pub fn load_molecule(&mut self, device: &wgpu::Device, molecule: &Molecule) {
        let atom_data = build_atom_instances(molecule);
        let bond_data = build_bond_instances(molecule);

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
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &ViewportResources = callback_resources.get().expect("ViewportResources not registered");
        let uniforms = SceneUniforms::new(&self.camera, self.aspect_ratio, &self.material);
        resources.write_uniforms(queue, &uniforms);
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
    }
}
