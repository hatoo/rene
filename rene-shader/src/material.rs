#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{uvec4, vec3a, vec4, UVec4, Vec2, Vec3A, Vec4},
    RuntimeArray,
};

use crate::{
    reflection::{
        fresnel::EnumFresnel,
        microfacet::{EnumMicrofacetDistribution, TrowbridgeReitz},
        Bsdf, EnumBxdf,
    },
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
    Metal,
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

#[repr(transparent)]
struct Metal<'a> {
    data: &'a EnumMaterialData,
}

#[repr(transparent)]
struct Glass<'a> {
    data: &'a EnumMaterialData,
}

impl<'a> Matte<'a> {
    pub fn new_data(albedo_index: u32) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(albedo_index, 0, 0, 0),
            v0: Vec4::ZERO,
        }
    }
}

impl<'a> Material for Matte<'a> {
    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        EnumBxdf::setup_lambertian_reflection(self.albedo(uv, textures, images), bsdf.add_mut());
    }
}

impl<'a> Substrate<'a> {
    pub fn new_data(
        diffuse_index: u32,
        specular_index: u32,
        rough_u: f32,
        rough_v: f32,
        remap_roughness: bool,
    ) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(
                diffuse_index,
                specular_index,
                if remap_roughness { 1 } else { 0 },
                0,
            ),
            v0: vec4(rough_u, rough_v, 0.0, 0.0),
        }
    }
    fn d(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn s(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.y as usize) }.color(textures, images, uv)
    }

    fn rough_u(&self) -> f32 {
        self.data.v0.x
    }

    fn rough_v(&self) -> f32 {
        self.data.v0.y
    }

    fn remap_roughness(&self) -> bool {
        self.data.u0.z != 0
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

        let (rough_u, rough_v) = if self.remap_roughness() {
            (
                TrowbridgeReitz::roughness_to_alpha(self.rough_u()),
                TrowbridgeReitz::roughness_to_alpha(self.rough_v()),
            )
        } else {
            (self.rough_u(), self.rough_v())
        };

        EnumBxdf::setup_fresnel_blend(
            d,
            s,
            EnumMicrofacetDistribution::new_trowbridge_reitz(rough_u, rough_v),
            bsdf.add_mut(),
        );
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

impl<'a> Metal<'a> {
    fn new_data(
        eta_index: u32,
        k_index: u32,
        rough_u: f32,
        rough_v: f32,
        remap_roghness: bool,
    ) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(eta_index, k_index, if remap_roghness { 1 } else { 0 }, 0),
            v0: vec4(rough_u, rough_v, 0.0, 0.0),
        }
    }

    fn eta(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn k(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.y as usize) }.color(textures, images, uv)
    }

    fn rough_u(&self) -> f32 {
        self.data.v0.x
    }

    fn rough_v(&self) -> f32 {
        self.data.v0.y
    }

    fn remap_roughness(&self) -> bool {
        self.data.u0.z != 0
    }
}

impl<'a> Material for Metal<'a> {
    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        let (rough_u, rough_v) = if self.remap_roughness() {
            (
                TrowbridgeReitz::roughness_to_alpha(self.rough_u()),
                TrowbridgeReitz::roughness_to_alpha(self.rough_v()),
            )
        } else {
            (self.rough_u(), self.rough_v())
        };

        let fr_mf = EnumFresnel::new_fresnel_conductor(
            vec3a(1.0, 1.0, 1.0),
            self.eta(uv, textures, images),
            self.k(uv, textures, images),
        );

        let dist = EnumMicrofacetDistribution::new_trowbridge_reitz(rough_u, rough_v);

        EnumBxdf::setup_microfacet_reflection(vec3a(1.0, 1.0, 1.0), dist, fr_mf, bsdf.add_mut())
    }

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        self.k(uv, textures, images)
    }
}

impl<'a> Glass<'a> {
    pub fn new_data(ir: f32) -> EnumMaterialData {
        EnumMaterialData {
            u0: UVec4::ZERO,
            v0: vec4(ir, 0.0, 0.0, 0.0),
        }
    }
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
        EnumBxdf::setup_fresnel_specular(self.ir(), bsdf.add_mut());
    }
}

impl EnumMaterial {
    pub fn new_matte(albedo_index: u32) -> Self {
        Self {
            t: MaterialType::Matte,
            data: Matte::new_data(albedo_index),
        }
    }

    pub fn new_substrate(
        diffuse_index: u32,
        specular_index: u32,
        rough_u: f32,
        rough_v: f32,
        remap_roughness: bool,
    ) -> Self {
        Self {
            t: MaterialType::Substrate,
            data: Substrate::new_data(
                diffuse_index,
                specular_index,
                rough_u,
                rough_v,
                remap_roughness,
            ),
        }
    }

    pub fn new_metal(
        eta_index: u32,
        k_index: u32,
        rough_u: f32,
        rough_v: f32,
        remap_roghness: bool,
    ) -> Self {
        Self {
            t: MaterialType::Metal,
            data: Metal::new_data(eta_index, k_index, rough_u, rough_v, remap_roghness),
        }
    }

    pub fn new_glass(ir: f32) -> Self {
        Self {
            t: MaterialType::Glass,
            data: Glass::new_data(ir),
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
            MaterialType::Metal => Metal { data: &self.data }.albedo(uv, textures, images),
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
            MaterialType::Metal => {
                Metal { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
        }
    }
}
