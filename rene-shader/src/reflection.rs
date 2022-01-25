use core::ops::BitOr;

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
pub mod microfacet;
pub mod onb;

use bxdf::{FresnelSpecular, LambertianReflection};

use self::{bxdf::FresnelBlend, microfacet::EnumMicrofacetDistribution, onb::Onb};

pub struct BxdfKind(u32);

impl BxdfKind {
    const REFLECTION: Self = Self(1 << 0);
    const TRANSMISSION: Self = Self(1 << 1);

    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl BitOr for BxdfKind {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

pub trait Bxdf {
    fn kind(&self) -> BxdfKind;

    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A;

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF;

    fn pdf(&self, wo: Vec3A, wi: Vec3A) -> f32;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumBxdfData {
    v0: Vec4,
    v1: Vec4,
    microfacet_distribution: EnumMicrofacetDistribution,
}

#[repr(u32)]
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
enum BxdfType {
    LambertianReflection,
    FresnelSpecular,
    FresnelBlend,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumBxdf {
    t: BxdfType,
    data: EnumBxdfData,
}

impl Bxdf for EnumBxdf {
    fn kind(&self) -> BxdfKind {
        match self.t {
            BxdfType::LambertianReflection => LambertianReflection { data: &self.data }.kind(),
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.kind(),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.kind(),
        }
    }

    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A {
        match self.t {
            BxdfType::LambertianReflection => LambertianReflection { data: &self.data }.f(wo, wi),
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.f(wo, wi),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.f(wo, wi),
        }
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        match self.t {
            BxdfType::LambertianReflection => {
                LambertianReflection { data: &self.data }.sample_f(wo, rng)
            }
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.sample_f(wo, rng),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.sample_f(wo, rng),
        }
    }

    fn pdf(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        match self.t {
            BxdfType::LambertianReflection => LambertianReflection { data: &self.data }.pdf(wo, wi),
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.pdf(wo, wi),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.pdf(wo, wi),
        }
    }
}

impl EnumBxdf {
    pub fn new_lambertian_reflection(albedo: Vec3A) -> Self {
        Self {
            t: BxdfType::LambertianReflection,
            data: LambertianReflection::new_data(albedo),
        }
    }

    pub fn new_fresnel_specular(ir: f32) -> Self {
        Self {
            t: BxdfType::FresnelSpecular,
            data: FresnelSpecular::new_data(ir),
        }
    }

    pub fn new_fresnel_blend(
        rd: Vec3A,
        rs: Vec3A,
        distribution: EnumMicrofacetDistribution,
    ) -> Self {
        Self {
            t: BxdfType::FresnelBlend,
            data: FresnelBlend::new_data(rd, rs, distribution),
        }
    }
}

const BXDF_LEN: usize = 8;

pub struct Bsdf {
    ng: Vec3A,
    onb: Onb,
    len: usize,
    bxdfs: [EnumBxdf; BXDF_LEN],
}

impl Default for Bsdf {
    fn default() -> Self {
        Self {
            ng: Vec3A::Z,
            onb: Onb::from_w(Vec3A::Z),
            len: 0,
            bxdfs: [EnumBxdf::new_lambertian_reflection(vec3a(0.0, 0.0, 0.0)); BXDF_LEN],
        }
    }
}

impl Bsdf {
    pub fn clear(&mut self, ng: Vec3A, onb: Onb) {
        self.len = 0;
        self.ng = ng;
        self.onb = onb;
    }

    pub fn add(&mut self, bxdf: EnumBxdf) {
        self.bxdfs[self.len] = bxdf;
        self.len += 1;
    }
}

impl Bsdf {
    pub fn f(&self, wo_world: Vec3A, wi_world: Vec3A) -> Vec3A {
        let wi = self.onb.world_to_local(wi_world);
        let wo = self.onb.world_to_local(wo_world);

        if wo.z == 0.0 {
            return Vec3A::ZERO;
        }

        let reflect = wi_world.dot(self.ng) * wo_world.dot(self.ng) > 0.0;

        let mut f = Vec3A::ZERO;

        for i in 0..self.len {
            let bxdf = *unsafe { self.bxdfs.index_unchecked(i) };
            if (reflect && bxdf.kind().contains(BxdfKind::REFLECTION))
                || (!reflect && bxdf.kind().contains(BxdfKind::TRANSMISSION))
            {
                f += bxdf.f(wo, wi);
            }
        }

        f
    }

    pub fn sample_f(&self, wo_world: Vec3A, rng: &mut DefaultRng) -> SampledF {
        if self.len == 0 {
            SampledF {
                wi: Vec3A::ZERO,
                f: Vec3A::ZERO,
                pdf: 0.0,
            }
        } else {
            let index = rng.next_u32() as usize % self.len;
            let bxdf = *unsafe { self.bxdfs.index_unchecked(index) };
            let wo = self.onb.world_to_local(wo_world);
            let mut sampled_f = bxdf.sample_f(wo, rng);

            sampled_f.pdf /= self.len as f32;
            sampled_f.wi = self.onb.local_to_world(sampled_f.wi);
            sampled_f
        }
    }

    pub fn pdf(&self, wo_world: Vec3A, wi_world: Vec3A) -> f32 {
        if self.len == 0 {
            return 0.0;
        }

        let mut pdf = 0.0;

        let wo = self.onb.world_to_local(wo_world);
        let wi = self.onb.world_to_local(wi_world);

        for i in 0..self.len {
            let bxdf = *unsafe { self.bxdfs.index_unchecked(i) };
            pdf += bxdf.pdf(wo, wi);
        }

        pdf / self.len as f32
    }
}
