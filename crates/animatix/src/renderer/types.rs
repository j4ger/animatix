use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
}

pub const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-1.0, -1.0],
    },
    Vertex {
        position: [1.0, -1.0],
    },
    Vertex {
        position: [-1.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
    },
];

pub const INDICES: &[u16] = &[0, 1, 2, 1, 3, 2];

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SdfInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_rect: [f32; 4],
    pub shape_params: [f32; 4],
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub glow_radius: f32,
    pub opacity: f32,
    pub shape_type: u32,
    pub target_position: [f32; 2],
    pub target_size: [f32; 2],
    pub target_shape_params: [f32; 4],
    pub target_shape_type: u32,
    pub shape_blend: f32,
    pub _padding1: [f32; 2],
    pub morph_params: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextInstance {
    pub position: [f32; 2],
    pub scale: [f32; 2],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn update_proj(&mut self, width: f32, height: f32) {
        let left = 0.0;
        let right = width;
        let bottom = height;
        let top = 0.0;
        let near = -1.0;
        let far = 1.0;

        self.view_proj = [
            [2.0 / (right - left), 0.0, 0.0, 0.0],
            [0.0, 2.0 / (top - bottom), 0.0, 0.0],
            [0.0, 0.0, -2.0 / (far - near), 0.0],
            [
                -(right + left) / (right - left),
                -(top + bottom) / (top - bottom),
                -(far + near) / (far - near),
                1.0,
            ],
        ];
    }
}
