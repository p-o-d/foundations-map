//! Billboard sprite pipeline state + per-instance data.
//!
//! GPU pipeline construction added in T6. This file currently exposes the
//! per-instance struct and the `from_target` helper.

use crate::renderer::atlas::{AtlasLookup, IconId};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SpriteInstance {
    pub world_pos: [f32; 3],
    pub _pad0: f32,
    pub atlas_uv_min: [f32; 2],
    pub atlas_uv_size: [f32; 2],
    pub faction_color: [f32; 4],
    pub scale_px: f32,
    pub ring_thickness_px: f32,
    pub _pad1: [f32; 2],
}

impl SpriteInstance {
    pub const SCALE_NORMAL: f32 = 32.0;
    pub const SCALE_SELECTED: f32 = 42.0;
    pub const RING_NORMAL: f32 = 2.0;
    pub const RING_SELECTED: f32 = 4.0;
    pub const SELECTION_COLOR: [f32; 4] = [1.0, 0.85, 0.1, 1.0];

    pub fn from_target(
        world_pos: Vec3,
        icon: IconId,
        ring_color: [f32; 4],
        selected: bool,
        atlas: &AtlasLookup,
    ) -> Self {
        let uv = atlas.uv_of(icon);
        let (scale_px, ring_thickness_px, color) = if selected {
            (Self::SCALE_SELECTED, Self::RING_SELECTED, Self::SELECTION_COLOR)
        } else {
            (Self::SCALE_NORMAL, Self::RING_NORMAL, ring_color)
        };
        Self {
            world_pos: [world_pos.x, world_pos.y, world_pos.z],
            _pad0: 0.0,
            atlas_uv_min: [uv[0], uv[1]],
            atlas_uv_size: [uv[2], uv[3]],
            faction_color: color,
            scale_px,
            ring_thickness_px,
            _pad1: [0.0; 2],
        }
    }
}

use eframe::egui_wgpu::wgpu;
use std::num::NonZeroU64;

pub const SPRITE_SHADER_SRC: &str = r#"
struct VIn {
    @location(0) corner: vec2<f32>,
};
struct IIn {
    @location(1) world_pos: vec3<f32>,
    @location(2) atlas_uv_min: vec2<f32>,
    @location(3) atlas_uv_size: vec2<f32>,
    @location(4) faction_color: vec4<f32>,
    @location(5) scale_px: f32,
    @location(6) ring_thickness_px: f32,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) faction_color: vec4<f32>,
    @location(3) ring_edge: f32,
};

struct Camera {
    view_proj: mat4x4<f32>,
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex fn vs(v: VIn, i: IIn) -> VOut {
    var center_clip = cam.view_proj * vec4<f32>(i.world_pos, 1.0);
    let pixel_offset = v.corner * i.scale_px * 2.0 / cam.viewport;
    center_clip.x = center_clip.x + pixel_offset.x * center_clip.w;
    center_clip.y = center_clip.y + pixel_offset.y * center_clip.w;
    var o: VOut;
    o.clip = center_clip;
    o.corner = v.corner;
    o.uv = i.atlas_uv_min + (v.corner + vec2<f32>(0.5)) * i.atlas_uv_size;
    o.faction_color = i.faction_color;
    // d_norm 0..1 maps to radius 0..(scale_px/2) px, so 1 unit of d_norm = scale_px/2 px.
    o.ring_edge = 1.0 - (i.ring_thickness_px * 2.0 / i.scale_px);
    return o;
}

@fragment fn fs(o: VOut) -> @location(0) vec4<f32> {
    let d_norm = length(o.corner) * 2.0;
    if d_norm > 1.0 { discard; }
    if d_norm > o.ring_edge {
        return o.faction_color;
    }
    let glyph_alpha = textureSample(atlas, samp, o.uv).r;
    return vec4<f32>(1.0, 1.0, 1.0, glyph_alpha);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub viewport: [f32; 2],
    pub _pad: [f32; 2],
}

pub struct SpritePipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub camera_buf: wgpu::Buffer,
    pub quad_vb: wgpu::Buffer,
    pub quad_ib: wgpu::Buffer,
    pub instance_vb: wgpu::Buffer,
    pub instance_capacity: usize,
}

