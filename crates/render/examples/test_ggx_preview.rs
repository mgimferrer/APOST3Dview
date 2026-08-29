//! One-off visual comparison, deliberately NOT wired into the app or any
//! production shader file: current atom/bond shading (Blinn-Phong +
//! Fresnel rim + hemisphere fill + filmic tone mapping, exactly what
//! ships today, rendered through the real `ViewportResources` pipeline)
//! side by side with a from-scratch Cook-Torrance/GGX implementation
//! (same tone mapping and hemisphere fill, so the only real difference is
//! the core BRDF), on the same real molecule and camera. For deciding
//! whether the GGX rewrite is worth doing for real before touching any
//! shipped shader. Not a kept regression test — delete once the decision
//! is made either way.
//!
//! AO and DoF are both off for this comparison on purpose: this is about
//! the base material/lighting model only, not the whole pipeline.

use apost3dview_core::Molecule;
use apost3dview_render::instances::{build_atom_instances, build_bond_instances, AtomInstance, BondInstance};
use apost3dview_render::mesh::{build_unit_cylinder, CylinderVertex};
use apost3dview_render::{ExportSettings, GlyphAtlas, Material, OrbitCamera, SceneUniforms, ViewportResources};
use std::collections::HashSet;
use std::path::PathBuf;
use wgpu::util::DeviceExt;

fn main() {
    pollster::block_on(run());
}

const GGX_SHADER: &str = r#"
struct SceneUniforms {
    view_proj: mat4x4<f32>,
    camera_eye: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
    light_dir: vec4<f32>,
    material: vec4<f32>,
    style: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene: SceneUniforms;

const PI: f32 = 3.14159265359;

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
fn aces_tonemap(c: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let cc = 2.43; let d = 0.59; let e = 0.14;
    return clamp((c * (a * c + vec3<f32>(b))) / (c * (cc * c + vec3<f32>(d)) + vec3<f32>(e)), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn hemisphere_ambient(normal: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(1.05, 1.05, 1.1);
    let ground = vec3<f32>(0.65, 0.62, 0.6);
    return mix(ground, sky, normal.y * 0.5 + 0.5);
}
fn finalize_color(linear_color: vec3<f32>) -> vec3<f32> {
    return linear_to_srgb(aces_tonemap(linear_color * scene.style.z));
}

// Standard Cook-Torrance/GGX, Karis/Epic's direct-lighting k remap for the
// Smith geometry term — the same formulation Blender's Principled BSDF /
// Unreal / most modern real-time renderers use.
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denom * denom, 1e-6);
}
fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / max(n_dot_v * (1.0 - k) + k, 1e-6);
}
fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}
fn fresnel_schlick_vec(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// Roughness/F0 fixed here (not exposed) — this is a one-off preview, not
// a real material system. 0.42 roughness / 0.04 F0 (dielectric, non-metal
// — right for CPK balls, not chrome) roughly matches the doc's suggested
// atom roughness range (0.35-0.5).
const ROUGHNESS: f32 = 0.42;
const F0_DIELECTRIC: vec3<f32> = vec3<f32>(0.04, 0.04, 0.04);
// Direct-light intensity fudge — GGX's energy-conserving specular and
// Blinn-Phong's ad hoc one aren't calibrated to the same scale, so this
// was tuned by eye against the current render's overall brightness rather
// than derived.
const LIGHT_INTENSITY: f32 = 3.2;
const AMBIENT_LEVEL: f32 = 0.32;

fn shade_ggx(hit_point: vec3<f32>, normal: vec3<f32>, albedo_srgb: vec3<f32>) -> vec3<f32> {
    let n = normal;
    let v = normalize(scene.camera_eye.xyz - hit_point);
    let l = normalize(scene.light_dir.xyz);
    let h = normalize(v + l);

    let n_dot_v = max(dot(n, v), 1e-4);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let albedo = srgb_to_linear(albedo_srgb);

    let d = distribution_ggx(n_dot_h, ROUGHNESS);
    let g = geometry_smith(n_dot_v, n_dot_l, ROUGHNESS);
    let f = fresnel_schlick_vec(v_dot_h, F0_DIELECTRIC);

    let specular = (d * g * f) / max(4.0 * n_dot_v * n_dot_l, 1e-4);
    let kd = vec3<f32>(1.0) - f;
    let diffuse = kd * albedo / PI;

    let direct = (diffuse + specular) * n_dot_l * LIGHT_INTENSITY;
    let ambient = hemisphere_ambient(n) * albedo * AMBIENT_LEVEL;
    return direct + ambient;
}

struct AtomInstanceIn {
    @location(0) center: vec3<f32>,
    @location(1) vdw_radius: f32,
    @location(2) color: vec3<f32>,
};
struct AtomVertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) center: vec3<f32>,
    @location(2) radius: f32,
    @location(3) color: vec3<f32>,
};
const CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vs_atom(@builtin(vertex_index) vertex_index: u32, instance: AtomInstanceIn) -> AtomVertexOut {
    let radius = instance.vdw_radius * scene.style.x;
    let to_eye = scene.camera_eye.xyz - instance.center;
    let distance_to_eye = max(length(to_eye), 0.0001);
    let forward_to_eye = to_eye / distance_to_eye;
    var right = cross(forward_to_eye, scene.camera_up.xyz);
    let right_len = length(right);
    if (right_len < 0.0001) {
        right = scene.camera_right.xyz;
    } else {
        right = right / right_len;
    }
    let up = cross(right, forward_to_eye);
    let ratio = clamp(radius / distance_to_eye, 0.0, 0.999);
    let quad_radius = radius / sqrt(1.0 - ratio * ratio);
    let corner = CORNERS[vertex_index];
    let world_position = instance.center + right * corner.x * quad_radius + up * corner.y * quad_radius;

    var out: AtomVertexOut;
    out.clip_position = scene.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.center = instance.center;
    out.radius = radius;
    out.color = instance.color;
    return out;
}

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

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_atom_ggx(in: AtomVertexOut) -> FragmentOutput {
    let ray_origin = scene.camera_eye.xyz;
    let ray_dir = normalize(in.world_position - ray_origin);
    let hit = intersect_sphere(ray_origin, ray_dir, in.center, in.radius);
    if (hit.w < 0.5) {
        discard;
    }
    let hit_point = hit.xyz;
    let normal = normalize(hit_point - in.center);
    let lit = shade_ggx(hit_point, normal, in.color);
    let clip = scene.view_proj * vec4<f32>(hit_point, 1.0);
    var out: FragmentOutput;
    out.color = vec4<f32>(finalize_color(lit), 1.0);
    out.depth = clip.z / clip.w;
    return out;
}

