use spirv_std::{
    arch::IndexUnchecked,
    glam::{uvec2, Affine3A, UVec2, Vec3A},
};

use crate::{math::random_in_unit_sphere, rand::DefaultRng, Vertex};

pub trait SurfaceSample {
    fn primitive_count(&self) -> u32;
    fn sample(&self, indices: &[u32], vertices: &[Vertex], rng: &mut DefaultRng) -> Vec3A;
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum SurfaceType {
    Triangle,
    Sphere,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct EnumSurfaceSample {
    t: SurfaceType,
    data: EnumSurfaceSampleData,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
struct EnumSurfaceSampleData {
    u0: UVec2,
    matrix: Affine3A,
}

struct Triangle<'a> {
    data: &'a EnumSurfaceSampleData,
}

struct Sphere<'a> {
    data: &'a EnumSurfaceSampleData,
}

impl<'a> Triangle<'a> {
    pub fn new_data(
        index_offset: u32,
        primitive_count: u32,
        matrix: Affine3A,
    ) -> EnumSurfaceSampleData {
        EnumSurfaceSampleData {
            u0: uvec2(index_offset, primitive_count),
            matrix,
        }
    }

    fn index_offset(&self) -> u32 {
        self.data.u0.x
    }
}

impl<'a> Sphere<'a> {
    pub fn new_data(matrix: Affine3A) -> EnumSurfaceSampleData {
        EnumSurfaceSampleData {
            u0: UVec2::ZERO,
            matrix,
        }
    }
}

impl<'a> SurfaceSample for Triangle<'a> {
    fn primitive_count(&self) -> u32 {
        self.data.u0.y
    }

    fn sample(&self, indices: &[u32], vertices: &[Vertex], rng: &mut DefaultRng) -> Vec3A {
        let p = rng.next_u32() % self.primitive_count();

        let v0 = unsafe {
            vertices.index_unchecked(
                *indices.index_unchecked((self.index_offset() + 3 * p) as usize) as usize,
            )
        };
        let v1 = unsafe {
            vertices.index_unchecked(
                *indices.index_unchecked((self.index_offset() + 3 * p + 1) as usize) as usize,
            )
        };
        let v2 = unsafe {
            vertices.index_unchecked(
                *indices.index_unchecked((self.index_offset() + 3 * p + 2) as usize) as usize,
            )
        };

        let r = rng.next_f32();
        let s = rng.next_f32();

        let (r, s) = if r + s > 1.0 {
            (1.0 - r, 1.0 - s)
        } else {
            (r, s)
        };

        let pos = v0.position * (1.0 - r - s) + v1.position * r + v2.position * s;

        self.data.matrix.transform_point3a(pos)
    }
}

impl<'a> SurfaceSample for Sphere<'a> {
    fn primitive_count(&self) -> u32 {
        1
    }

    fn sample(&self, _indices: &[u32], _vertices: &[Vertex], rng: &mut DefaultRng) -> Vec3A {
        let v = random_in_unit_sphere(rng).normalize();
        self.data.matrix.transform_point3a(v)
    }
}

impl EnumSurfaceSample {
    pub fn new_triangle(index_offset: u32, primitive_count: u32, matrix: Affine3A) -> Self {
        EnumSurfaceSample {
            t: SurfaceType::Triangle,
            data: Triangle::new_data(index_offset, primitive_count, matrix),
        }
    }

    pub fn new_sphere(matrix: Affine3A) -> Self {
        EnumSurfaceSample {
            t: SurfaceType::Sphere,
            data: Sphere::new_data(matrix),
        }
    }
}

impl SurfaceSample for EnumSurfaceSample {
    fn primitive_count(&self) -> u32 {
        match self.t {
            SurfaceType::Triangle => Triangle { data: &self.data }.primitive_count(),
            SurfaceType::Sphere => Sphere { data: &self.data }.primitive_count(),
        }
    }

    fn sample(&self, indices: &[u32], vertices: &[Vertex], rng: &mut DefaultRng) -> Vec3A {
        match self.t {
            SurfaceType::Triangle => Triangle { data: &self.data }.sample(indices, vertices, rng),
            SurfaceType::Sphere => Sphere { data: &self.data }.sample(indices, vertices, rng),
        }
    }
}
