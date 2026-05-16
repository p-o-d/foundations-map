# Phase 2: 3D Sector View — Implementation Plan

> **Status:** ✅ Completed + bonus scope (stations, superhighways, DLC, animated dashes, scrollable panel). Historical reference. See `docs/superpowers/retrospectives/2026-05-15-phase2-retro.md` for actual outcomes and `docs/superpowers/specs/2026-05-14-x4-map-design.md` for current design.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a 3D sector view that renders static gate objects inside an orbit-camera wgpu scene embedded in egui, with object selection and keyboard/mouse controls.

**Architecture:** A `GpuScene` is built once at startup and stored in egui_wgpu's `CallbackResources`; each frame a `SceneCallback` is pushed as an `egui::PaintCallback` carrying the current camera and draw calls. The 3D overlay covers 80% of the central panel when `ViewMode::SectorView` is active; the 2D map dims behind it. Gate positions (and later stations) are loaded from `zones.xml` into `sector.static_objects` at startup.

**Tech Stack:** wgpu 29, egui-wgpu 0.34.2, eframe 0.34.2, bytemuck 1, glam 0.32, Rust 2024

---

## File Structure

| Path | Role |
|------|------|
| `crates/map-app/src/renderer/mod.rs` | Module root: re-exports `camera`, `mesh`, `gpu` |
| `crates/map-app/src/renderer/camera.rs` | `OrbitCamera` struct: view/proj matrices, rotate, zoom, fit_all |
| `crates/map-app/src/renderer/mesh.rs` | `Vertex`, `Mesh`, `box_mesh`, `ring_mesh`, `sphere_mesh` |
| `crates/map-app/src/renderer/gpu.rs` | `GpuScene` (pipeline + static mesh buffers + dynamic uniform buffer), `SceneCallback` implementing `egui_wgpu::CallbackTrait` |
| `crates/map-app/src/ui/sector_view.rs` | `SectorView3D::show()` – egui overlay panel with header, close button, 3D canvas allocation, mouse input |
| `crates/map-app/src/app.rs` | Store `camera: OrbitCamera`; initialize `GpuScene` in `new`; route `SectorView` mode to overlay |
| `crates/map-app/src/ui/sector_panel.rs` | Add object-list branch when `SectorView` is active |
| `crates/map-app/Cargo.toml` | Add `bytemuck`, `wgpu` |
| `crates/map-io/src/xml_parser.rs` | Enhance zones.xml parsing to populate `sector.static_objects` with gate positions |

---

## Task 1: OrbitCamera — math + unit tests

**Files:**
- Create: `crates/map-app/src/renderer/camera.rs`
- Create: `crates/map-app/src/renderer/mod.rs`

- [ ] **Step 1: Write failing test**

```rust
// crates/map-app/src/renderer/camera.rs (just the test for now)
#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn default_eye_is_above_origin() {
        let cam = OrbitCamera::default();
        let eye = cam.eye();
        // Default: target=ZERO, yaw=0, pitch=0.3, distance=100 → eye is above and in front
        assert!(eye.y > 0.0, "eye must be above target");
        assert!((eye - cam.target).length() > 0.0, "eye must not be at target");
    }

    #[test]
    fn view_matrix_looks_at_target() {
        let cam = OrbitCamera::default();
        let view = cam.view_matrix();
        // Transform target point by view: it should land near origin
        let t_view = view.transform_point3(cam.target);
        assert!(t_view.z < 0.0, "target must be in front (negative z in RH view)");
    }

    #[test]
    fn proj_matrix_maps_center_to_zero() {
        let cam = OrbitCamera::default();
        let proj = cam.proj_matrix(16.0 / 9.0);
        // Forward direction: z=-near should map to clip near=0 in depth
        assert!(proj.w_axis.w != 0.0, "projection must be perspective (non-zero w_axis.w)");
    }

    #[test]
    fn rotate_updates_yaw_pitch() {
        let mut cam = OrbitCamera::default();
        let old_yaw = cam.yaw;
        cam.rotate(0.1, 0.0);
        assert!((cam.yaw - old_yaw - 0.1).abs() < 1e-5);
    }

    #[test]
    fn pitch_clamps_to_avoid_gimbal() {
        let mut cam = OrbitCamera::default();
        cam.rotate(0.0, 10.0); // huge pitch delta
        assert!(cam.pitch <= 85f32.to_radians() + 1e-4);
    }

    #[test]
    fn zoom_changes_distance() {
        let mut cam = OrbitCamera::default();
        let old = cam.distance;
        cam.zoom(-1.0); // scroll down = zoom out
        assert!(cam.distance > old);
    }

    #[test]
    fn fit_all_centers_on_objects() {
        let mut cam = OrbitCamera::default();
        let pts = vec![
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(-10.0, 0.0, 0.0),
        ];
        cam.fit_all(&pts);
        assert!((cam.target - Vec3::ZERO).length() < 1e-3);
        assert!(cam.distance > 10.0, "must be far enough to see ±10 unit spread");
    }

    #[test]
    fn fit_all_empty_resets_to_default() {
        let mut cam = OrbitCamera::default();
        cam.fit_all(&[]);
        assert_eq!(cam.target, Vec3::ZERO);
    }
}
```

- [ ] **Step 2: Run test — expect compile failure (OrbitCamera not defined)**

```bash
cargo test -p map-app 2>&1 | head -20
```
Expected: `error[E0433]: failed to resolve: use of undeclared module`

- [ ] **Step 3: Create renderer/mod.rs**

```rust
// crates/map-app/src/renderer/mod.rs
pub mod camera;
pub mod mesh;
pub mod gpu;
```

- [ ] **Step 4: Create camera.rs with OrbitCamera implementation**

```rust
// crates/map-app/src/renderer/camera.rs
use glam::{Mat4, Vec3};

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,   // radians, horizontal
    pub pitch: f32, // radians, vertical, clamped to ±85°
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 100.0,
            yaw: 0.0,
            pitch: 0.3, // slight downward look
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * sy, sp, cp * cy)
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    pub fn proj_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(60f32.to_radians(), aspect, 0.1, 2_000_000.0)
    }

    /// `dyaw` and `dpitch` in radians. Mouse sensitivity applied by caller.
    pub fn rotate(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch)
            .clamp(-85f32.to_radians(), 85f32.to_radians());
    }

    /// `delta` > 0 = zoom in, < 0 = zoom out (matches scroll_delta.y).
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.1)).clamp(1.0, 5_000_000.0);
    }

    /// Position camera to fit all given positions in view. Resets yaw/pitch.
    pub fn fit_all(&mut self, positions: &[Vec3]) {
        if positions.is_empty() {
            self.target = Vec3::ZERO;
            self.distance = 100.0;
            return;
        }
        let center = positions.iter().copied().sum::<Vec3>() / positions.len() as f32;
        let max_r = positions.iter()
            .map(|p| (*p - center).length())
            .fold(0.0f32, f32::max);
        self.target = center;
        // For 60° FoV half-angle = 30°; distance = radius / tan(30°)
        self.distance = ((max_r + 1.0) / 30f32.to_radians().tan()).max(10.0);
        self.yaw = 0.0;
        self.pitch = 0.3;
    }
}

#[cfg(test)]
mod tests { /* (contents from Step 1) */ }
```

