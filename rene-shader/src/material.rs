#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    glam::{uvec4, vec4, UVec4, Vec2, Vec3A, Vec4},
    RuntimeArray,
};

use crate::{
    reflection::{Bsdf, EnumBxdf},
    texture::EnumTexture,
    InputImage,
};

pub struct SampledF {
    pub wi: Vec3A,
    pub f: Vec3A,
    pub pdf: f32,
}

pub trait Material {
    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    );

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

impl<'a> Material for Lambertian<'a> {
    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        textures[self.data.u0.x as usize].color(textures, images, uv)
    }

    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        bsdf.add(EnumBxdf::new_lambertian(self.albedo(uv, textures, images)));
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

    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        _uv: Vec2,
        _textures: &[EnumTexture],
        _images: &RuntimeArray<InputImage>,
    ) {
        bsdf.add(EnumBxdf::new_dielectric(self.ir()))
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

    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        match self.t {
            MaterialType::Lambertian => {
                Lambertian { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
            MaterialType::Dielectric => {
                Dielectric { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
        }
    }
}