impl SpritePipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        atlas_bytes: &[u8],
        atlas_w: u32,
        atlas_h: u32,
    ) -> Self {
        // Atlas texture.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("icon_atlas"),
            size: wgpu::Extent3d { width: atlas_w, height: atlas_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d { width: atlas_w, height: atlas_h, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("icon_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<CameraUniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite_shader"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_SHADER_SRC.into()),
        });

        let vertex_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<[f32; 2]>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2],
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    1 => Float32x3,
                    2 => Float32x2,
                    3 => Float32x2,
                    4 => Float32x4,
                    5 => Float32,
                    6 => Float32,
                ],
            },
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite_pipeline"),
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Shared quad: 4 verts, 6 indices.
        let quad_verts: [[f32; 2]; 4] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
        let quad_idx: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let quad_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_quad_vb"),
            size: (quad_verts.len() * std::mem::size_of::<[f32; 2]>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&quad_vb, 0, bytemuck::cast_slice(&quad_verts));
        let quad_ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_quad_ib"),
            size: (quad_idx.len() * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&quad_ib, 0, bytemuck::cast_slice(&quad_idx));

        let instance_capacity = 1024usize;
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_instance_vb"),
            size: (instance_capacity * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group,
            camera_buf,
            quad_vb,
            quad_ib,
            instance_vb,
            instance_capacity,
        }
    }

    pub fn update_camera(&self, queue: &wgpu::Queue, view_proj: glam::Mat4, viewport: [f32; 2]) {
        let u = CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            viewport,
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&u));
    }

    pub fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[SpriteInstance],
    ) {
        if instances.len() > self.instance_capacity {
            let new_cap = instances.len().max(self.instance_capacity * 2);
            self.instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprite_instance_vb_grown"),
                size: (new_cap * std::mem::size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = new_cap;
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(instances));
        }
    }

    pub fn draw(&self, rpass: &mut wgpu::RenderPass<'_>, n_instances: u32) {
        if n_instances == 0 { return; }
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.quad_vb.slice(..));
        rpass.set_vertex_buffer(
            1,
            self.instance_vb.slice(
                ..(n_instances as u64 * std::mem::size_of::<SpriteInstance>() as u64),
            ),
        );
        rpass.set_index_buffer(self.quad_ib.slice(..), wgpu::IndexFormat::Uint16);
        rpass.draw_indexed(0..6, 0, 0..n_instances);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn sample_atlas() -> AtlasLookup {
        AtlasLookup::build()
    }

    #[test]
    fn from_target_normal_uses_faction_ring() {
        let atlas = sample_atlas();
        let inst = SpriteInstance::from_target(
            Vec3::new(10.0, 20.0, 30.0),
            IconId::Factory,
            [0.2, 0.5, 1.0, 1.0],
            false,
            &atlas,
        );
        assert_eq!(inst.world_pos, [10.0, 20.0, 30.0]);
        assert_eq!(inst.scale_px, SpriteInstance::SCALE_NORMAL);
        assert_eq!(inst.ring_thickness_px, SpriteInstance::RING_NORMAL);
        assert_eq!(inst.faction_color, [0.2, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn from_target_selected_uses_yellow_ring_and_larger_scale() {
        let atlas = sample_atlas();
        let inst = SpriteInstance::from_target(
            Vec3::ZERO,
            IconId::Capital,
            [0.2, 0.5, 1.0, 1.0],
            true,
            &atlas,
        );
        assert_eq!(inst.scale_px, SpriteInstance::SCALE_SELECTED);
        assert_eq!(inst.ring_thickness_px, SpriteInstance::RING_SELECTED);
        assert_eq!(inst.faction_color, SpriteInstance::SELECTION_COLOR);
    }

    #[test]
    fn from_target_writes_correct_uv_rect() {
        let atlas = sample_atlas();
        let expected = atlas.uv_of(IconId::Anomaly);
        let inst = SpriteInstance::from_target(
            Vec3::ZERO,
            IconId::Anomaly,
            [1.0, 1.0, 1.0, 1.0],
            false,
            &atlas,
        );
        assert_eq!(inst.atlas_uv_min, [expected[0], expected[1]]);
        assert_eq!(inst.atlas_uv_size, [expected[2], expected[3]]);
    }
}