- [ ] **Step 5: Add stub mesh and gpu modules so the crate compiles**

```rust
// crates/map-app/src/renderer/mesh.rs  (stub)
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}
```

```rust
// crates/map-app/src/renderer/gpu.rs  (stub)
// Will be filled in Task 3
```

- [ ] **Step 6: Wire renderer module into main.rs**

In `crates/map-app/src/main.rs`, add before `mod app;`:
```rust
mod renderer;
```

- [ ] **Step 7: Run tests — all must pass**

```bash
cargo test -p map-app 2>&1
```
Expected: all 7 camera tests pass, total 34+ tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/map-app/src/renderer/ crates/map-app/src/main.rs
git commit -m "feat(renderer): OrbitCamera with view/proj matrices, rotate, zoom, fit_all"
```

---

## Task 2: Mesh primitives — box, ring, sphere + unit tests

**Files:**
- Modify: `crates/map-app/src/renderer/mesh.rs`

- [ ] **Step 1: Write failing tests**

```rust
// At bottom of crates/map-app/src/renderer/mesh.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mesh_has_correct_vertex_count() {
        let m = box_mesh([1.0, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]);
        // 6 faces × 4 vertices = 24
        assert_eq!(m.vertices.len(), 24);
        // 6 faces × 2 triangles × 3 indices = 36
        assert_eq!(m.indices.len(), 36);
    }

    #[test]
    fn sphere_mesh_has_triangles() {
        let m = sphere_mesh(1.0, 8, 8, [0.0, 1.0, 0.0, 1.0]);
        assert!(m.indices.len() % 3 == 0, "indices must be multiple of 3");
        assert!(!m.vertices.is_empty());
    }

    #[test]
    fn ring_mesh_has_triangles() {
        let m = ring_mesh(0.5, 1.0, 16, [0.0, 0.8, 1.0, 1.0]);
        assert!(m.indices.len() % 3 == 0);
        assert!(!m.vertices.is_empty());
    }

    #[test]
    fn vertex_color_is_stored() {
        let m = box_mesh([1.0, 1.0, 1.0], [0.2, 0.4, 0.8, 1.0]);
        assert_eq!(m.vertices[0].color, [0.2, 0.4, 0.8, 1.0]);
    }
}
```

- [ ] **Step 2: Run — expect failure (functions not defined)**

```bash
cargo test -p map-app renderer::mesh 2>&1 | head -10
```

- [ ] **Step 3: Implement mesh.rs**

```rust
// crates/map-app/src/renderer/mesh.rs
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color:    [f32; 4],
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices:  Vec<u16>,
}

pub fn box_mesh(half_extents: [f32; 3], color: [f32; 4]) -> Mesh {
    let [hx, hy, hz] = half_extents;
    // 6 faces, each a quad (4 verts, 2 tris)
    let faces: &[([f32; 3], [f32; 3], [f32; 3], [f32; 3])] = &[
        ([-hx,-hy,-hz], [ hx,-hy,-hz], [ hx, hy,-hz], [-hx, hy,-hz]), // -Z
        ([-hx,-hy, hz], [-hx, hy, hz], [ hx, hy, hz], [ hx,-hy, hz]), // +Z
        ([-hx,-hy,-hz], [-hx, hy,-hz], [-hx, hy, hz], [-hx,-hy, hz]), // -X
        ([ hx,-hy,-hz], [ hx,-hy, hz], [ hx, hy, hz], [ hx, hy,-hz]), // +X
        ([-hx,-hy,-hz], [-hx,-hy, hz], [ hx,-hy, hz], [ hx,-hy,-hz]), // -Y
        ([-hx, hy,-hz], [ hx, hy,-hz], [ hx, hy, hz], [-hx, hy, hz]), // +Y
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices  = Vec::with_capacity(36);
    for face in faces {
        let base = vertices.len() as u16;
        for &pos in &[face.0, face.1, face.2, face.3] {
            vertices.push(Vertex { position: pos, color });
        }
        indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
    }
    Mesh { vertices, indices }
}

/// Lat-long UV sphere.
pub fn sphere_mesh(radius: f32, stacks: u32, slices: u32, color: [f32; 4]) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices  = Vec::new();
    for i in 0..=stacks {
        let phi = std::f32::consts::PI * i as f32 / stacks as f32;
        let (sp, cp) = phi.sin_cos();
        for j in 0..=slices {
            let theta = 2.0 * std::f32::consts::PI * j as f32 / slices as f32;
            let (st, ct) = theta.sin_cos();
            vertices.push(Vertex {
                position: [radius * sp * ct, radius * cp, radius * sp * st],
                color,
            });
        }
    }
    for i in 0..stacks {
        for j in 0..slices {
            let a = (i * (slices + 1) + j) as u16;
            let b = a + slices as u16 + 1;
            indices.extend_from_slice(&[a, b, a+1, b, b+1, a+1]);
        }
    }
    Mesh { vertices, indices }
}

/// Flat ring (torus cross-section = 0), oriented in XZ plane.
pub fn ring_mesh(inner: f32, outer: f32, segments: u32, color: [f32; 4]) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices  = Vec::new();
    for i in 0..=segments {
        let t = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
        let (st, ct) = t.sin_cos();
        vertices.push(Vertex { position: [inner * ct, 0.0, inner * st], color });
        vertices.push(Vertex { position: [outer * ct, 0.0, outer * st], color });
    }
    for i in 0..segments {
        let b = (i * 2) as u16;
        indices.extend_from_slice(&[b, b+1, b+2, b+1, b+3, b+2]);
    }
    Mesh { vertices, indices }
}

#[cfg(test)]
mod tests { /* (contents from Step 1) */ }
```

- [ ] **Step 4: Add bytemuck to map-app/Cargo.toml**

```toml
bytemuck = { version = "1", features = ["derive"] }
```

- [ ] **Step 5: Run tests — all must pass**

```bash
cargo test -p map-app renderer::mesh 2>&1
```
Expected: 4 mesh tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/map-app/src/renderer/mesh.rs crates/map-app/Cargo.toml
git commit -m "feat(renderer): box, ring, sphere mesh generation with bytemuck vertices"
```

---

## Task 3: wgpu pipeline + GpuScene

**Files:**
- Modify: `crates/map-app/src/renderer/gpu.rs`
- Modify: `crates/map-app/Cargo.toml`

No unit tests for GPU code — verified visually in Task 4.

- [ ] **Step 1: Add wgpu as direct dependency**