struct CylinderVertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
struct BondInstanceIn {
    @location(2) center: vec3<f32>,
    @location(3) length: f32,
    @location(4) axis: vec3<f32>,
    @location(5) dashed: f32,
    @location(6) color: vec3<f32>,
};
struct CylinderVertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn vs_cylinder(in: CylinderVertexIn, instance: BondInstanceIn) -> CylinderVertexOut {
    let axis = normalize(instance.axis);
    var reference = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(dot(axis, reference)) > 0.99) {
        reference = vec3<f32>(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(reference, axis));
    let bitangent = cross(axis, tangent);
    let radius = scene.style.y;
    let world_position = instance.center
        + tangent * in.position.x * radius
        + axis * in.position.y * instance.length
        + bitangent * in.position.z * radius;
    let world_normal = normalize(tangent * in.normal.x + bitangent * in.normal.z);

    var out: CylinderVertexOut;
    out.clip_position = scene.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.world_normal = world_normal;
    out.color = instance.color;
    return out;
}

@fragment
fn fs_cylinder_ggx(in: CylinderVertexOut) -> @location(0) vec4<f32> {
    let normal = normalize(in.world_normal);
    let lit = shade_ggx(in.world_position, normal, in.color);
    return vec4<f32>(finalize_color(lit), 1.0);
}
"#;

