use spirv_std::glam::{Vec3A, Vec4, Vec4Swizzles};
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
    fn sample(&self, ray: &Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium;
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
}

impl<'a> Medium for Homogeous<'a> {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A {
        (-self.sigma_t() * ray.direction.length() * t_max).exp()
    }

    fn sample(&self, ray: &Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium {
        let channel = rng.next_u32() % 3;
        let dist = -(1.0 - rng.next_f32()).ln() / self.sigma_t()[channel as usize];
        let t = (dist / ray.direction.length()).min(t_max);
        let sampled = t < t_max;

        let tr = (-self.sigma_t() * t * ray.direction.length()).exp();
        let density = if sampled { self.sigma_t() * tr } else { tr };
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
            MediumType::Vaccum => Vec3A::ZERO,
            MediumType::Homogeous => Homogeous { data: &self.data }.tr(ray, t_max),
        }
    }

    fn sample(&self, ray: &Ray, t_max: f32, rng: &mut DefaultRng) -> SampledMedium {
        match self.t {
            MediumType::Vaccum => Default::default(),
            MediumType::Homogeous => Homogeous { data: &self.data }.sample(ray, t_max, rng),
        }
    }
}