In `crates/map-app/Cargo.toml`:
```toml
wgpu = "29"
```

- [ ] **Step 2: Write WGSL shader source constant and GpuScene struct**

```rust
// crates/map-app/src/renderer/gpu.rs
use std::collections::HashMap;
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use glam::Mat4;
use super::mesh::{Mesh, Vertex};

const SHADER_SRC: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VIn  { @location(0) pos: vec3<f32>, @location(1) col: vec4<f32> }
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) col: vec4<f32> }

@vertex  fn vs(v: VIn) -> VOut {
    return VOut(uniforms.mvp * vec4<f32>(v.pos, 1.0), v.col);
}
@fragment fn fs(v: VOut) -> @location(0) vec4<f32> { return v.col; }
"#;

/// Stride between per-object uniform slots in the dynamic uniform buffer.
/// wgpu requires 256-byte alignment for dynamic uniform offsets.
const UNIFORM_STRIDE: u64 = 256;
const MAX_OBJECTS: u64 = 128;

pub struct GpuMesh {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf:  wgpu::Buffer,
    pub index_count: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MeshKind { Box, Ring, Sphere }

pub struct GpuScene {
    pub pipeline:      wgpu::RenderPipeline,
    pub bgl:           wgpu::BindGroupLayout,
    pub uniform_buf:   wgpu::Buffer,
    pub bind_group:    wgpu::BindGroup,
    pub meshes:        HashMap<MeshKind, GpuMesh>,
}

impl GpuScene {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("3d_scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("3d_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size:   std::num::NonZeroU64::new(64), // mat4 = 64 bytes
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("3d_layout"),
            bind_group_layouts:   &[&bgl],
            push_constant_ranges: &[],
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode:    wgpu::VertexStepMode::Vertex,
            attributes:   &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
        }];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("3d_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:      &shader,
                entry_point: Some("vs"),
                buffers:     &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module:      &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format:     target_format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None, // no depth buffer in egui's render pass
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("3d_uniforms"),
            size:               UNIFORM_STRIDE * MAX_OBJECTS,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("3d_bg"),
            layout:  &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding:  0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buf,
                    offset: 0,
                    size:   std::num::NonZeroU64::new(64),
                }),
            }],
        });

        use super::mesh::{box_mesh, ring_mesh, sphere_mesh};
        let mut meshes = HashMap::new();
        let white = [1.0f32; 4];
        for (kind, mesh) in [
            (MeshKind::Box,    box_mesh([1.0, 1.0, 1.0], white)),
            (MeshKind::Ring,   ring_mesh(0.6, 1.0, 32, white)),
            (MeshKind::Sphere, sphere_mesh(1.0, 12, 12, white)),
        ] {
            meshes.insert(kind, upload_mesh(device, &mesh));
        }

        Self { pipeline, bgl, uniform_buf, bind_group, meshes }
    }
}

fn upload_mesh(device: &wgpu::Device, mesh: &Mesh) -> GpuMesh {
    use wgpu::util::DeviceExt;
    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("mesh_vb"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage:    wgpu::BufferUsages::VERTEX,
    });
    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:    Some("mesh_ib"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage:    wgpu::BufferUsages::INDEX,
    });
    GpuMesh { vertex_buf, index_buf, index_count: mesh.indices.len() as u32 }
}
```

- [ ] **Step 3: Add SceneCallback and DrawCall types**

Append to `gpu.rs`:

```rust
/// One object to render: mesh type, MVP matrix, color.
#[repr(C, align(256))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniforms {
    mvp: [[f32; 4]; 4],
    _pad: [f32; 48], // 16 (mat4) + 48 = 64 floats = 256 bytes
}

pub struct DrawCall {
    pub kind:  MeshKind,
    pub mvp:   Mat4,
    pub color: [f32; 4],
}

/// Created each frame with current camera + draw list.
pub struct SceneCallback {
    pub draw_calls: Vec<DrawCall>,
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        _device:      &wgpu::Device,
        queue:         &wgpu::Queue,
        _screen_desc:  &egui_wgpu::ScreenDescriptor,
        _encoder:      &mut wgpu::CommandEncoder,
        resources:     &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(scene) = resources.get::<GpuScene>() else { return vec![]; };
        let mut data = vec![[0u8; 256]; self.draw_calls.len().min(MAX_OBJECTS as usize)];
        for (i, dc) in self.draw_calls.iter().take(MAX_OBJECTS as usize).enumerate() {
            // Pack MVP into the first 64 bytes of a 256-byte slot
            let m: [[f32; 4]; 4] = dc.mvp.to_cols_array_2d();
            let floats: &[f32; 16] = bytemuck::cast_ref(&m);
            let bytes: &[u8; 64] = bytemuck::cast_ref(floats);
            data[i][..64].copy_from_slice(bytes);
        }
        if !data.is_empty() {
            queue.write_buffer(
                &scene.uniform_buf,
                0,
                bytemuck::cast_slice(data.as_slice()),
            );
        }
        vec![]
    }

    fn paint(
        &self,
        _info:     egui_wgpu::PaintCallbackInfo,
        rpass:     &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(scene) = resources.get::<GpuScene>() else { return; };
        rpass.set_pipeline(&scene.pipeline);
        for (i, dc) in self.draw_calls.iter().take(MAX_OBJECTS as usize).enumerate() {
            let Some(gpu_mesh) = scene.meshes.get(&dc.kind) else { continue; };
            let offset = (i as u64 * UNIFORM_STRIDE) as u32;
            rpass.set_bind_group(0, &scene.bind_group, &[offset]);
            rpass.set_vertex_buffer(0, gpu_mesh.vertex_buf.slice(..));
            rpass.set_index_buffer(gpu_mesh.index_buf.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..gpu_mesh.index_count, 0, 0..1);
        }
    }
}
```

- [ ] **Step 4: Build — must compile with zero errors**

```bash
cargo build -p map-app 2>&1 | grep "^error" | head -20
```
Expected: no errors. Warnings about unused imports are OK.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/renderer/gpu.rs crates/map-app/Cargo.toml
git commit -m "feat(renderer): wgpu pipeline, GpuScene, SceneCallback with dynamic uniforms"
```

---

## Task 4: App wires up GpuScene + renders first frame

**Files:**
- Modify: `crates/map-app/src/app.rs`
- Create: `crates/map-app/src/ui/sector_view.rs`
- Modify: `crates/map-app/src/ui/mod.rs`

- [ ] **Step 1: Initialize GpuScene in App::new**

Replace `crates/map-app/src/app.rs` with:

```rust
use map_domain::universe::Universe;
use map_domain::view::ViewMode;
use crate::ui::{top_bar::TopBar, map_view::MapView, sector_panel::SectorPanel, sector_view::SectorView3D};
use crate::renderer::camera::OrbitCamera;

