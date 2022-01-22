use core::f32::consts::PI;
#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{vec3a, vec4, Vec3A, Vec4, Vec4Swizzles},
};

use crate::{
    math::{random_in_unit_sphere, IsNearZero},
    rand::DefaultRng,
};

pub struct SampledF {
    pub wi: Vec3A,
    pub f: Vec3A,
    pub pdf: f32,
}

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

#[repr(transparent)]
struct Lambertian<'a> {
    data: &'a EnumBxdfData,
}

#[repr(transparent)]
struct Dielectric<'a> {
    data: &'a EnumBxdfData,
}

impl<'a> Lambertian<'a> {
    fn albedo(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }
}

impl<'a> Bxdf for Lambertian<'a> {
    fn f(&self, _wo: Vec3A, _wi: Vec3A) -> Vec3A {
        self.albedo() / PI
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        _front_face: bool,
        rng: &mut DefaultRng,
    ) -> SampledF {
        let scatter_direction = normal + random_in_unit_sphere(rng).normalize();

        let scatter_direction = if scatter_direction.is_near_zero() {
            normal
        } else {
            scatter_direction
        };

        let wi = scatter_direction.normalize();
        let pdf = (normal.dot(wi) / PI).max(0.0);

        SampledF {
            wi,
            f: self.f(wo, wi),
            pdf,
        }
    }

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32 {
        wi.dot(normal).abs()
    }
}

fn reflect(v: Vec3A, n: Vec3A) -> Vec3A {
    v - 2.0 * v.dot(n) * n
}

fn refract(uv: Vec3A, n: Vec3A, etai_over_etat: f32) -> Vec3A {
    let cos_theta = (-uv).dot(n).min(1.0);
    let r_out_perp = etai_over_etat * (uv + cos_theta * n);
    let r_out_parallel = -(1.0 - r_out_perp.length_squared()).abs().sqrt() * n;
    r_out_perp + r_out_parallel
}

fn reflectance(cosine: f32, ref_idx: f32) -> f32 {
    let r0 = (1.0 - ref_idx) / (1.0 + ref_idx);
    let r0 = r0 * r0;
    r0 + (1.0 - r0) * (1.0 - cosine).powf(5.0)
}

impl<'a> Dielectric<'a> {
    fn ir(&self) -> f32 {
        self.data.v0.x
    }
}

impl<'a> Bxdf for Dielectric<'a> {
    fn f(&self, _wo: Vec3A, _wi: Vec3A) -> Vec3A {
        Vec3A::ZERO
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        front_face: bool,
        rng: &mut DefaultRng,
    ) -> SampledF {
        let refraction_ratio = if front_face {
            1.0 / self.ir()
        } else {
            self.ir()
        };

        let unit_direction = -wo;
        let cos_theta = (-unit_direction).dot(normal).min(1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let cannot_refract = refraction_ratio * sin_theta > 1.0;

        let f = reflectance(cos_theta, refraction_ratio);

        if cannot_refract || f > rng.next_f32() {
            let wi = reflect(unit_direction, normal);

            SampledF {
                wi,
                f: vec3a(1.0, 1.0, 1.0) * f / normal.dot(wi).abs(),
                pdf: f,
            }
        } else {
            let wi = refract(unit_direction, normal, refraction_ratio);

            SampledF {
                wi,
                f: vec3a(1.0, 1.0, 1.0) * (1.0 - f) / normal.dot(wi).abs(),
                pdf: 1.0 - f,
            }
        }
    }

    fn pdf(&self, _wi: Vec3A, _normal: Vec3A) -> f32 {
        0.0
    }
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
            data: EnumBxdfData {
                v0: albedo.extend(0.0),
            },
        }
    }

    pub fn new_dielectric(ir: f32) -> Self {
        Self {
            t: BxdfType::Dielectric,
            data: EnumBxdfData {
                v0: vec4(ir, 0.0, 0.0, 0.0),
            },
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
