use spirv_std::glam::{vec3a, Vec3A};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::asm::f32_clamp;

pub struct Onb {
    pub u: Vec3A,
    pub v: Vec3A,
    pub w: Vec3A,
}

impl Onb {
    pub fn from_w(w: Vec3A) -> Self {
        let a = if w.x.abs() > 0.9 {
            vec3a(0.0, 1.0, 0.0)
        } else {
            vec3a(1.0, 0.0, 0.0)
        };
        let v = w.cross(a).normalize();
        let u = w.cross(v);

        Self { u, v, w }
    }

    pub fn local_to_world(&self, v: Vec3A) -> Vec3A {
        v.x * self.u + v.y * self.v + v.z * self.w
    }

    pub fn world_to_local(&self, v: Vec3A) -> Vec3A {
        vec3a(v.dot(self.u), v.dot(self.v), v.dot(self.w))
    }

    pub fn local_cos_theta(w: Vec3A) -> f32 {
        w.z
    }

    pub fn local_cos2_theta(w: Vec3A) -> f32 {
        w.z * w.z
    }

    pub fn local_abs_cos_theta(w: Vec3A) -> f32 {
        w.z.abs()
    }

    pub fn local_sin2_theta(w: Vec3A) -> f32 {
        (1.0 - Self::local_cos2_theta(w)).max(0.0)
    }

    pub fn local_sin_theata(w: Vec3A) -> f32 {
        Self::local_sin2_theta(w).sqrt()
    }

    pub fn local_tan_theta(w: Vec3A) -> f32 {
        Self::local_sin_theata(w) / Self::local_cos_theta(w)
    }

    pub fn local_tan2_theta(w: Vec3A) -> f32 {
        Self::local_sin2_theta(w) / Self::local_cos2_theta(w)
    }

    pub fn local_cos_phi(w: Vec3A) -> f32 {
        let sin_theta = Self::local_sin_theata(w);
        if sin_theta == 0.0 {
            1.0
        } else {
            f32_clamp(w.x / sin_theta, -1.0, 1.0)
        }
    }

    pub fn local_sin_phi(w: Vec3A) -> f32 {
        let sin_theta = Self::local_sin_theata(w);
        if sin_theta == 0.0 {
            0.0
        } else {
            f32_clamp(w.y / sin_theta, -1.0, 1.0)
        }
    }

    pub fn local_cos2_phi(w: Vec3A) -> f32 {
        let cos_phi = Self::local_cos_phi(w);
        cos_phi * cos_phi
    }

    pub fn local_sin2_phi(w: Vec3A) -> f32 {
        let sin_phi = Self::local_sin_phi(w);
        sin_phi * sin_phi
    }

    pub fn local_same_hemisphere(v1: Vec3A, v2: Vec3A) -> bool {
        v1.z * v2.z > 0.0
    }
}