pub struct App {
    pub universe:     Universe,
    pub view_mode:    ViewMode,
    pub camera:       OrbitCamera,
    top_bar:          TopBar,
    map_view:         MapView,
    sector_panel:     SectorPanel,
    sector_view:      SectorView3D,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, universe: Universe) -> Self {
        crate::theme::apply(&cc.egui_ctx);

        // Set up wgpu scene resources (stored in egui_wgpu's callback_resources)
        if let Some(rs) = &cc.wgpu_render_state {
            let scene = crate::renderer::gpu::GpuScene::new(&rs.device, rs.target_format);
            rs.renderer.write().callback_resources.insert(scene);
        }

        Self {
            universe,
            view_mode: ViewMode::initial(),
            camera:    OrbitCamera::default(),
            top_bar:   TopBar::default(),
            map_view:  MapView::default(),
            sector_panel: SectorPanel::default(),
            sector_view:  SectorView3D::default(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Global keyboard handling
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            self.view_mode = match self.view_mode.clone() {
                ViewMode::SectorView { sector, selected_obj: Some(_) } => {
                    // Reset camera; deselect object
                    let sec = self.universe.sector(sector);
                    if let Some(s) = sec {
                        let positions: Vec<_> = s.static_objects.iter()
                            .map(|o| o.position)
                            .collect();
                        self.camera.fit_all(&positions);
                    }
                    ViewMode::SectorView { sector, selected_obj: None }
                }
                other => other,
            };
        }

        egui::Panel::top("top_bar")
            .exact_size(36.0)
            .show_inside(ui, |ui| {
                self.top_bar.show(ui);
            });

        egui::Panel::right("sector_panel")
            .exact_size(220.0)
            .resizable(false)
            .show_inside(ui, |ui| {
                let selected = self.view_mode.selected_sector();
                let sector   = selected.and_then(|id| self.universe.sector(id));
                let panel_resp = self.sector_panel.show(ui, sector, &self.universe, &self.view_mode);
                if panel_resp.open_3d_clicked {
                    if let Some(s) = sector {
                        let positions: Vec<_> = s.static_objects.iter()
                            .map(|o| o.position).collect();
                        self.camera.fit_all(&positions);
                    }
                    self.view_mode = self.view_mode.clone().open_sector_3d();
                }
                if panel_resp.back_to_map_clicked {
                    self.view_mode = self.view_mode.clone().close_sector_3d();
                }
                if let Some(obj_id) = panel_resp.object_clicked {
                    self.view_mode = self.view_mode.clone().select_object(obj_id);
                    // Focus camera on that object
                    if let ViewMode::SectorView { sector, .. } = &self.view_mode {
                        if let Some(s) = self.universe.sector(*sector) {
                            if let Some(obj) = s.static_objects.iter().find(|o| o.id == obj_id) {
                                self.camera.fit_all(&[obj.position]);
                            }
                        }
                    }
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let selected = self.view_mode.selected_sector();

            match &self.view_mode.clone() {
                ViewMode::SectorView { sector, selected_obj } => {
                    let sec = self.universe.sector(*sector);
                    let sv_resp = self.sector_view.show(
                        ui, sec, &mut self.camera, *selected_obj,
                    );
                    if sv_resp.close_clicked {
                        self.view_mode = self.view_mode.clone().close_sector_3d();
                    }
                    if let Some(obj_id) = sv_resp.clicked_object {
                        self.view_mode = self.view_mode.clone().select_object(obj_id);
                        if let Some(s) = sec {
                            if let Some(obj) = s.static_objects.iter().find(|o| o.id == obj_id) {
                                self.camera.fit_all(&[obj.position]);
                            }
                        }
                    }
                    // Map still in background (not interactive)
                    let map_rect = ui.available_rect_before_wrap();
                    let painter = ui.painter_at(map_rect);
                    painter.rect_filled(
                        map_rect,
                        0.0,
                        egui::Color32::from_black_alpha(180),
                    );
                }
                ViewMode::UniverseMap { .. } => {
                    let mvr = self.map_view.show(ui, &self.universe, selected);
                    if let Some(sector_id) = mvr.double_clicked_sector {
                        let s = self.universe.sector(sector_id);
                        if let Some(sec) = s {
                            let positions: Vec<_> = sec.static_objects.iter()
                                .map(|o| o.position).collect();
                            self.camera.fit_all(&positions);
                        }
                        self.view_mode = self.view_mode.clone()
                            .select_sector(sector_id).open_sector_3d();
                    } else if let Some(sector_id) = mvr.clicked_sector {
                        self.view_mode = self.view_mode.clone().select_sector(sector_id);
                    }
                }
            }
        });
    }
}
```

- [ ] **Step 2: Create sector_view.rs stub**

```rust
// crates/map-app/src/ui/sector_view.rs
use map_domain::ids::ObjectId;
use map_domain::universe::Sector;
use crate::renderer::camera::OrbitCamera;

pub struct SectorViewResponse {
    pub close_clicked:  bool,
    pub clicked_object: Option<ObjectId>,
}

#[derive(Default)]
pub struct SectorView3D;

impl SectorView3D {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        sector:       Option<&Sector>,
        camera:       &mut OrbitCamera,
        selected_obj: Option<ObjectId>,
    ) -> SectorViewResponse {
        // Placeholder: just show a dark rectangle with header
        let available = ui.available_rect_before_wrap();
        let painter = ui.painter_at(available);
        painter.rect_filled(available, 4.0, egui::Color32::from_rgb(5, 7, 12));

        if let Some(sector) = sector {
            painter.text(
                available.center_top() + egui::Vec2::new(0.0, 12.0),
                egui::Align2::CENTER_TOP,
                &sector.name,
                egui::FontId::proportional(14.0),
                crate::theme::TEXT_PRIMARY,
            );
        }

        SectorViewResponse { close_clicked: false, clicked_object: None }
    }
}
```

- [ ] **Step 3: Update sector_panel.rs to accept view_mode and return object_clicked**

Add `object_clicked` to `SectorPanelResponse` and pass `view_mode` to `show()`:

```rust
// crates/map-app/src/ui/sector_panel.rs
use map_domain::ids::ObjectId;
use map_domain::universe::{Sector, Universe, GateType};
use map_domain::view::ViewMode;
use crate::theme;

pub struct SectorPanelResponse {
    pub open_3d_clicked:   bool,
    pub back_to_map_clicked: bool,
    pub object_clicked:    Option<ObjectId>,
}

#[derive(Default)]
pub struct SectorPanel;

