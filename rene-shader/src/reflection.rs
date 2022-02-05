use core::ops::BitOr;

#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{vec3a, Vec3A, Vec4},
};

use crate::rand::DefaultRng;

#[derive(Default)]
pub struct SampledF {
    pub wi: Vec3A,
    pub f: Vec3A,
    pub pdf: f32,
}

mod bxdf;
pub mod fresnel;
pub mod microfacet;
pub mod onb;

use bxdf::{FresnelSpecular, LambertianReflection};

use self::{
    bxdf::{FresnelBlend, MicrofacetReflection},
    fresnel::EnumFresnel,
    microfacet::EnumMicrofacetDistribution,
    onb::Onb,
};

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct Packed4<T> {
    pub t: T,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl<T> Packed4<T> {
    pub fn new(t: T, v: Vec3A) -> Self {
        Self {
            t,
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }

    pub fn set_xyz(&mut self, v: Vec3A) {
        self.x = v.x;
        self.y = v.y;
        self.z = v.z;
    }

    pub fn xyz(&self) -> Vec3A {
        vec3a(self.x, self.y, self.z)
    }
}

#[derive(Clone, Copy)]
pub struct BxdfKind(u32);

impl BxdfKind {
    pub const REFLECTION: Self = Self(1 << 0);
    pub const TRANSMISSION: Self = Self(1 << 1);
    pub const DIFFUSE: Self = Self(1 << 2);

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
pub struct EnumBxdfData {
    v0: Packed4<BxdfType>,
    v1: Vec4,
    microfacet_distribution: EnumMicrofacetDistribution,
    fresnel: EnumFresnel,
}

#[repr(u32)]
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
enum BxdfType {
    LambertianReflection,
    FresnelSpecular,
    FresnelBlend,
    MicroFacetReflection,
}

impl Default for BxdfType {
    fn default() -> Self {
        Self::LambertianReflection
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(transparent)]
pub struct EnumBxdf {
    data: EnumBxdfData,
}

impl Bxdf for EnumBxdf {
    fn kind(&self) -> BxdfKind {
        match self.t() {
            BxdfType::LambertianReflection => LambertianReflection { data: &self.data }.kind(),
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.kind(),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.kind(),
            BxdfType::MicroFacetReflection => MicrofacetReflection { data: &self.data }.kind(),
        }
    }

    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A {
        match self.t() {
            BxdfType::LambertianReflection => LambertianReflection { data: &self.data }.f(wo, wi),
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.f(wo, wi),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.f(wo, wi),
            BxdfType::MicroFacetReflection => MicrofacetReflection { data: &self.data }.f(wo, wi),
        }
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        match self.t() {
            BxdfType::LambertianReflection => {
                LambertianReflection { data: &self.data }.sample_f(wo, rng)
            }
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.sample_f(wo, rng),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.sample_f(wo, rng),
            BxdfType::MicroFacetReflection => {
                MicrofacetReflection { data: &self.data }.sample_f(wo, rng)
            }
        }
    }

    fn pdf(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        match self.t() {
            BxdfType::LambertianReflection => LambertianReflection { data: &self.data }.pdf(wo, wi),
            BxdfType::FresnelSpecular => FresnelSpecular { data: &self.data }.pdf(wo, wi),
            BxdfType::FresnelBlend => FresnelBlend { data: &self.data }.pdf(wo, wi),
            BxdfType::MicroFacetReflection => MicrofacetReflection { data: &self.data }.pdf(wo, wi),
        }
    }
}

impl EnumBxdf {
    fn t(&self) -> BxdfType {
        self.data.v0.t
    }

    pub fn setup_lambertian_reflection(albedo: Vec3A, bxdf: &mut EnumBxdf) {
        bxdf.data.v0.t = BxdfType::LambertianReflection;
        LambertianReflection::setup_data(albedo, &mut bxdf.data);
    }

    pub fn setup_fresnel_specular(ir: f32, bxdf: &mut EnumBxdf) {
        bxdf.data.v0.t = BxdfType::FresnelSpecular;
        FresnelSpecular::setup_data(ir, &mut bxdf.data);
    }

    pub fn setup_fresnel_blend(
        rd: Vec3A,
        rs: Vec3A,
        distribution: EnumMicrofacetDistribution,
        bxdf: &mut EnumBxdf,
    ) {
        bxdf.data.v0.t = BxdfType::FresnelBlend;
        FresnelBlend::setup_data(rd, rs, distribution, &mut bxdf.data);
    }

    pub fn setup_microfacet_reflection(
        r: Vec3A,
        microfacet_distribution: EnumMicrofacetDistribution,
        fresnel: EnumFresnel,
        bxdf: &mut EnumBxdf,
    ) {
        bxdf.data.v0.t = BxdfType::MicroFacetReflection;
        MicrofacetReflection::setup_data(r, microfacet_distribution, fresnel, &mut bxdf.data);
    }
}

const BXDF_LEN: usize = 4;

pub struct Bsdf {
    ng: Vec3A,
    onb: Onb,
    len: u32,
    bxdfs: [EnumBxdf; BXDF_LEN],
}

impl Default for Bsdf {
    fn default() -> Self {
        Self {
            ng: Vec3A::Z,
            onb: Onb::from_w(Vec3A::Z),
            len: 0,
            bxdfs: [EnumBxdf {
                data: Default::default(),
            }; BXDF_LEN],
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
        *unsafe { self.bxdfs.index_unchecked_mut(self.len as usize) } = bxdf;
        self.len += 1;
    }

    pub fn add_mut(&mut self) -> &mut EnumBxdf {
        let bxdf = unsafe { self.bxdfs.index_unchecked_mut(self.len as usize) };
        self.len += 1;
        bxdf
    }

    pub fn contains(&self, kind: BxdfKind) -> bool {
        let mut i = 0;

        while i < self.len {
            if unsafe { self.bxdfs.index_unchecked(i as usize) }
                .kind()
                .contains(kind)
            {
                return true;
            }
            i += 1;
        }

        false
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

        let mut i = 0;
        while i < self.len {
            let bxdf = unsafe { self.bxdfs.index_unchecked(i as usize) };
            if (reflect && bxdf.kind().contains(BxdfKind::REFLECTION))
                || (!reflect && bxdf.kind().contains(BxdfKind::TRANSMISSION))
            {
                f += bxdf.f(wo, wi);
            }

            i += 1;
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
            let index = rng.next_u32() as usize % self.len as usize;
            let bxdf = unsafe { self.bxdfs.index_unchecked(index) };
            let wo = self.onb.world_to_local(wo_world);
            let mut sampled_f = bxdf.sample_f(wo, rng);

            sampled_f.pdf /= self.len as f32;
            sampled_f.wi = self.onb.local_to_world(sampled_f.wi);
            sampled_f
        }
    }

    pub fn pdf(&self, wo_world: Vec3A, wi_world: Vec3A) -> f32 {
        let mut pdf = 0.0;

        let wo = self.onb.world_to_local(wo_world);
        let wi = self.onb.world_to_local(wi_world);

        let mut i = 0;
        while i < self.len {
            let bxdf = unsafe { self.bxdfs.index_unchecked(i as usize) };
            pdf += bxdf.pdf(wo, wi);
            i += 1;
        }

        pdf / self.len as f32
    }
}
