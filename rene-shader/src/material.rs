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
    u1: UVec4,
    v0: Vec4,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
enum MaterialType {
    None,
    Matte,
    Glass,
    Substrate,
    Metal,
    Mirror,
    Uber,
    Plastic,
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

#[repr(transparent)]
struct Mirror<'a> {
    data: &'a EnumMaterialData,
}

#[repr(transparent)]
struct Uber<'a> {
    data: &'a EnumMaterialData,
}

#[repr(transparent)]
struct Plastic<'a> {
    data: &'a EnumMaterialData,
}

impl<'a> Matte<'a> {
    pub fn new_data(albedo_index: u32) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(albedo_index, 0, 0, 0),
            v0: Vec4::ZERO,
            ..Default::default()
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
        rough_u_index: u32,
        rough_v_index: u32,
        remap_roughness: bool,
    ) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(diffuse_index, specular_index, rough_u_index, rough_v_index),
            u1: uvec4(if remap_roughness { 1 } else { 0 }, 0, 0, 0),
            ..Default::default()
        }
    }
    fn d(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn s(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.y as usize) }.color(textures, images, uv)
    }

    fn rough_u(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> f32 {
        unsafe { textures.index_unchecked(self.data.u0.z as usize) }
            .color(textures, images, uv)
            .x
    }

    fn rough_v(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> f32 {
        unsafe { textures.index_unchecked(self.data.u0.w as usize) }
            .color(textures, images, uv)
            .x
    }

    fn remap_roughness(&self) -> bool {
        self.data.u1.x != 0
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
                TrowbridgeReitz::roughness_to_alpha(self.rough_u(uv, textures, images)),
                TrowbridgeReitz::roughness_to_alpha(self.rough_v(uv, textures, images)),
            )
        } else {
            (
                self.rough_u(uv, textures, images),
                self.rough_v(uv, textures, images),
            )
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
        rough_u_index: u32,
        rough_v_index: u32,
        remap_roghness: bool,
    ) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(eta_index, k_index, rough_u_index, rough_v_index),
            u1: uvec4(if remap_roghness { 1 } else { 0 }, 0, 0, 0),
            ..Default::default()
        }
    }

    fn eta(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn k(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.y as usize) }.color(textures, images, uv)
    }

    fn rough_u(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> f32 {
        unsafe { textures.index_unchecked(self.data.u0.z as usize) }
            .color(textures, images, uv)
            .x
    }

    fn rough_v(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> f32 {
        unsafe { textures.index_unchecked(self.data.u0.w as usize) }
            .color(textures, images, uv)
            .x
    }

    fn remap_roughness(&self) -> bool {
        self.data.u1.x != 0
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
                TrowbridgeReitz::roughness_to_alpha(self.rough_u(uv, textures, images)),
                TrowbridgeReitz::roughness_to_alpha(self.rough_v(uv, textures, images)),
            )
        } else {
            (
                self.rough_u(uv, textures, images),
                self.rough_v(uv, textures, images),
            )
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
            ..Default::default()
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

impl<'a> Mirror<'a> {
    fn new_data(r_index: u32) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(r_index, 0, 0, 0),
            ..Default::default()
        }
    }
}

impl<'a> Material for Mirror<'a> {
    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        let fresnel = EnumFresnel::new_nop();
        let bxdf = bsdf.add_mut();
        EnumBxdf::setup_specular_reflection(self.albedo(uv, textures, images), fresnel, bxdf);
    }

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }
}

impl EnumMaterial {
    pub fn is_none(&self) -> bool {
        self.t == MaterialType::None
    }

    pub fn new_matte(albedo_index: u32) -> Self {
        Self {
            t: MaterialType::Matte,
            data: Matte::new_data(albedo_index),
        }
    }

    pub fn new_substrate(
        diffuse_index: u32,
        specular_index: u32,
        rough_u_index: u32,
        rough_v_index: u32,
        remap_roughness: bool,
    ) -> Self {
        Self {
            t: MaterialType::Substrate,
            data: Substrate::new_data(
                diffuse_index,
                specular_index,
                rough_u_index,
                rough_v_index,
                remap_roughness,
            ),
        }
    }