impl SectorPanel {
    pub fn show(
        &mut self,
        ui:        &mut egui::Ui,
        sector:    Option<&Sector>,
        universe:  &Universe,
        view_mode: &ViewMode,
    ) -> SectorPanelResponse {
        ui.add_space(8.0);

        let Some(sector) = sector else {
            ui.colored_label(theme::TEXT_MUTED, "Select a sector");
            ui.add_space(4.0);
            ui.colored_label(theme::TEXT_MUTED, "Click on the map.");
            return SectorPanelResponse {
                open_3d_clicked: false, back_to_map_clicked: false, object_clicked: None,
            };
        };

        let back_clicked = ui.small_button("← Universe").clicked();
        ui.add_space(4.0);

        ui.colored_label(theme::TEXT_MUTED, "SECTOR");
        ui.add_space(2.0);
        ui.colored_label(theme::TEXT_PRIMARY, &sector.name);
        if let Some(faction_id) = sector.faction {
            ui.colored_label(theme::ACCENT, format!("Faction #{}", faction_id.0));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        let mut object_clicked = None;

        if let ViewMode::SectorView { selected_obj, .. } = view_mode {
            // Show object list
            ui.colored_label(theme::TEXT_MUTED, "OBJECTS");
            ui.add_space(4.0);
            if sector.static_objects.is_empty() {
                ui.colored_label(theme::TEXT_MUTED, "None loaded");
            }
            for obj in &sector.static_objects {
                let is_sel = *selected_obj == Some(obj.id);
                let label = format!("{} {}", kind_icon(&obj.kind), &obj.name);
                let color = if is_sel { theme::ACCENT } else { theme::TEXT_PRIMARY };
                if ui.colored_label(color, &label).clicked() {
                    object_clicked = Some(obj.id);
                }
            }
        } else {
            // Show connections (map mode)
            ui.colored_label(theme::TEXT_MUTED, "CONNECTIONS");
            ui.add_space(4.0);
            let neighbours = universe.neighbour_ids(sector.id);
            let conns = universe.connections_for(sector.id);
            if neighbours.is_empty() {
                ui.colored_label(theme::TEXT_MUTED, "None");
            }
            for nb_id in &neighbours {
                if let Some(nb) = universe.sector(*nb_id) {
                    let gate_type = conns.iter()
                        .find(|c| c.from == *nb_id || c.to == *nb_id)
                        .map(|c| &c.gate_type);
                    let prefix = match gate_type {
                        Some(GateType::Superhighway) => "⇒",
                        _ => "→",
                    };
                    ui.colored_label(theme::TEXT_PRIMARY, format!("{} {}", prefix, nb.name));
                }
            }
        }

        ui.add_space(12.0);
        let open_clicked = ui.button("▣  Open 3D View").clicked();

        SectorPanelResponse {
            open_3d_clicked: open_clicked,
            back_to_map_clicked: back_clicked,
            object_clicked,
        }
    }
}

fn kind_icon(kind: &map_domain::objects::StaticObjectKind) -> &'static str {
    use map_domain::objects::StaticObjectKind::*;
    match kind {
        Station      => "◼",
        Gate         => "◯",
        ResourceZone => "◎",
        Anomaly      => "✦",
    }
}
```

- [ ] **Step 4: Add sector_view to ui/mod.rs**

```rust
// crates/map-app/src/ui/mod.rs
pub mod top_bar;
pub mod map_view;
pub mod sector_panel;
pub mod sector_view;
```

- [ ] **Step 5: Build — must compile**

```bash
cargo build -p map-app 2>&1 | grep "^error" | head -20
```
Expected: no errors.

- [ ] **Step 6: Run — app must open, 2D map still works**

```bash
cargo run 2>&1 | tail -3
```
Expected: `[map] Loaded 76 sectors.` — open the app, click a sector, verify right panel still shows info.

- [ ] **Step 7: Commit**

```bash
git add crates/map-app/src/app.rs crates/map-app/src/ui/
git commit -m "feat(app): wire GpuScene into eframe, SectorView3D stub, updated SectorPanel"
```

---

## Task 5: SectorView3D — 3D canvas with camera input

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs`

- [ ] **Step 1: Implement full SectorView3D::show with 3D paint callback**

Replace the entire file:

