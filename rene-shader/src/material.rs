use spirv_std::glam::{uvec4, vec3a, vec4, UVec4, Vec2, Vec3A, Vec4, Vec4Swizzles};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::{
    math::{random_in_unit_sphere, IsNearZero},
    rand::DefaultRng,
    texture::EnumTexture,
    Ray, RayPayload,
};

#[derive(Clone, Default)]
pub struct Scatter {
    pub color: Vec3A,
    pub ray: Ray,
}

pub trait Material {
    fn scatter(
        &self,
        textures: &[EnumTexture],
        ray: &Ray,
        ray_payload: &RayPayload,
        rng: &mut DefaultRng,
        scatter: &mut Scatter,
    ) -> bool;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumMaterialData {
    u0: UVec4,
    v0: Vec4,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumMaterial {
    t: u32,
    data: EnumMaterialData,
}

#[repr(transparent)]
struct Lambertian<'a> {
    data: &'a EnumMaterialData,
}

#[repr(transparent)]
struct Metal<'a> {
    data: &'a EnumMaterialData,
}

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

impl<'a> Lambertian<'a> {
    fn albedo(&self, textures: &[EnumTexture], uv: Vec2) -> Vec3A {
        textures[self.data.u0.x as usize].color(textures, uv)
    }
}

impl<'a> Material for Lambertian<'a> {
    fn scatter(
        &self,
        textures: &[EnumTexture],
        _ray: &Ray,
        ray_payload: &RayPayload,
        rng: &mut DefaultRng,
        scatter: &mut Scatter,
    ) -> bool {
        let scatter_direction = ray_payload.normal + random_in_unit_sphere(rng).normalize();

        let scatter_direction = if scatter_direction.is_near_zero() {
            ray_payload.normal
        } else {
            scatter_direction
        };

        let scatterd = Ray {
            origin: ray_payload.position,
            direction: scatter_direction,
        };

        *scatter = Scatter {
            color: self.albedo(textures, ray_payload.uv),
            ray: scatterd,
        };
        true
    }
}

impl<'a> Metal<'a> {
    fn albedo(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }

    fn fuzz(&self) -> f32 {
        self.data.v0.w
    }
}

impl<'a> Material for Metal<'a> {
    fn scatter(
        &self,
        _: &[EnumTexture],
        ray: &Ray,
        ray_payload: &RayPayload,
        rng: &mut DefaultRng,
        scatter: &mut Scatter,
    ) -> bool {
        let reflected = reflect(ray.direction.normalize(), ray_payload.normal);
        let scatterd = reflected + self.fuzz() * random_in_unit_sphere(rng);
        if scatterd.dot(ray_payload.normal) > 0.0 {
            *scatter = Scatter {
                color: self.albedo(),
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
}

impl<'a> Dielectric<'a> {
    fn ir(&self) -> f32 {
        self.data.v0.x
    }
}

impl<'a> Material for Dielectric<'a> {
    fn scatter(
        &self,
        _: &[EnumTexture],
        ray: &Ray,
        ray_payload: &RayPayload,
        rng: &mut DefaultRng,
        scatter: &mut Scatter,
    ) -> bool {
        let refraction_ratio = if ray_payload.front_face != 0 {
            1.0 / self.ir()
        } else {
            self.ir()
        };

        let unit_direction = ray.direction.normalize();
        let cos_theta = (-unit_direction).dot(ray_payload.normal).min(1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let cannot_refract = refraction_ratio * sin_theta > 1.0;

        let direction =
            if cannot_refract || reflectance(cos_theta, refraction_ratio) > rng.next_f32() {
                reflect(unit_direction, ray_payload.normal)
            } else {
                refract(unit_direction, ray_payload.normal, refraction_ratio)
            };

        *scatter = Scatter {
            color: vec3a(1.0, 1.0, 1.0),
            ray: Ray {
                origin: ray_payload.position,
                direction,
            },
        };
        true
    }
}

impl EnumMaterial {
    pub fn new_lambertian(albedo_index: u32) -> Self {
        Self {
            t: 0,
            data: EnumMaterialData {
                u0: uvec4(albedo_index, 0, 0, 0),
                v0: Vec4::ZERO,
            },
        }
    }

    pub fn new_metal(albedo: Vec3A, fuzz: f32) -> Self {
        Self {
            t: 1,
            data: EnumMaterialData {
                u0: UVec4::ZERO,
                v0: vec4(albedo.x, albedo.y, albedo.z, fuzz),
            },
        }
    }

    pub fn new_dielectric(ir: f32) -> Self {
        Self {
            t: 2,
            data: EnumMaterialData {
                u0: UVec4::ZERO,
                v0: vec4(ir, 0.0, 0.0, 0.0),
            },
        }
    }
}

impl Material for EnumMaterial {
    fn scatter(
        &self,
        textures: &[EnumTexture],
        ray: &Ray,
        ray_payload: &RayPayload,
        rng: &mut DefaultRng,
        scatter: &mut Scatter,
    ) -> bool {
        match self.t {
            0 => Lambertian { data: &self.data }.scatter(textures, ray, ray_payload, rng, scatter),
            1 => Metal { data: &self.data }.scatter(textures, ray, ray_payload, rng, scatter),
            _ => Dielectric { data: &self.data }.scatter(textures, ray, ray_payload, rng, scatter),
        }
    }
}
