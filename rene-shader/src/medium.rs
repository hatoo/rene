use spirv_std::glam::{Vec3A, Vec4, Vec4Swizzles};

use crate::Ray;

pub trait Medium {
    fn tr(&self, ray: Ray, t_max: f32) -> Vec3A;
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
}