```rust
// crates/map-app/src/ui/sector_view.rs
use std::sync::Arc;
use egui::{Pos2, Rect, Sense, Vec2};
use glam::{Mat4, Vec3};
use map_domain::ids::ObjectId;
use map_domain::objects::StaticObjectKind;
use map_domain::universe::Sector;
use crate::renderer::camera::OrbitCamera;
use crate::renderer::gpu::{DrawCall, MeshKind, SceneCallback};
use crate::theme;

pub struct SectorViewResponse {
    pub close_clicked:  bool,
    pub clicked_object: Option<ObjectId>,
}

#[derive(Default)]
pub struct SectorView3D;

impl SectorView3D {
    pub fn show(
        &mut self,
        ui:          &mut egui::Ui,
        sector:      Option<&Sector>,
        camera:      &mut OrbitCamera,
        selected_obj: Option<ObjectId>,
    ) -> SectorViewResponse {
        let mut close_clicked  = false;
        let mut clicked_object = None;

        let available = ui.available_rect_before_wrap();

        // Allocate 80% width for the 3D canvas
        let canvas_w = available.width() * 0.80;
        let canvas_rect = Rect::from_min_size(
            available.min + Vec2::new((available.width() - canvas_w) * 0.5, 0.0),
            Vec2::new(canvas_w, available.height()),
        );

        // Header bar (30px) at top of canvas
        let header_rect = Rect::from_min_size(canvas_rect.min, Vec2::new(canvas_w, 30.0));
        let view_rect   = Rect::from_min_size(
            canvas_rect.min + Vec2::new(0.0, 30.0),
            Vec2::new(canvas_w, available.height() - 30.0),
        );

        // Draw header
        let painter = ui.painter();
        painter.rect_filled(header_rect, 0.0, egui::Color32::from_rgb(15, 18, 28));
        if let Some(s) = sector {
            painter.text(
                header_rect.center(),
                egui::Align2::CENTER_CENTER,
                &s.name,
                egui::FontId::proportional(13.0),
                theme::TEXT_PRIMARY,
            );
        }
        // Close button
        let close_rect = Rect::from_center_size(
            Pos2::new(header_rect.right() - 20.0, header_rect.center().y),
            Vec2::splat(20.0),
        );
        painter.text(
            close_rect.center(),
            egui::Align2::CENTER_CENTER,
            "✕",
            egui::FontId::proportional(12.0),
            theme::TEXT_MUTED,
        );
        let close_resp = ui.allocate_rect(close_rect, Sense::click());
        if close_resp.clicked() {
            close_clicked = true;
        }

        // 3D canvas input (drag = rotate, scroll = zoom)
        let (_, canvas_resp) = ui.allocate_exact_size(view_rect.size(), Sense::click_and_drag());
        // Reposition the response rect manually since we allocated from a specific origin
        // by using allocate_rect:
        let canvas_resp = ui.allocate_rect(view_rect, Sense::click_and_drag());

        if canvas_resp.dragged() {
            let delta = canvas_resp.drag_delta();
            camera.rotate(delta.x * 0.005, -delta.y * 0.005);
        }
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if canvas_resp.contains_pointer() && scroll != 0.0 {
            camera.zoom(scroll * 0.01);
        }

        // Screen-space object picking on click
        if canvas_resp.clicked() {
            if let (Some(ptr), Some(sector)) = (canvas_resp.interact_pointer_pos(), sector) {
                clicked_object = pick_object(ptr, view_rect, camera, sector);
            }
        }

        // Build draw calls from sector objects
        let draw_calls = sector.map(|s| build_draw_calls(s, selected_obj)).unwrap_or_default();

        // Push wgpu paint callback
        let aspect = view_rect.width() / view_rect.height().max(1.0);
        let vp = camera.view_matrix();
        let proj = camera.proj_matrix(aspect);
        // Re-project each draw call's MVP with current camera
        let draw_calls_with_vp: Vec<DrawCall> = draw_calls.into_iter().map(|mut dc| {
            dc.mvp = proj * vp * dc.mvp;
            dc
        }).collect();

        let cb = egui_wgpu::Callback::new_paint_callback(
            view_rect,
            SceneCallback { draw_calls: draw_calls_with_vp },
        );
        ui.painter().add(cb);

        // Dark border around canvas
        ui.painter().rect_stroke(canvas_rect, 2.0, egui::Stroke::new(1.0, theme::BORDER));

        SectorViewResponse { close_clicked, clicked_object }
    }
}

/// Build world-space draw calls (MVP = model only; view/proj applied in show()).
fn build_draw_calls(sector: &Sector, selected: Option<ObjectId>) -> Vec<DrawCall> {
    sector.static_objects.iter().map(|obj| {
        let scale: f32 = match obj.kind {
            StaticObjectKind::Station      => 3.0,
            StaticObjectKind::Gate         => 4.0,
            StaticObjectKind::ResourceZone => 8.0,
            StaticObjectKind::Anomaly      => 2.0,
        };
        let kind = match obj.kind {
            StaticObjectKind::Station      => MeshKind::Box,
            StaticObjectKind::Gate         => MeshKind::Ring,
            StaticObjectKind::ResourceZone => MeshKind::Sphere,
            StaticObjectKind::Anomaly      => MeshKind::Sphere,
        };
        let color = if selected == Some(obj.id) {
            [1.0, 0.8, 0.1, 1.0]  // selected: yellow
        } else {
            kind_color(&obj.kind)
        };
        // Model matrix: translate to object position + uniform scale
        let model = Mat4::from_translation(obj.position)
            * Mat4::from_scale(glam::Vec3::splat(scale));
        // Color baked into vertex data at mesh generation time is white;
        // apply color by recoloring draw call (ShaderCallback applies it)
        DrawCall { kind, mvp: model, color }
    }).collect()
}

fn kind_color(kind: &StaticObjectKind) -> [f32; 4] {
    match kind {
        StaticObjectKind::Station      => [0.4, 0.6, 1.0, 1.0],  // blue
        StaticObjectKind::Gate         => [0.2, 0.9, 0.4, 1.0],  // green
        StaticObjectKind::ResourceZone => [0.5, 0.3, 0.9, 0.5],  // purple, semi-transparent
        StaticObjectKind::Anomaly      => [1.0, 0.4, 0.2, 1.0],  // orange
    }
}

/// Screen-space picking: project each object to screen, return nearest to click.
fn pick_object(
    ptr:    Pos2,
    rect:   Rect,
    camera: &OrbitCamera,
    sector: &Sector,
) -> Option<ObjectId> {
    let aspect = rect.width() / rect.height().max(1.0);
    let vp   = camera.view_matrix();
    let proj = camera.proj_matrix(aspect);
    let mvp  = proj * vp;

    let mut best_id   = None;
    let mut best_dist = f32::MAX;

    for obj in &sector.static_objects {
        let clip = mvp * obj.position.extend(1.0);
        if clip.w <= 0.0 { continue; } // behind camera
        let ndc = clip.xyz() / clip.w;
        // NDC to screen
        let sx = (ndc.x * 0.5 + 0.5) * rect.width()  + rect.left();
        let sy = (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height() + rect.top();
        let dist = ((sx - ptr.x).powi(2) + (sy - ptr.y).powi(2)).sqrt();
        if dist < 20.0 && dist < best_dist {
            best_dist = dist;
            best_id = Some(obj.id);
        }
    }
    best_id
}
```

- [ ] **Step 2: Fix `DrawCall` to carry color**

The color from `kind_color` needs to reach the shader. Currently vertex color is baked as white at mesh-generation time and `DrawCall.color` is unused. Update `gpu.rs` to multiply vertex color by draw call color in `prepare`, or pass it via the uniform buffer.

Simplest fix: add `color: [f32; 4]` to `ObjectUniforms` and update the WGSL shader.

In `gpu.rs`, update `SHADER_SRC`:
```wgsl
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
```

Update `ObjectUniforms` in `gpu.rs` to include color (must still fit in 256 bytes: 64 bytes mvp + 16 bytes color = 80 bytes, pad to 256):
```rust
#[repr(C, align(256))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniforms {
    mvp:   [[f32; 4]; 4], // 64 bytes
    color: [f32; 4],      // 16 bytes
    _pad:  [f32; 44],     // 176 bytes → total 256
}
```

Update `min_binding_size` in the bind group layout to `NonZeroU64::new(80)`.

Update `prepare` in `SceneCallback` to also write the color:
```rust
// In prepare, replace the data packing loop:
for (i, dc) in self.draw_calls.iter().take(MAX_OBJECTS as usize).enumerate() {
    let m: [[f32; 4]; 4] = dc.mvp.to_cols_array_2d();
    let mvp_bytes: [u8; 64] = bytemuck::cast(m);
    let col_bytes: [u8; 16] = bytemuck::cast(dc.color);
    let mut slot = [0u8; 256];
    slot[..64].copy_from_slice(&mvp_bytes);
    slot[64..80].copy_from_slice(&col_bytes);
    queue.write_buffer(&scene.uniform_buf, i as u64 * UNIFORM_STRIDE, &slot);
}
```

- [ ] **Step 3: Build and run — open a sector's 3D view**

```bash
cargo run 2>&1 | tail -3
```
Expected: app opens; clicking a sector and clicking "Open 3D View" (or double-clicking sector) shows a dark panel with the sector name. Since static_objects is still empty, the 3D canvas is blank but functional.

- [ ] **Step 4: Verify camera responds to mouse**

With the 3D view open: drag within the dark canvas area → no crash. Scroll → no crash.

- [ ] **Step 5: Commit**

```bash
git add crates/map-app/src/ui/sector_view.rs crates/map-app/src/renderer/gpu.rs
git commit -m "feat(3d): SectorView3D with camera mouse input, object picking, paint callback"
```

---

## Task 6: Load gate positions from zones.xml

