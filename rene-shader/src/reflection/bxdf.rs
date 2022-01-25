use core::f32::consts::PI;
use spirv_std::glam::{vec3a, vec4, Vec3A, Vec4Swizzles};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::{math::random_cosine_direction, rand::DefaultRng};

use super::{onb::Onb, Bxdf, BxdfKind, EnumBxdfData, SampledF};

#[repr(transparent)]
pub struct LambertianReflection<'a> {
    pub data: &'a EnumBxdfData,
}

#[repr(transparent)]
pub struct FresnelSpecular<'a> {
    pub data: &'a EnumBxdfData,
}

impl<'a> LambertianReflection<'a> {
    pub fn new_data(albedo: Vec3A) -> EnumBxdfData {
        EnumBxdfData {
            v0: albedo.extend(0.0),
        }
    }

    fn albedo(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }
}

impl<'a> Bxdf for LambertianReflection<'a> {
    fn kind(&self) -> super::BxdfKind {
        BxdfKind::REFLECTION
    }

    fn f(&self, _wo: Vec3A, _wi: Vec3A) -> Vec3A {
        self.albedo() / PI
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        let mut wi = random_cosine_direction(rng);

        if wo.z < 0.0 {
            wi.z *= -1.0;
        }

        let pdf = self.pdf(wo, wi);

        SampledF {
            wi,
            f: self.f(wo, wi),
            pdf,
        }
    }

    fn pdf(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        if Onb::local_same_hemisphere(wo, wi) {
            Onb::local_abs_cos_theta(wi) / PI
        } else {
            0.0
        }
    }
}

#[allow(dead_code)]
fn reflect(wo: Vec3A, n: Vec3A) -> Vec3A {
    -wo + 2.0 * wo.dot(n) * n
}

fn refract(wi: Vec3A, n: Vec3A, etai_over_etat: f32) -> Vec3A {
    let cos_theta_i = n.dot(wi);
    let sin2theta_i = (1.0 - cos_theta_i * cos_theta_i).max(0.0);
    let sin2theta_t = etai_over_etat * etai_over_etat * sin2theta_i;

    let cos_theta_t = (1.0 - sin2theta_t).sqrt();

    etai_over_etat * -wi + (etai_over_etat * cos_theta_i - cos_theta_t) * n
}

fn fr_dielectric(cos_theta_i: f32, eta_i: f32, eta_t: f32) -> f32 {
    let cos_theta_i = cos_theta_i.clamp(-1.0, 1.0);
    let entering = cos_theta_i > 0.0;

    let (eta_i, eta_t) = if !entering {
        (eta_t, eta_i)
    } else {
        (eta_i, eta_t)
    };

    let cos_theta_i = cos_theta_i.abs();

    let sin_theta_i = (1.0 - cos_theta_i * cos_theta_i).sqrt();
    let sin_theta_t = eta_i / eta_t * sin_theta_i;

    if sin_theta_t >= 1.0 {
        return 1.0;
    }

    let cos_theta_t = (1.0 - sin_theta_t * sin_theta_t).sqrt();

    let r_parl = ((eta_t * cos_theta_i) - (eta_i * cos_theta_t))
        / ((eta_t * cos_theta_i) + (eta_i * cos_theta_t));
    let r_perp = ((eta_i * cos_theta_i) - (eta_t * cos_theta_t))
        / ((eta_i * cos_theta_i) + (eta_t * cos_theta_t));

    0.5 * (r_parl * r_parl + r_perp * r_perp)
}

#[allow(dead_code)]
fn reflectance(cosine: f32, ref_idx: f32) -> f32 {
    let r0 = (1.0 - ref_idx) / (1.0 + ref_idx);
    let r0 = r0 * r0;
    r0 + (1.0 - r0) * (1.0 - cosine).powf(5.0)
}

impl<'a> FresnelSpecular<'a> {
    pub fn new_data(ir: f32) -> EnumBxdfData {
        EnumBxdfData {
            v0: vec4(ir, 0.0, 0.0, 0.0),
        }
    }

    fn ir(&self) -> f32 {
        self.data.v0.x
    }
}

impl<'a> Bxdf for FresnelSpecular<'a> {
    fn kind(&self) -> super::BxdfKind {
        BxdfKind::REFLECTION | BxdfKind::TRANSMISSION
    }

    fn f(&self, _wo: Vec3A, _wi: Vec3A) -> Vec3A {
        Vec3A::ZERO
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        let (eta_i, eta_t) = if Onb::local_cos_theta(wo) > 0.0 {
            (1.0, self.ir())
        } else {
            (self.ir(), 1.0)
        };

        let refraction_ratio = eta_i / eta_t;

        let cos_theta = Onb::local_cos_theta(wo).min(1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        let cannot_refract = refraction_ratio * sin_theta > 1.0;

        let f = fr_dielectric(cos_theta, eta_i, eta_t);

        if cannot_refract || f > rng.next_f32() {
            let wi = vec3a(-wo.x, -wo.y, wo.z);

            SampledF {
                wi,
                f: f * vec3a(1.0, 1.0, 1.0) / Onb::local_abs_cos_theta(wi),
                pdf: f,
            }
        } else {
            let wi = refract(
                wo,
                if wo.z > 0.0 {
                    vec3a(0.0, 0.0, 1.0)
                } else {
                    vec3a(0.0, 0.0, -1.0)
                },
                refraction_ratio,
            );

            SampledF {
                wi,
                f: vec3a(1.0, 1.0, 1.0) * (1.0 - f) / Onb::local_abs_cos_theta(wi),
                pdf: 1.0 - f,
            }
        }
    }

    fn pdf(&self, _wi: Vec3A, _normal: Vec3A) -> f32 {
        0.0
    }
}
