#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    glam::{uvec4, vec4, UVec4, Vec2, Vec3A, Vec4},
    RuntimeArray,
};

use crate::{
    reflection::{microfacet::EnumMicrofacetDistribution, Bsdf, EnumBxdf},
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
    Matte,
    Glass,
    Substrate,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumMaterial {
    t: MaterialType,
    data: EnumMaterialData,
}

#[repr(transparent)]
struct Matte<'a> {
    data: &'a EnumMaterialData,
}

#[repr(transparent)]
struct Substrate<'a> {
    data: &'a EnumMaterialData,
}

/*
#[repr(transparent)]
struct Metal<'a> {
    data: &'a EnumMaterialData,
}
*/

#[repr(transparent)]
struct Glass<'a> {
    data: &'a EnumMaterialData,
}

impl<'a> Material for Matte<'a> {
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
        bsdf.add(EnumBxdf::new_lambertian_reflection(
            self.albedo(uv, textures, images),
        ));
    }
}

impl<'a> Substrate<'a> {
    fn d(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        textures[self.data.u0.x as usize].color(textures, images, uv)
    }

    fn s(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        textures[self.data.u0.y as usize].color(textures, images, uv)
    }

    fn rough_u(&self) -> f32 {
        self.data.v0.x
    }

    fn rough_v(&self) -> f32 {
        self.data.v0.y
    }
}

impl<'a> Material for Substrate<'a> {
    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        let d = self.d(uv, textures, images);
        let s = self.s(uv, textures, images);
        let rough_u = self.rough_u();
        let rough_v = self.rough_v();

        bsdf.add(EnumBxdf::new_fresnel_blend(
            d,
            s,
            EnumMicrofacetDistribution::new_trowbridge_reitz(rough_u, rough_v),
        ))
    }

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        self.d(uv, textures, images)
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

impl<'a> Glass<'a> {
    fn ir(&self) -> f32 {
        self.data.v0.x
    }
}

impl<'a> Material for Glass<'a> {
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
        bsdf.add(EnumBxdf::new_fresnel_specular(self.ir()))
    }
}

impl EnumMaterial {
    pub fn new_matte(albedo_index: u32) -> Self {
        Self {
            t: MaterialType::Matte,
            data: EnumMaterialData {
                u0: uvec4(albedo_index, 0, 0, 0),
                v0: Vec4::ZERO,
            },
        }
    }

    pub fn new_substrate(
        diffuse_index: u32,
        specular_index: u32,
        rough_u: f32,
        rough_v: f32,
    ) -> Self {
        Self {
            t: MaterialType::Substrate,
            data: EnumMaterialData {
                u0: uvec4(diffuse_index, specular_index, 0, 0),
                v0: vec4(rough_u, rough_v, 0.0, 0.0),
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

    pub fn new_glass(ir: f32) -> Self {
        Self {
            t: MaterialType::Glass,
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
            MaterialType::Matte => Matte { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Glass => Glass { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Substrate => Substrate { data: &self.data }.albedo(uv, textures, images),
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
            MaterialType::Matte => {
                Matte { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
            MaterialType::Glass => {
                Glass { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
            MaterialType::Substrate => {
                Substrate { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
        }
    }
}
