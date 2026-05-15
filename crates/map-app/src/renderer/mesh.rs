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
mod tests {
    use super::*;

    #[test]
    fn box_mesh_has_correct_vertex_count() {
        let m = box_mesh([1.0, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(m.vertices.len(), 24);
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
