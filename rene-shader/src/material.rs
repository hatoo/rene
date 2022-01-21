use core::f32::consts::PI;
#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    glam::{uvec4, vec3a, vec4, UVec4, Vec2, Vec3A, Vec4},
    RuntimeArray,
};

use crate::{
    math::{random_in_unit_sphere, IsNearZero},
    rand::DefaultRng,
    texture::EnumTexture,
    InputImage,
};

pub struct SampledF {
    pub wi: Vec3A,
    pub f: Vec3A,
    pub pdf: f32,
}

pub trait Material {
    fn f(
        &self,
        wo: Vec3A,
        wi: Vec3A,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A;

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        rng: &mut DefaultRng,
    ) -> SampledF;

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32;

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumMaterialData {
    u0: UVec4,
    v0: Vec4,
}

#[repr(u32)]
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
enum MaterialType {
    Lambertian,
    Dielectric,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumMaterial {
    t: MaterialType,
    data: EnumMaterialData,
}

#[repr(transparent)]
struct Lambertian<'a> {
    data: &'a EnumMaterialData,
}

/*
#[repr(transparent)]
struct Metal<'a> {
    data: &'a EnumMaterialData,
}
*/

#[repr(transparent)]
struct Dielectric<'a> {
    data: &'a EnumMaterialData,
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

impl<'a> Material for Lambertian<'a> {
    fn f(
        &self,
        _wo: Vec3A,
        _wi: Vec3A,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        self.albedo(uv, textures, images) / PI
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        mut normal: Vec3A,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        rng: &mut DefaultRng,
    ) -> SampledF {
        if normal.dot(wo) < 0.0 {
            normal = -normal;
        }

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
            f: self.f(wo, wi, uv, textures, images),
            pdf,
        }
    }

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        textures[self.data.u0.x as usize].color(textures, images, uv)
    }

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32 {
        wi.dot(normal).abs()
    }
}

/*

impl<'a> Metal<'a> {
    fn fuzz(&self) -> f32 {
        self.data.v0.w
    }
}

impl<'a> Material for Metal<'a> {
    fn scatter(
        &self,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        ray: &Ray,
        ray_payload: &RayPayload,
        rng: &mut DefaultRng,
        scatter: &mut Scatter,
    ) -> bool {
        let reflected = reflect(ray.direction.normalize(), ray_payload.normal);
        let scatterd = reflected + self.fuzz() * random_in_unit_sphere(rng);
        if scatterd.dot(ray_payload.normal) > 0.0 {
            *scatter = Scatter {
                color: self.albedo(textures, images, ray_payload.uv),
                ray: Ray {
                    origin: ray_payload.position,
                    direction: scatterd,
                },
            };
            true
        } else {
            false
        }
    }

    fn albedo(
        &self,
        _textures: &[EnumTexture],
        _images: &RuntimeArray<InputImage>,
        _uv: Vec2,
    ) -> Vec3A {
        self.data.v0.xyz().into()
    }

    fn brdf(&self, _v0: Vec3A, _v1: Vec3A) -> f32 {
        // TODO
        1.0
    }
}
*/

impl<'a> Dielectric<'a> {
    fn ir(&self) -> f32 {
        self.data.v0.x
    }
}

impl<'a> Material for Dielectric<'a> {
    fn albedo(
        &self,
        _uv: Vec2,
        _textures: &[EnumTexture],
        _images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        Vec3A::ZERO
    }

    fn f(
        &self,
        _wo: Vec3A,
        _wi: Vec3A,
        _uv: Vec2,
        _textures: &[EnumTexture],
        _images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        Vec3A::ZERO
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        mut normal: Vec3A,
        _uv: Vec2,
        _textures: &[EnumTexture],
        _images: &RuntimeArray<InputImage>,
        rng: &mut DefaultRng,
    ) -> SampledF {
        let front_face = normal.dot(wo) > 0.0;

        let refraction_ratio = if front_face {
            1.0 / self.ir()
        } else {
            self.ir()
        };

        if !front_face {
            normal = -normal;
        }

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

impl EnumMaterial {
    pub fn new_lambertian(albedo_index: u32) -> Self {
        Self {
            t: MaterialType::Lambertian,
            data: EnumMaterialData {
                u0: uvec4(albedo_index, 0, 0, 0),
                v0: Vec4::ZERO,
            },
        }
    }

    /*
    pub fn new_metal(albedo: Vec3A, fuzz: f32) -> Self {
        Self {
            t: 1,
            data: EnumMaterialData {
                u0: UVec4::ZERO,
                v0: vec4(albedo.x, albedo.y, albedo.z, fuzz),
            },
        }
    }
    */

    pub fn new_dielectric(ir: f32) -> Self {
        Self {
            t: MaterialType::Dielectric,
            data: EnumMaterialData {
                u0: UVec4::ZERO,
                v0: vec4(ir, 0.0, 0.0, 0.0),
            },
        }
    }
}

impl Material for EnumMaterial {
    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        match self.t {
            MaterialType::Lambertian => {
                Lambertian { data: &self.data }.albedo(uv, textures, images)
            }
            MaterialType::Dielectric => {
                Dielectric { data: &self.data }.albedo(uv, textures, images)
            }
        }
    }

    fn f(
        &self,
        wo: Vec3A,
        wi: Vec3A,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        match self.t {
            MaterialType::Lambertian => {
                Lambertian { data: &self.data }.f(wo, wi, uv, textures, images)
            }
            MaterialType::Dielectric => {
                Dielectric { data: &self.data }.f(wo, wi, uv, textures, images)
            }
        }
    }

    fn sample_f(
        &self,
        wo: Vec3A,
        normal: Vec3A,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        rng: &mut DefaultRng,
    ) -> SampledF {
        match self.t {
            MaterialType::Lambertian => {
                Lambertian { data: &self.data }.sample_f(wo, normal, uv, textures, images, rng)
            }
            MaterialType::Dielectric => {
                Dielectric { data: &self.data }.sample_f(wo, normal, uv, textures, images, rng)
            }
        }
    }

    fn pdf(&self, wi: Vec3A, normal: Vec3A) -> f32 {
        match self.t {
            MaterialType::Lambertian => Lambertian { data: &self.data }.pdf(wi, normal),
            MaterialType::Dielectric => Dielectric { data: &self.data }.pdf(wi, normal),
        }
    }
}
