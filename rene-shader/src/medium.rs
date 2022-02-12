use core::f32::consts::PI;

#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{vec3a, Vec3A, Vec4, Vec4Swizzles},
};

use crate::{math::coordinate_system, rand::DefaultRng, Ray};

fn spherical_direction(
    sin_theta: f32,
    cos_theta: f32,
    phi: f32,
    x: Vec3A,
    y: Vec3A,
    z: Vec3A,
) -> Vec3A {
    sin_theta * phi.cos() * x + sin_theta * phi.sin() * y + cos_theta * z
}

pub struct SampledMedium {
    pub sampled: bool,
    pub position: Vec3A,
    pub tr: Vec3A,
}

impl Default for SampledMedium {
    fn default() -> Self {
        SampledMedium {
            sampled: false,
            position: Vec3A::ZERO,
            tr: vec3a(1.0, 1.0, 1.0),
        }
    }
}

pub trait Medium {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A;
    fn sample(&self, ray: Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium;
    fn sample_p(&self, wo: Vec3A, rng: &mut DefaultRng) -> Vec3A;
    fn phase(&self, wo: Vec3A, wi: Vec3A) -> f32;
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub enum MediumType {
    Vaccum,
    Homogeneous,
}

impl Default for MediumType {
    fn default() -> Self {
        Self::Vaccum
    }
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumMediumData {
    v0: Vec4,
    v1: Vec4,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumMedium {
    t: MediumType,
    data: EnumMediumData,
}

struct Homogeneous<'a> {
    data: &'a EnumMediumData,
}

impl<'a> Homogeneous<'a> {
    fn new_data(sigma_a: Vec3A, sigma_s: Vec3A, g: f32) -> EnumMediumData {
        EnumMediumData {
            v0: sigma_a.extend(g),
            v1: sigma_s.extend(0.0),
        }
    }

    fn sigma_a(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }

    fn sigma_s(&self) -> Vec3A {
        self.data.v1.xyz().into()
    }

    fn sigma_t(&self) -> Vec3A {
        self.sigma_a() + self.sigma_s()
    }

    fn g(&self) -> f32 {
        self.data.v0.w
    }
}

impl<'a> Medium for Homogeneous<'a> {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A {
        (-self.sigma_t() * ray.direction.length() * t_max).exp()
    }

    fn sample(&self, ray: Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium {
        let channel = rng.next_u32() % 3;
        let sigma_t = self.sigma_t();
        let dist = -(1.0 - rng.next_f32()).ln()
            / *unsafe { [sigma_t.x, sigma_t.y, sigma_t.z].index_unchecked(channel as usize) };
        let t = dist / ray.direction.length();
        let sampled = t < t_max;
        let t = t.min(t_max);

        let tr = (-sigma_t * t * ray.direction.length()).exp();
        let density = if sampled { sigma_t * tr } else { tr };
        let pdf = (density.x + density.y + density.z) / 3.0;
        let pdf = if pdf == 0.0 { 1.0 } else { pdf };

        SampledMedium {
            sampled,
            position: ray.origin + t * ray.direction,
            tr: if sampled {
                tr * self.sigma_s() / pdf
            } else {
                tr / pdf
            },
        }
    }

    fn phase(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        let cos_theta = wo.dot(wi);
        let g = self.g();
        let denom = 1.0 + g * g + 2.0 * g * cos_theta;
        1.0 / (4.0 * PI) * (1.0 - g * g) / (denom * denom.sqrt())
    }

    fn sample_p(&self, wo: Vec3A, rng: &mut DefaultRng) -> Vec3A {
        let u0 = rng.next_f32();
        let u1 = rng.next_f32();
        let g = self.g();
        let cos_theta = if g.abs() < 1e-3 {
            1.0 - 2.0 * u0
        } else {
            let sqr_term = (1.0 - g * g) / (1.0 + g - 2.0 * g * u0);
            -(1.0 + g * g - sqr_term * sqr_term) / (2.0 * g)
        };

        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let phi = 2.0 * PI * u1;
        let (v1, v2) = coordinate_system(wo);
        spherical_direction(sin_theta, cos_theta, phi, v1, v2, wo)
    }
}

impl EnumMedium {
    pub fn is_vaccum(&self) -> bool {
        self.t == MediumType::Vaccum
    }

    pub fn new_vaccum() -> Self {
        Self {
            t: MediumType::Vaccum,
            data: Default::default(),
        }
    }

    pub fn new_homogeneous(sigma_a: Vec3A, sigma_s: Vec3A, g: f32) -> Self {
        Self {
            t: MediumType::Homogeneous,
            data: Homogeneous::new_data(sigma_a, sigma_s, g),
        }
    }
}

impl Medium for EnumMedium {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A {
        match self.t {
            MediumType::Vaccum => vec3a(1.0, 1.0, 1.0),
            MediumType::Homogeneous => Homogeneous { data: &self.data }.tr(ray, t_max),
        }
    }

    fn sample(&self, ray: Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium {
        match self.t {
            MediumType::Vaccum => Default::default(),
            MediumType::Homogeneous => Homogeneous { data: &self.data }.sample(ray, t_max, rng),
        }
    }

    fn phase(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        match self.t {
            MediumType::Vaccum => 0.0,
            MediumType::Homogeneous => Homogeneous { data: &self.data }.phase(wo, wi),
        }
    }

    fn sample_p(&self, wo: Vec3A, rng: &mut DefaultRng) -> Vec3A {
        match self.t {
            MediumType::Vaccum => Vec3A::ZERO,
            MediumType::Homogeneous => Homogeneous { data: &self.data }.sample_p(wo, rng),
        }
    }
}
