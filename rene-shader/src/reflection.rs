#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{vec3a, Vec3A, Vec4},
};

use crate::rand::DefaultRng;

pub struct SampledF {
    pub wi: Vec3A,
    pub f: Vec3A,
    pub pdf: f32,
}

mod bxdf;
mod onb;

use bxdf::{Dielectric, Lambertian};

pub trait Bxdf {
    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A;

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        front_face: bool,
        rng: &mut DefaultRng,
    ) -> SampledF;

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumBxdfData {
    v0: Vec4,
}

#[repr(u32)]
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
enum BxdfType {
    Lambertian,
    Dielectric,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumBxdf {
    t: BxdfType,
    data: EnumBxdfData,
}

impl Bxdf for EnumBxdf {
    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A {
        match self.t {
            BxdfType::Lambertian => Lambertian { data: &self.data }.f(wo, wi),
            BxdfType::Dielectric => Dielectric { data: &self.data }.f(wo, wi),
        }
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        front_face: bool,
        rng: &mut DefaultRng,
    ) -> SampledF {
        match self.t {
            BxdfType::Lambertian => {
                Lambertian { data: &self.data }.sample_f(wo, normal, front_face, rng)
            }
            BxdfType::Dielectric => {
                Dielectric { data: &self.data }.sample_f(wo, normal, front_face, rng)
            }
        }
    }

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32 {
        match self.t {
            BxdfType::Lambertian => Lambertian { data: &self.data }.pdf(wi, normal),
            BxdfType::Dielectric => Dielectric { data: &self.data }.pdf(wi, normal),
        }
    }
}

impl EnumBxdf {
    pub fn new_lambertian(albedo: Vec3A) -> Self {
        Self {
            t: BxdfType::Lambertian,
            data: Lambertian::new(albedo),
        }
    }

    pub fn new_dielectric(ir: f32) -> Self {
        Self {
            t: BxdfType::Dielectric,
            data: Dielectric::new(ir),
        }
    }
}

const BXDF_LEN: usize = 8;

pub struct Bsdf {
    len: usize,
    bxdfs: [EnumBxdf; BXDF_LEN],
}

impl Bsdf {
    pub fn new() -> Self {
        Self {
            len: 0,
            bxdfs: [EnumBxdf::new_lambertian(vec3a(0.0, 0.0, 0.0)); BXDF_LEN],
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn add(&mut self, bxdf: EnumBxdf) {
        self.bxdfs[self.len] = bxdf;
        self.len += 1;
    }
}

impl Bxdf for Bsdf {
    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A {
        let mut f = Vec3A::ZERO;

        for i in 0..self.len {
            let bxdf = *unsafe { self.bxdfs.index_unchecked(i) };
            f += bxdf.f(wo, wi);
        }

        f
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        front_face: bool,
        rng: &mut DefaultRng,
    ) -> SampledF {
        if self.len == 0 {
            SampledF {
                wi: Vec3A::ZERO,
                f: Vec3A::ZERO,
                pdf: 0.0,
            }
        } else {
            let index = rng.next_u32() as usize % self.len;
            let bxdf = *unsafe { self.bxdfs.index_unchecked(index) };
            let mut sampled_f = bxdf.sample_f(wo, normal, front_face, rng);

            sampled_f.pdf /= self.len as f32;
            sampled_f
        }
    }

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32 {
        if self.len == 0 {
            return 0.0;
        }

        let mut pdf = 0.0;

        for i in 0..self.len {
            let bxdf = *unsafe { self.bxdfs.index_unchecked(i) };
            pdf += bxdf.pdf(wi, normal);
        }

        pdf / self.len as f32
    }
}