**Files:**
- Modify: `crates/map-io/src/xml_parser.rs`

- [ ] **Step 1: Write integration test**

In `crates/map-io/tests/xml_parser_test.rs`, add:

```rust
#[test]
fn parse_galaxy_str_with_gates_is_placeholder_for_real_test() {
    // The fixture galaxy XML doesn't have zone data, so this test just verifies
    // parse_galaxy_from_game populates static_objects when run live.
    // Real verification is done by running the app and seeing gates in 3D view.
    // This test verifies zone_name_to_sector_macro works:
    use map_io::xml_parser::zone_name_to_sector_macro_pub;
    assert_eq!(
        zone_name_to_sector_macro_pub("Zone003_Cluster_01_Sector001_macro"),
        Some("Cluster_01_Sector001_macro".to_string()),
    );
    assert_eq!(
        zone_name_to_sector_macro_pub("NotAZone"),
        None,
    );
}
```

Make `zone_name_to_sector_macro` public (rename to `zone_name_to_sector_macro_pub` or just `pub fn zone_name_to_sector_macro`):

In `xml_parser.rs`, change:
```rust
fn zone_name_to_sector_macro(name: &str) -> Option<String> {
```
to:
```rust
pub fn zone_name_to_sector_macro(name: &str) -> Option<String> {
```

And in the test use the public function.

- [ ] **Step 2: Run test — expect failure (function not public)**

```bash
cargo test -p map-io zone_name_to_sector_macro 2>&1 | head -10
```

- [ ] **Step 3: Make zone_name_to_sector_macro pub**

In `xml_parser.rs` line ~509, change `fn` to `pub fn`.

- [ ] **Step 4: Run test — must pass**

```bash
cargo test -p map-io zone_name_to_sector_macro 2>&1
```

- [ ] **Step 5: Add gate position extraction to parse_galaxy_from_game**

In `parse_galaxy_from_game`, after building `gate_pairs` (connections), add a second pass to extract gate positions. The positions in zones.xml are the gate connection endpoints.

Add a new function `parse_gate_positions_xml`:

```rust
/// zones.xml: sector_macro → list of gate positions (in sector-local coordinates, km).
///
/// Gate connections are named `connection_ClusterGate{N}To{M}`.
/// Their `<offset><position x y z />` is in metres; we divide by 1000 to get km.
pub fn parse_gate_positions_xml(
    xml: &str,
) -> HashMap<String, Vec<(f32, f32, f32, String)>> { // macro → Vec<(x,y,z,dest_name)>
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut result: HashMap<String, Vec<(f32, f32, f32, String)>> = HashMap::new();
    let mut current_sector: Option<String> = None;
    let mut in_gate_conn = false;
    let mut in_offset    = false;
    let mut gate_dest: Option<String> = None;
    let mut gate_pos: (f32, f32, f32) = (0.0, 0.0, 0.0);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"macro" => {
                    let class = attr_value(e, b"class").unwrap_or_default();
                    if class == "zone" {
                        let name = attr_value(e, b"name").unwrap_or_default();
                        current_sector = zone_name_to_sector_macro(&name);
                    }
                }
                b"connection" if current_sector.is_some() => {
                    let conn_name = attr_value(e, b"name").unwrap_or_default();
                    if parse_gate_cluster_nums(&conn_name).is_some() {
                        in_gate_conn = true;
                        gate_pos = (0.0, 0.0, 0.0);
                        gate_dest = Some(conn_name);
                    }
                }
                b"offset" if in_gate_conn => in_offset = true,
                _ => {}
            },
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"position" && in_offset {
                    gate_pos.0 = attr_value(e, b"x").and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    gate_pos.1 = attr_value(e, b"y").and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    gate_pos.2 = attr_value(e, b"z").and_then(|s| s.parse().ok()).unwrap_or(0.0);
                }
            }
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"offset" => in_offset = false,
                b"connection" if in_gate_conn => {
                    if let (Some(sector), Some(dest)) = (&current_sector, gate_dest.take()) {
                        result.entry(sector.clone()).or_default().push((
                            gate_pos.0 / 1000.0, // metres → km
                            gate_pos.1 / 1000.0,
                            gate_pos.2 / 1000.0,
                            dest,
                        ));
                    }
                    in_gate_conn = false;
                }
                b"macro" => current_sector = None,
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }
    result
}
```

- [ ] **Step 6: Integrate gate positions into sector construction**

In `parse_galaxy_from_game`, after parsing zones_str for gate_pairs, also extract gate positions and populate `sector.static_objects`:

```rust
// After: let gate_pairs = parse_gate_connections_xml(&zones_str, &sector_placements);
let gate_positions = parse_gate_positions_xml(&zones_str);

// Populate sectors with gate static objects
let mut gate_obj_counter = 0u32;
for sector in &mut sectors {
    // Reverse-lookup sector macro name from id
    let sector_macro = macro_to_id.iter()
        .find_map(|(k, &v)| if v == sector.id { Some(k.as_str()) } else { None });
    if let Some(sm) = sector_macro {
        if let Some(gates) = gate_positions.get(sm) {
            for (x, y, z, dest_name) in gates {
                gate_obj_counter += 1;
                sector.static_objects.push(StaticObject {
                    id:       ObjectId(10_000 + gate_obj_counter),
                    kind:     map_domain::objects::StaticObjectKind::Gate,
                    position: glam::Vec3::new(*x, *y, *z),
                    faction:  None,
                    name:     dest_name.clone(),
                });
            }
        }
    }
}
```

Note: `macro_to_id` is built in the earlier loop and maps `sector_macro → SectorId`. We need to build the reverse after the sectors Vec is built, OR use a separate `id_to_macro` map built in the same loop.

Add `id_to_macro: HashMap<SectorId, String>` alongside `macro_to_id`:

```rust
let mut id_to_macro: HashMap<SectorId, String> = HashMap::new();
// Inside the sector construction loop:
id_to_macro.insert(id, sector_macro.clone());
```

Then use `id_to_macro` in the gate population loop:

```rust
for sector in &mut sectors {
    if let Some(sm) = id_to_macro.get(&sector.id) {
        if let Some(gates) = gate_positions.get(sm) { ... }
    }
}
```

- [ ] **Step 7: Build and run — log should show sectors with gates loaded**

```bash
cargo run 2>&1 | tail -5
```

Add a debug log line temporarily to verify:
```rust
eprintln!("[map] Gate objects loaded: {}", gate_obj_counter);
```
Expected output includes the gate count.

- [ ] **Step 8: Run all tests — must still pass**

