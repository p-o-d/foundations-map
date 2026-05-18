use super::mesh::{Mesh, Vertex};
use eframe::egui_wgpu;
use eframe::egui_wgpu::wgpu;
use glam::Mat4;
use std::collections::HashMap;

const SHADER_SRC: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VIn  { @location(0) pos: vec3<f32>, @location(1) col: vec4<f32> }
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) col: vec4<f32> }

@vertex  fn vs(v: VIn) -> VOut {
    return VOut(uniforms.mvp * vec4<f32>(v.pos, 1.0), v.col * uniforms.color);
}
@fragment fn fs(v: VOut) -> @location(0) vec4<f32> { return v.col; }
"#;

const UNIFORM_STRIDE: u64 = 256;
// 2048 × 256-byte stride = 512 KB uniform buffer. WebGPU baseline guarantees
// 64 KB; modern desktops give 1 MB+. If we ship to lower-spec hardware, chunk
// the draw passes instead of one giant buffer.
const MAX_OBJECTS: u64 = 2048;

pub struct GpuMesh {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub index_count: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MeshKind {
    Box,
    Ring,
    Sphere,
}

pub struct GpuScene {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buf: wgpu::Buffer,
    pub meshes: HashMap<MeshKind, GpuMesh>,
}

impl GpuScene {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3d_scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3d_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(80), // 64 (mat4) + 16 (color)
                },
                count: None,
            }],
        });

        // wgpu 29: bind_group_layouts takes &[Option<&BindGroupLayout>], no push_constant_ranges
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3d_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
        }];

        // wgpu 29: multiview is multiview_mask: Option<NonZeroU32>
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3d_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3d_uniforms"),
            size: UNIFORM_STRIDE * MAX_OBJECTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3d_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buf,
                    offset: 0,
                    size: std::num::NonZeroU64::new(80),
                }),
            }],
        });

        use super::mesh::{box_mesh, ring_mesh, sphere_mesh};
        let mut meshes = HashMap::new();
        let white = [1.0f32; 4];
        for (kind, mesh) in [
            (MeshKind::Box, box_mesh([1.0, 1.0, 1.0], white)),
            (MeshKind::Ring, ring_mesh(0.6, 1.0, 32, white)),
            (MeshKind::Sphere, sphere_mesh(1.0, 12, 12, white)),
        ] {
            meshes.insert(kind, upload_mesh(device, &mesh));
        }

        Self {
            pipeline,
            bind_group,
            uniform_buf,
            meshes,
        }
    }
}

fn upload_mesh(device: &wgpu::Device, mesh: &Mesh) -> GpuMesh {
    use wgpu::util::DeviceExt;
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh_vb"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh_ib"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        vertex_buf,
        index_buf,
        index_count: mesh.indices.len() as u32,
    }
}

pub struct DrawCall {
    pub kind: MeshKind,
    pub mvp: Mat4,
    pub color: [f32; 4],
}

pub struct SceneCallback {
    pub draw_calls: Vec<DrawCall>,
}

// PaintCallbackInfo is in egui (re-exported from epaint), not egui_wgpu
impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(scene) = callback_resources.get_mut::<GpuScene>() else {
            return vec![];
        };
        if self.draw_calls.len() > MAX_OBJECTS as usize {
            eprintln!(
                "[render] WARNING: scene has {} draw calls but GPU cap is {}; truncating",
                self.draw_calls.len(), MAX_OBJECTS
            );
        }
        let n = self.draw_calls.len().min(MAX_OBJECTS as usize);
        if n > 0 {
            let mut buf = vec![0u8; n * UNIFORM_STRIDE as usize];
            for (i, dc) in self.draw_calls[..n].iter().enumerate() {
                let offset = i * UNIFORM_STRIDE as usize;
                let mvp: [[f32; 4]; 4] = dc.mvp.to_cols_array_2d();
                let mvp_bytes: &[u8; 64] = bytemuck::cast_ref(&mvp);
                buf[offset..offset + 64].copy_from_slice(mvp_bytes);
                let col_bytes: &[u8; 16] = bytemuck::cast_ref(&dc.color);
                buf[offset + 64..offset + 80].copy_from_slice(col_bytes);
            }
            queue.write_buffer(&scene.uniform_buf, 0, &buf);
        }

        vec![]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(scene) = callback_resources.get::<GpuScene>() else {
            return;
        };
        render_pass.set_pipeline(&scene.pipeline);
        let mut draw_idx: usize = 0;
        for dc in self.draw_calls.iter().take(MAX_OBJECTS as usize) {
            let Some(gpu_mesh) = scene.meshes.get(&dc.kind) else {
                draw_idx += 1;
                continue;
            };
            let offset = (draw_idx as u64 * UNIFORM_STRIDE) as u32;
            // wgpu 29: set_bind_group takes Option<&BindGroup>
            render_pass.set_bind_group(0, Some(&scene.bind_group), &[offset]);
            render_pass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            render_pass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..1);
            draw_idx += 1;
        }
    }
}
