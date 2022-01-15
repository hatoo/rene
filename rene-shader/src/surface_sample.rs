use spirv_std::glam::{Affine3A, Vec3A};

use crate::{math::random_in_unit_sphere, rand::DefaultRng, Vertex};

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SurfaceSample {
    t: u32,
    index_offset: u32,
    primitive_count: u32,
    matrix: Affine3A,
}

impl SurfaceSample {
    pub fn new_triangle(index_offset: u32, primitive_count: u32, matrix: Affine3A) -> Self {
        SurfaceSample {
            t: 0,
            index_offset,
            primitive_count,
            matrix,
        }
    }

    pub fn new_sphere(matrix: Affine3A) -> Self {
        SurfaceSample {
            t: 1,
            index_offset: 0,
            primitive_count: 0,
            matrix,
        }
    }

    pub fn sample(&self, indices: &[u32], vertices: &[Vertex], rng: &mut DefaultRng) -> Vec3A {
        match self.t {
            0 => {
                let p = rng.next_u32() % self.primitive_count;

                let v0 = vertices[indices[(self.index_offset + 3 * p + 0) as usize] as usize];
                let v1 = vertices[indices[(self.index_offset + 3 * p + 1) as usize] as usize];
                let v2 = vertices[indices[(self.index_offset + 3 * p + 2) as usize] as usize];

                let r = rng.next_f32();
                let s = rng.next_f32();

                let (r, s) = if r + s > 1.0 {
                    (1.0 - r, 1.0 - s)
                } else {
                    (r, s)
                };

                let pos = v0.position * (1.0 - r - s) + v1.position * r + v2.position * s;

                self.matrix.transform_point3a(pos)
            }
            _ => {
                let v = random_in_unit_sphere(rng).normalize();
                self.matrix.transform_point3a(v)
            }
        }
    }
}