```bash
cargo test 2>&1 | tail -5
```
Expected: 35+ tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/map-io/src/xml_parser.rs crates/map-io/tests/xml_parser_test.rs
git commit -m "feat(io): load gate positions from zones.xml into sector.static_objects"
```

---

## Task 7: Render objects visible in 3D view

**Files:**
- Modify: `crates/map-app/src/ui/sector_view.rs` (ensure build_draw_calls works)
- Modify: `crates/map-app/src/renderer/gpu.rs` (fix write_buffer call — must not write >MAX_OBJECTS)

- [ ] **Step 1: Verify queue.write_buffer writes correct byte count**

In `SceneCallback::prepare` in `gpu.rs`, the `queue.write_buffer` must write exactly `n * UNIFORM_STRIDE` bytes where `n = draw_calls.len().min(MAX_OBJECTS)`. Refactor to use a single `write_buffer` call:

```rust
fn prepare(&self, _device: &wgpu::Device, queue: &wgpu::Queue,
           _sd: &egui_wgpu::ScreenDescriptor, _enc: &mut wgpu::CommandEncoder,
           resources: &mut egui_wgpu::CallbackResources) -> Vec<wgpu::CommandBuffer> {
    let Some(scene) = resources.get::<GpuScene>() else { return vec![]; };
    let n = self.draw_calls.len().min(MAX_OBJECTS as usize);
    if n == 0 { return vec![]; }

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
    vec![]
}
```

- [ ] **Step 2: Build + run — open 3D view, see gate rings**

```bash
cargo run
```
Double-click "Argon Prime" → 3D view opens → should see green ring meshes at gate positions.

If the view is blank: check that `sector.static_objects` was populated (add a temporary `eprintln!` if needed).

If the rings are too small or too large: adjust scale in `build_draw_calls` (currently 4.0 for gates) and `camera.fit_all` distance calculation.

- [ ] **Step 3: Commit**

```bash
git add crates/map-app/src/renderer/gpu.rs crates/map-app/src/ui/sector_view.rs
git commit -m "feat(3d): render gate objects as ring meshes in 3D sector view"
```

---

## Task 8: Object selection + panel sync

**Files:**
- Verify: `crates/map-app/src/ui/sector_view.rs` — picking already implemented
- Verify: `crates/map-app/src/app.rs` — clicked_object handling already implemented
- Verify: `crates/map-app/src/ui/sector_panel.rs` — object list already implemented

- [ ] **Step 1: Test click-to-select in running app**

```bash
cargo run
```
1. Double-click a sector with multiple gate connections → 3D view opens
2. Click on a gate ring → it turns yellow in the 3D view (selected color)
3. Right panel shows the object list with the clicked gate highlighted

Expected: yellow ring highlights the selected gate.

- [ ] **Step 2: Test panel list click**

In the right panel, click on an object name in the OBJECTS list:
- Camera should move to orbit that object (via `camera.fit_all([obj.position])` in app.rs)
- Object turns yellow

- [ ] **Step 3: Fix panel click if broken**

If panel object list clicks don't update the camera, verify `app.rs` handles `panel_resp.object_clicked`:
The handler in `app.rs` already does:
```rust
if let Some(obj_id) = panel_resp.object_clicked {
    self.view_mode = self.view_mode.clone().select_object(obj_id);
    if let ViewMode::SectorView { sector, .. } = &self.view_mode {
        if let Some(s) = self.universe.sector(*sector) {
            if let Some(obj) = s.static_objects.iter().find(|o| o.id == obj_id) {
                self.camera.fit_all(&[obj.position]);
            }
        }
    }
}
```
If not present, add it.

- [ ] **Step 4: Commit (if any fixes)**

```bash
git add crates/map-app/src/
git commit -m "fix(3d): object selection syncs camera and highlights in panel list"
```

---

## Task 9: Escape + close behavior

**Files:**
- Verify: `crates/map-app/src/app.rs` — Escape key handler already added

- [ ] **Step 1: Test Escape key with object selected**

1. Open 3D view of a sector
2. Click a gate → gate turns yellow
3. Press Escape → gate deselects (turns green), camera resets to fit-all

Expected: object deselected, camera zooms back out.

- [ ] **Step 2: Test Escape key with no object selected**

1. Open 3D view, nothing selected
2. Press Escape → nothing happens (view stays open, camera unchanged)

This matches the spec: `Escape` = deselect + reset camera, NOT close.

- [ ] **Step 3: Test ✕ button closes view**

In the 3D panel header, clicking ✕ → returns to 2D map view with the sector still selected.

Expected: right panel shows sector info (connections), ← Universe button visible.

- [ ] **Step 4: Test ← Universe button in panel**

With any sector selected in map mode, clicking ← Universe in the sector panel:
Expected: selection cleared, panel shows "Select a sector".

Note: ← Universe in `SectorPanel` already calls `close_sector_3d()` via `back_to_map_clicked`. In map mode it should deselect — review `app.rs`. If in universe map mode, the back button should do nothing; in sector view mode it should close. Verify the current behavior and fix if it incorrectly deselects when in map mode.

In `app.rs`, confirm:
```rust
if panel_resp.back_to_map_clicked {
    self.view_mode = self.view_mode.clone().close_sector_3d();
}
```
`close_sector_3d()` on `UniverseMap` is a no-op — correct.

- [ ] **Step 5: Run all tests**

```bash
cargo test 2>&1 | tail -5
```
Expected: all 35+ tests pass.

- [ ] **Step 6: Final commit**

```bash
git add crates/map-app/src/
git commit -m "feat(3d): complete Phase 2 — Escape, close, object selection, gate rendering"
```

---

## Phase 2 Acceptance Criteria

Before declaring Phase 2 complete, verify all manually:

- [ ] `cargo test` passes with zero failures
- [ ] App opens; 2D map still works (pan, zoom, sector select)
- [ ] Double-clicking a sector opens 3D view; clicking "Open 3D View" button also works
- [ ] 3D canvas shows gate ring meshes at correct sector positions
- [ ] Orbit camera: drag rotates the scene, scroll zooms
- [ ] Clicking a gate in 3D: gate turns yellow, right panel highlights it in the list
- [ ] Clicking gate in right panel list: camera moves to orbit that gate, gate highlighted in 3D
- [ ] Pressing Escape with object selected: object deselects, camera resets to fit-all
- [ ] Pressing Escape with nothing selected: no crash, view stays open
- [ ] Clicking ✕ in 3D header: returns to 2D map, sector still selected
- [ ] 2D map behind 3D view is visibly dimmed

---

## Notes for Implementers

**wgpu render pass depth:** The egui render pass has no depth attachment. Objects at overlapping screen positions may show incorrect ordering. For Phase 2 this is acceptable — gate positions in X4 sectors are well-separated. Depth buffer can be added in Phase 4.

**Static object positions:** Only gate positions are loaded in this phase (from zones.xml). Stations and resource zones come in a later task using the god.xml parser.

**Gate count:** Sectors typically have 2–6 gates. The camera `fit_all` should work correctly for this range.

**Scale:** Gate positions are converted from metres to km (÷ 1000). The camera near/far planes (0.1 to 2,000,000 km) accommodate gate separations of hundreds of km.
