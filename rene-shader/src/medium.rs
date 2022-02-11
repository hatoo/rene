use core::f32::consts::PI;

use spirv_std::glam::{vec3a, Vec3A, Vec4, Vec4Swizzles};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::{rand::DefaultRng, Ray};

#[derive(Default)]
pub struct SampledMedium {
    pub sampled: bool,
    pub position: Vec3A,
    pub wo: Vec3A,
    pub tr: Vec3A,
}

pub trait Medium {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A;
    fn sample(&self, ray: Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium;
    fn phase(&self, wo: Vec3A, wi: Vec3A) -> f32;
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub enum MediumType {
    Vaccum,
    Homogeous,
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

struct Homogeous<'a> {
    data: &'a EnumMediumData,
}

impl<'a> Homogeous<'a> {
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

impl<'a> Medium for Homogeous<'a> {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A {
        (-self.sigma_t() * ray.direction.length() * t_max).exp()
    }

    fn sample(&self, ray: Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium {
        let channel = rng.next_u32() % 3;
        let sigma_t = self.sigma_t();
        let dist =
            -(1.0 - rng.next_f32()).ln() / [sigma_t.x, sigma_t.y, sigma_t.z][channel as usize];
        let t = (dist / ray.direction.length()).min(t_max);
        let sampled = t < t_max;

        let tr = (-sigma_t * t * ray.direction.length()).exp();
        let density = if sampled { sigma_t * tr } else { tr };
        let pdf = (density.x + density.y + density.z) / 3.0;
        let pdf = if pdf == 0.0 { 1.0 } else { pdf };

        SampledMedium {
            sampled,
            position: ray.origin + t * ray.direction,
            wo: -ray.direction,
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

    pub fn new_homogeus(sigma_a: Vec3A, sigma_s: Vec3A, g: f32) -> Self {
        Self {
            t: MediumType::Homogeous,
            data: Homogeous::new_data(sigma_a, sigma_s, g),
        }
    }
}

impl Medium for EnumMedium {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A {
        match self.t {
            MediumType::Vaccum => vec3a(1.0, 1.0, 1.0),
            MediumType::Homogeous => Homogeous { data: &self.data }.tr(ray, t_max),
        }
    }

    fn sample(&self, ray: Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium {
        match self.t {
            MediumType::Vaccum => Default::default(),
            MediumType::Homogeous => Homogeous { data: &self.data }.sample(ray, t_max, rng),
        }
    }

    fn phase(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        match self.t {
            MediumType::Vaccum => 0.0,
            MediumType::Homogeous => Homogeous { data: &self.data }.phase(wo, wi),
        }
    }
}
