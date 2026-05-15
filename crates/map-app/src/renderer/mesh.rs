pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}
