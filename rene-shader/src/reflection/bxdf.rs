use core::f32::consts::PI;
use spirv_std::glam::{vec3a, vec4, Vec3A, Vec4Swizzles};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::{math::random_cosine_direction, rand::DefaultRng};

use super::{onb::Onb, Bxdf, EnumBxdfData, SampledF};

#[repr(transparent)]
pub struct Lambertian<'a> {
    pub data: &'a EnumBxdfData,
}

#[repr(transparent)]
pub struct Dielectric<'a> {
    pub data: &'a EnumBxdfData,
}

impl<'a> Lambertian<'a> {
    pub fn new(albedo: Vec3A) -> EnumBxdfData {
        EnumBxdfData {
            v0: albedo.extend(0.0),
        }
    }

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
        let onb = Onb::from_w(normal);
        let scatter_direction = onb.local(random_cosine_direction(rng)).normalize();

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
    pub fn new(ir: f32) -> EnumBxdfData {
        EnumBxdfData {
            v0: vec4(ir, 0.0, 0.0, 0.0),
        }
    }

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