    pub fn new_metal(
        eta_index: u32,
        k_index: u32,
        rough_u_index: u32,
        rough_v_index: u32,
        remap_roghness: bool,
    ) -> Self {
        Self {
            t: MaterialType::Metal,
            data: Metal::new_data(
                eta_index,
                k_index,
                rough_u_index,
                rough_v_index,
                remap_roghness,
            ),
        }
    }

    pub fn new_glass(ir: f32) -> Self {
        Self {
            t: MaterialType::Glass,
            data: Glass::new_data(ir),
        }
    }

    pub fn new_mirror(r_index: u32) -> Self {
        Self {
            t: MaterialType::Mirror,
            data: Mirror::new_data(r_index),
        }
    }

    pub fn new_uber(
        kd_index: u32,
        ks_index: u32,
        kr_index: u32,
        kt_index: u32,
        rough_u_index: u32,
        rough_v_index: u32,
        opacity_index: u32,
        eta: f32,
        remap_roughness: bool,
    ) -> Self {
        Self {
            t: MaterialType::Uber,
            data: Uber::new_data(
                kd_index,
                ks_index,
                kr_index,
                kt_index,
                rough_u_index,
                rough_v_index,
                opacity_index,
                eta,
                remap_roughness,
            ),
        }
    }

    pub fn new_plastic(
        kd_index: u32,
        ks_index: u32,
        roughness_index: u32,
        remap_roughness: bool,
    ) -> Self {
        Self {
            t: MaterialType::Plastic,
            data: Plastic::new_data(kd_index, ks_index, roughness_index, remap_roughness),
        }
    }

    pub fn new_none() -> Self {
        Self {
            t: MaterialType::None,
            data: Default::default(),
        }
    }
}

impl<'a> Uber<'a> {
    pub fn new_data(
        kd_index: u32,
        ks_index: u32,
        kr_index: u32,
        kt_index: u32,
        rough_u_index: u32,
        rough_v_index: u32,
        opacity_index: u32,
        eta: f32,
        remap_roughness: bool,
    ) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(kd_index, ks_index, kr_index, kt_index),
            u1: uvec4(
                opacity_index,
                if remap_roughness { 1 } else { 0 },
                rough_u_index,
                rough_v_index,
            ),
            v0: vec4(eta, 0.0, 0.0, 0.0),
        }
    }

    fn kd(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn ks(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.y as usize) }.color(textures, images, uv)
    }

    fn kr(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.z as usize) }.color(textures, images, uv)
    }

    fn kt(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.w as usize) }.color(textures, images, uv)
    }

    fn rough_u(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> f32 {
        unsafe { textures.index_unchecked(self.data.u1.z as usize) }
            .color(textures, images, uv)
            .x
    }

    fn rough_v(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> f32 {
        unsafe { textures.index_unchecked(self.data.u1.w as usize) }
            .color(textures, images, uv)
            .x
    }

    fn opacity(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u1.x as usize) }.color(textures, images, uv)
    }

    fn eta(&self) -> f32 {
        self.data.v0.x
    }

    fn remap_roughness(&self) -> bool {
        self.data.u1.y != 0
    }
}

impl<'a> Material for Uber<'a> {
    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        let e = self.eta();

        let op = self.opacity(uv, textures, images);
        let t = vec3a(1.0, 1.0, 1.0) - op;

        if t != Vec3A::ZERO {
            EnumBxdf::setup_specular_transmission(t, 1.0, 1.0, bsdf.add_mut());
        }

        let kd = self.kd(uv, textures, images);

        if kd != Vec3A::ZERO {
            EnumBxdf::setup_lambertian_reflection(kd, bsdf.add_mut());
        }