async fn run() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("TESTS-VISUALIZER/Bi-dianion-OSD.fchk"));
    let molecule = Molecule::from_fchk(&path).expect("failed to parse real fchk geometry");
    println!("Parsed {}: {} atoms, {} bonds", path.display(), molecule.atomic_numbers.len(), molecule.bonds.len());

    let instance = wgpu::Instance::default();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("no adapter");
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("no device");
    let target_format = wgpu::TextureFormat::Bgra8Unorm;

    let (center, radius) = molecule.bounding_sphere();
    let mut camera = OrbitCamera::default();
    camera.frame_bounds(center, radius);
    let material = Material::default();
    let mut uniforms = SceneUniforms::new(&camera, 1.0, &material);
    uniforms.set_srgb_target(false);

    let out_dir = std::env::var("GGX_TEST_OUT_DIR").unwrap_or_else(|_| ".".to_string());
    let (width, height) = (800u32, 800u32);

    // --- "Current": the real, shipped pipeline, AO/DoF off ---
    let glyph_atlas = GlyphAtlas::new(&device, &queue);
    let mut resources = ViewportResources::new(&device, target_format, &glyph_atlas);
    resources.load_molecule(&device, &molecule);
    let settings = ExportSettings {
        width,
        height,
        supersample: 2,
        background: Some([1.0, 1.0, 1.0, 1.0]),
        ambient_occlusion: None,
        depth_of_field: None,
        dof_focus_distance: 0.0,
    };
    let current_pixels = resources
        .render_offscreen(&device, &queue, target_format, &uniforms, &[], &settings)
        .expect("current-pipeline render should succeed");
    save_png(&current_pixels, width, height, &out_dir, "ggx_preview_current");

    // --- "GGX": hand-rolled standalone pipeline, same instance data ---
    let atom_instances = build_atom_instances(&molecule, &HashSet::new());
    let bond_instances = build_bond_instances(&molecule, &HashSet::new(), &HashSet::new(), &[]);
    let (cyl_vertices, cyl_indices) = build_unit_cylinder(48);

    let atom_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ggx_atom_instances"),
        contents: bytemuck::cast_slice(&atom_instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let bond_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ggx_bond_instances"),
        contents: bytemuck::cast_slice(&bond_instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let cyl_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ggx_cyl_vertices"),
        contents: bytemuck::cast_slice(&cyl_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let cyl_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ggx_cyl_indices"),
        contents: bytemuck::cast_slice(&cyl_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ggx_scene_uniforms"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ggx_bind_group_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ggx_bind_group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ggx_pipeline_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("ggx_shader"), source: wgpu::ShaderSource::Wgsl(GGX_SHADER.into()) });

    let depth_stencil = Some(wgpu::DepthStencilState {
        format: apost3dview_render::DEPTH_FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });
    let multisample = wgpu::MultisampleState { count: apost3dview_render::MSAA_SAMPLES, ..Default::default() };

    let atom_instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<AtomInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
        ],
    };
    let atom_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ggx_atom_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_atom"), buffers: &[Some(atom_instance_layout)], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_atom_ggx"),
            targets: &[Some(wgpu::ColorTargetState { format: target_format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: depth_stencil.clone(),
        multisample,
        multiview_mask: None,
        cache: None,
    });

    let cyl_vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<CylinderVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
        ],
    };
    let bond_instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<BondInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 3, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { offset: 16, shader_location: 4, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 28, shader_location: 5, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { offset: 32, shader_location: 6, format: wgpu::VertexFormat::Float32x3 },
        ],
    };
    let cylinder_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ggx_cylinder_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_cylinder"),
            buffers: &[Some(cyl_vertex_layout), Some(bond_instance_layout)],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_cylinder_ggx"),
            targets: &[Some(wgpu::ColorTargetState { format: target_format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil,
        multisample,
        multiview_mask: None,
        cache: None,
    });

    let render_width = width * 2;
    let render_height = height * 2;
    let size = wgpu::Extent3d { width: render_width, height: render_height, depth_or_array_layers: 1 };
    let msaa_color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ggx_msaa_color"),
        size,
        mip_level_count: 1,
        sample_count: apost3dview_render::MSAA_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let msaa_view = msaa_color.create_view(&wgpu::TextureViewDescriptor::default());
    let resolve_color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ggx_resolve_color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let resolve_view = resolve_color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("ggx_depth"),
            size,
            mip_level_count: 1,
            sample_count: apost3dview_render::MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: apost3dview_render::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ggx_encoder") });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ggx_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                resolve_target: Some(&resolve_view),
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_pipeline(&cylinder_pipeline);
        pass.set_vertex_buffer(0, cyl_vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, bond_buffer.slice(..));
        pass.set_index_buffer(cyl_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..cyl_indices.len() as u32, 0, 0..bond_instances.len() as u32);

        pass.set_pipeline(&atom_pipeline);
        pass.set_vertex_buffer(0, atom_buffer.slice(..));
        pass.draw(0..6, 0..atom_instances.len() as u32);
    }
    let bytes_per_row = (render_width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ggx_readback"),
        size: (bytes_per_row * render_height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &resolve_color, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row), rows_per_image: Some(render_height) } },
        size,
    );
    queue.submit(Some(encoder.finish()));
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).expect("poll failed");
    rx.recv().expect("readback channel closed").expect("failed to map readback buffer");
    let padded = slice.get_mapped_range().expect("failed to get mapped range");
    let mut rendered = vec![0u8; (render_width * 4 * render_height) as usize];
    for row in 0..render_height as usize {
        let src = row * bytes_per_row as usize;
        let dst = row * (render_width * 4) as usize;
        rendered[dst..dst + (render_width * 4) as usize].copy_from_slice(&padded[src..src + (render_width * 4) as usize]);
    }
    drop(padded);
    readback.unmap();
    let (downsampled, _, _) = apost3dview_render::downsample_rgba(&rendered, render_width, render_height, 2);
    save_png(&downsampled, width, height, &out_dir, "ggx_preview_ggx");

    println!("Saved {out_dir}/ggx_preview_current.png and {out_dir}/ggx_preview_ggx.png");
}

fn save_png(pixels: &[u8], width: u32, height: u32, out_dir: &str, name: &str) {
    let mut rgba = pixels.to_vec();
    for px in rgba.chunks_mut(4) {
        px.swap(0, 2);
    }
    let out_path = format!("{out_dir}/{name}.png");
    image::save_buffer(&out_path, &rgba, width, height, image::ColorType::Rgba8).expect("failed to save PNG");
}
