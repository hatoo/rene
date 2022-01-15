use core::f32;

use spirv_std::glam::{vec3a, Affine3A, Vec3A};

use crate::rand::DefaultRng;

#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct AABB {
    pub min: Vec3A,
    pub max: Vec3A,
}

impl AABB {
    pub fn sample(&self, rng: &mut DefaultRng) -> Vec3A {
        vec3a(
            rng.next_f32_range(self.min.x, self.max.x),
            rng.next_f32_range(self.min.y, self.max.y),
            rng.next_f32_range(self.min.z, self.max.z),
        )
    }

    #[cfg(not(target_arch = "spirv"))]
    pub fn transform(&self, affine3: &Affine3A) -> Self {
        let vs = [
            vec3a(self.min.x, self.min.y, self.min.z),
            vec3a(self.max.x, self.min.y, self.min.z),
            vec3a(self.min.x, self.max.y, self.min.z),
            vec3a(self.max.x, self.max.y, self.min.z),
            vec3a(self.min.x, self.min.y, self.max.z),
            vec3a(self.max.x, self.min.y, self.max.z),
            vec3a(self.min.x, self.max.y, self.max.z),
            vec3a(self.max.x, self.max.y, self.max.z),
        ];

        let mut result = Self {
            min: vec3a(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: vec3a(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        };

        for v in vs {
            result.merge(affine3.transform_point3a(v));
        }

        result
    }

    #[cfg(not(target_arch = "spirv"))]
    pub fn merge(&mut self, v: Vec3A) {
        self.min = vec3a(
            self.min.x.min(v.x),
            self.min.y.min(v.y),
            self.min.z.min(v.z),
        );
        self.max = vec3a(
            self.max.x.max(v.x),
            self.max.y.max(v.y),
            self.max.z.max(v.z),
        );
    }
}