        let ks = self.ks(uv, textures, images);
        if ks != Vec3A::ZERO {
            let fresnel = EnumFresnel::new_fresnel_dielectric(1.0, e);
            let (rough_u, rough_v) = if self.remap_roughness() {
                (
                    TrowbridgeReitz::roughness_to_alpha(self.rough_u(uv, textures, images)),
                    TrowbridgeReitz::roughness_to_alpha(self.rough_v(uv, textures, images)),
                )
            } else {
                (
                    self.rough_u(uv, textures, images),
                    self.rough_v(uv, textures, images),
                )
            };

            let distrib = EnumMicrofacetDistribution::new_trowbridge_reitz(rough_u, rough_v);
            EnumBxdf::setup_microfacet_reflection(ks, distrib, fresnel, bsdf.add_mut());
        }

        let kr = op * self.kr(uv, textures, images);
        if kr != Vec3A::ZERO {
            let fresnel = EnumFresnel::new_fresnel_dielectric(1.0, e);
            EnumBxdf::setup_specular_reflection(kr, fresnel, bsdf.add_mut());
        }

        let kt = op * self.kt(uv, textures, images);
        if kt != Vec3A::ZERO {
            EnumBxdf::setup_specular_transmission(kt, 1.0, e, bsdf.add_mut());
        }
    }

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        self.kd(uv, textures, images)
    }
}

impl<'a> Plastic<'a> {
    fn new_data(
        kd_index: u32,
        ks_index: u32,
        roughness_index: u32,
        remap_roughness: bool,
    ) -> EnumMaterialData {
        EnumMaterialData {
            u0: uvec4(
                kd_index,
                ks_index,
                if remap_roughness { 1 } else { 0 },
                roughness_index,
            ),
            ..Default::default()
        }
    }

    fn kd(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.x as usize) }.color(textures, images, uv)
    }

    fn ks(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> Vec3A {
        unsafe { textures.index_unchecked(self.data.u0.y as usize) }.color(textures, images, uv)
    }

    fn rough(&self, uv: Vec2, textures: &[EnumTexture], images: &RuntimeArray<InputImage>) -> f32 {
        unsafe { textures.index_unchecked(self.data.u0.w as usize) }
            .color(textures, images, uv)
            .x
    }

    fn remap_roughness(&self) -> bool {
        self.data.u1.z != 0
    }
}

impl<'a> Material for Plastic<'a> {
    fn compute_bsdf(
        &self,
        bsdf: &mut Bsdf,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) {
        let kd = self.kd(uv, textures, images);

        if kd != Vec3A::ZERO {
            EnumBxdf::setup_lambertian_reflection(kd, bsdf.add_mut());
        }

        let ks = self.ks(uv, textures, images);

        if ks != Vec3A::ZERO {
            let rough = if self.remap_roughness() {
                TrowbridgeReitz::roughness_to_alpha(self.rough(uv, textures, images))
            } else {
                self.rough(uv, textures, images)
            };

            let fresnel = EnumFresnel::new_fresnel_dielectric(1.5, 1.0);
            let distrib = EnumMicrofacetDistribution::new_trowbridge_reitz(rough, rough);

            EnumBxdf::setup_microfacet_reflection(ks, distrib, fresnel, bsdf.add_mut());
        }
    }

    fn albedo(
        &self,
        uv: Vec2,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
    ) -> Vec3A {
        self.kd(uv, textures, images)
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
            MaterialType::None => Vec3A::ZERO,
            MaterialType::Matte => Matte { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Glass => Glass { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Substrate => Substrate { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Metal => Metal { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Mirror => Mirror { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Uber => Uber { data: &self.data }.albedo(uv, textures, images),
            MaterialType::Plastic => Plastic { data: &self.data }.albedo(uv, textures, images),
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
            MaterialType::None => {}
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
            MaterialType::Mirror => {
                Mirror { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
            MaterialType::Uber => {
                Uber { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
            MaterialType::Plastic => {
                Plastic { data: &self.data }.compute_bsdf(bsdf, uv, textures, images)
            }
        }
    }
}
