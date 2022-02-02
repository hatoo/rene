use core::f32::consts::{FRAC_1_PI, PI};
use spirv_std::glam::{vec2, vec3a, vec4, Vec2, Vec3A, Vec4Swizzles};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::{
    asm::f32_clamp, math::random_cosine_direction, rand::DefaultRng, reflection::fresnel::Fresnel,
};

use super::{
    fresnel::EnumFresnel,
    microfacet::{EnumMicrofacetDistribution, MicrofacetDistribution},
    onb::Onb,
    Bxdf, BxdfKind, EnumBxdfData, SampledF,
};

#[repr(transparent)]
pub struct LambertianReflection<'a> {
    pub data: &'a EnumBxdfData,
}

#[repr(transparent)]
pub struct FresnelSpecular<'a> {
    pub data: &'a EnumBxdfData,
}

#[repr(transparent)]
pub struct FresnelBlend<'a> {
    pub data: &'a EnumBxdfData,
}

#[repr(transparent)]
pub struct MicrofacetReflection<'a> {
    pub data: &'a EnumBxdfData,
}

#[allow(dead_code)]
fn concentric_sample_disk(rng: &mut DefaultRng) -> Vec2 {
    let u_offset = 2.0 * vec2(rng.next_f32(), rng.next_f32()) - vec2(1.0, 1.0);

    if u_offset == Vec2::ZERO {
        return Vec2::ZERO;
    }

    let (theta, r) = if u_offset.x.abs() > u_offset.y.abs() {
        (PI / 4.0 * (u_offset.y / u_offset.x), u_offset.x)
    } else {
        (PI / 2.0 - PI / 4.0 * (u_offset.x / u_offset.y), u_offset.y)
    };

    r * vec2(theta.cos(), theta.sin())
}

#[allow(dead_code)]
fn cosine_sample_hemisphere(rng: &mut DefaultRng) -> Vec3A {
    let d = concentric_sample_disk(rng);
    let z = (1.0 - d.x * d.x - d.y * d.y).max(0.0).sqrt();

    vec3a(d.x, d.y, z)
}

impl<'a> LambertianReflection<'a> {
    #[allow(dead_code)]
    pub fn new_data(albedo: Vec3A) -> EnumBxdfData {
        EnumBxdfData {
            v0: albedo.extend(0.0),
            ..Default::default()
        }
    }

    pub fn setup_data(albedo: Vec3A, data: &mut EnumBxdfData) {
        data.v0.x = albedo.x;
        data.v0.y = albedo.y;
        data.v0.z = albedo.z;
    }

    fn albedo(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }
}

impl<'a> Bxdf for LambertianReflection<'a> {
    fn kind(&self) -> super::BxdfKind {
        BxdfKind::REFLECTION | BxdfKind::DIFFUSE
    }

    fn f(&self, _wo: Vec3A, _wi: Vec3A) -> Vec3A {
        self.albedo() * FRAC_1_PI
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        let mut wi = random_cosine_direction(rng);

        if wo.z < 0.0 {
            wi.z = -wi.z;
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
            Onb::local_abs_cos_theta(wi) * FRAC_1_PI
        } else {
            0.0
        }
    }
}

#[allow(dead_code)]
fn reflect(wo: Vec3A, n: Vec3A) -> Vec3A {
    -wo + 2.0 * wo.dot(n) * n
}

fn refract(wi: Vec3A, n: Vec3A, etai_over_etat: f32) -> (bool, Vec3A) {
    let cos_theta_i = n.dot(wi);
    let sin2theta_i = (1.0 - cos_theta_i * cos_theta_i).max(0.0);
    let sin2theta_t = etai_over_etat * etai_over_etat * sin2theta_i;

    if sin2theta_t >= 1.0 {
        return (false, Vec3A::ZERO);
    }

    let cos_theta_t = (1.0 - sin2theta_t).sqrt();

    (
        true,
        etai_over_etat * -wi + (etai_over_etat * cos_theta_i - cos_theta_t) * n,
    )
}

fn fr_dielectric(cos_theta_i: f32, eta_i: f32, eta_t: f32) -> f32 {
    let cos_theta_i = f32_clamp(cos_theta_i, -1.0, 1.0);
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
    #[allow(dead_code)]
    pub fn new_data(ir: f32) -> EnumBxdfData {
        EnumBxdfData {
            v0: vec4(ir, 0.0, 0.0, 0.0),
            ..Default::default()
        }
    }

    pub fn setup_data(ir: f32, data: &mut EnumBxdfData) {
        data.v0.x = ir;
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
        let cos_theta = Onb::local_cos_theta(wo);
        let f = fr_dielectric(cos_theta, 1.0, self.ir());

        if rng.next_f32() < f {
            let wi = vec3a(-wo.x, -wo.y, wo.z);

            SampledF {
                wi,
                f: f * vec3a(1.0, 1.0, 1.0) / Onb::local_abs_cos_theta(wi),
                pdf: f,
            }
        } else {
            let (eta_i, eta_t) = if Onb::local_cos_theta(wo) > 0.0 {
                (1.0, self.ir())
            } else {
                (self.ir(), 1.0)
            };

            let refraction_ratio = eta_i / eta_t;

            let (b, wi) = refract(
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
                pdf: if !b { 0.0 } else { 1.0 - f },
            }
        }
    }

    fn pdf(&self, _wi: Vec3A, _normal: Vec3A) -> f32 {
        0.0
    }
}

impl<'a> FresnelBlend<'a> {
    #[allow(dead_code)]
    pub fn new_data(
        rd: Vec3A,
        rs: Vec3A,
        distribution: EnumMicrofacetDistribution,
    ) -> EnumBxdfData {
        EnumBxdfData {
            v0: rd.extend(0.0),
            v1: rs.extend(0.0),
            microfacet_distribution: distribution,
            ..Default::default()
        }
    }

    pub fn setup_data(
        rd: Vec3A,
        rs: Vec3A,
        distribution: EnumMicrofacetDistribution,
        data: &mut EnumBxdfData,
    ) {
        data.v0 = rd.extend(0.0);
        data.v1 = rs.extend(0.0);
        data.microfacet_distribution = distribution;
    }

    fn rd(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }

    fn rs(&self) -> Vec3A {
        self.data.v1.xyz().into()
    }

    fn schlick_fresnel(&self, cos_theta: f32) -> Vec3A {
        let v = 1.0 - cos_theta;
        let v5 = (v * v) * (v * v) * v;

        self.rs() + v5 * (vec3a(1.0, 1.0, 1.0) - self.rs())
    }
}

impl<'a> Bxdf for FresnelBlend<'a> {
    fn kind(&self) -> BxdfKind {
        BxdfKind::REFLECTION | BxdfKind::DIFFUSE
    }

    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A {
        let pow5 = |v: f32| (v * v) * (v * v) * v;

        let diffuse = (28.0 / (23.0 * PI))
            * self.rd()
            * (vec3a(1.0, 1.0, 1.0) - self.rs())
            * (1.0 - pow5(1.0 - 0.5 * Onb::local_abs_cos_theta(wi)))
            * (1.0 - pow5(1.0 - 0.5 * Onb::local_abs_cos_theta(wo)));

        let wh = wi + wo;

        if wh == Vec3A::ZERO {
            return Vec3A::ZERO;
        }

        let wh = wh.normalize();

        let specular = self.data.microfacet_distribution.d(wh)
            / (4.0
                * wi.dot(wh).abs()
                * Onb::local_abs_cos_theta(wi).max(Onb::local_abs_cos_theta(wo)))
            * self.schlick_fresnel(wi.dot(wh));

        diffuse + specular
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        let wi = if rng.next_f32() < 0.5 {
            let mut wi = random_cosine_direction(rng);

            if wo.z < 0.0 {
                wi.z = -wi.z;
            }

            wi
        } else {
            let wh = self.data.microfacet_distribution.sample_wh(wo, rng);
            let wi = reflect(wo, wh);

            if !Onb::local_same_hemisphere(wo, wi) {
                return SampledF::default();
            }

            wi
        };

        SampledF {
            wi,
            f: self.f(wo, wi),
            pdf: self.pdf(wo, wi),
        }
    }

    fn pdf(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        if !Onb::local_same_hemisphere(wo, wi) {
            return 0.0;
        }

        let wh = (wo + wi).normalize();
        let pdf_wh = self.data.microfacet_distribution.pdf(wo, wh);

        0.5 * (Onb::local_abs_cos_theta(wi) * FRAC_1_PI + pdf_wh / (4.0 * wo.dot(wh)))
    }
}

impl<'a> MicrofacetReflection<'a> {
    #[allow(dead_code)]
    pub fn new_data(
        r: Vec3A,
        microfacet_distribution: EnumMicrofacetDistribution,
        fresnel: EnumFresnel,
    ) -> EnumBxdfData {
        EnumBxdfData {
            v0: r.extend(0.0),
            microfacet_distribution,
            fresnel,
            ..Default::default()
        }
    }

    pub fn setup_data(
        r: Vec3A,
        microfacet_distribution: EnumMicrofacetDistribution,
        fresnel: EnumFresnel,
        data: &mut EnumBxdfData,
    ) {
        data.v0 = r.extend(0.0);
        data.microfacet_distribution = microfacet_distribution;
        data.fresnel = fresnel;
    }

    fn r(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }
}

fn face_forward(v: Vec3A, v2: Vec3A) -> Vec3A {
    if v.dot(v2) < 0.0 {
        -v
    } else {
        v
    }
}

impl<'a> Bxdf for MicrofacetReflection<'a> {
    fn kind(&self) -> BxdfKind {
        BxdfKind::REFLECTION | BxdfKind::DIFFUSE
    }

    fn f(&self, wo: Vec3A, wi: Vec3A) -> Vec3A {
        let cos_theta_o = Onb::local_abs_cos_theta(wo);
        let cos_theta_i = Onb::local_abs_cos_theta(wi);

        let wh = wi + wo;

        if cos_theta_i == 0.0 || cos_theta_o == 0.0 || wh == Vec3A::ZERO {
            return Vec3A::ZERO;
        }

        let wh = wh.normalize();

        let f = self
            .data
            .fresnel
            .evaluate(wi.dot(face_forward(wh, vec3a(0.0, 0.0, 1.0))));

        self.r()
            * self.data.microfacet_distribution.d(wh)
            * self.data.microfacet_distribution.g(wo, wi)
            * f
            / (4.0 * cos_theta_i * cos_theta_o)
    }

    fn sample_f(&self, wo: Vec3A, rng: &mut DefaultRng) -> SampledF {
        if wo.z == 0.0 {
            return SampledF::default();
        }

        let wh = self.data.microfacet_distribution.sample_wh(wo, rng);
        if wo.dot(wh) < 0.0 {
            return SampledF::default();
        }
        let wi = reflect(wo, wh);
        if !Onb::local_same_hemisphere(wo, wi) {
            return SampledF::default();
        }

        let pdf = self.data.microfacet_distribution.pdf(wo, wh) / (4.0 * wo.dot(wh));

        SampledF {
            wi,
            f: self.f(wo, wi),
            pdf,
        }
    }

    fn pdf(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        if !Onb::local_same_hemisphere(wo, wi) {
            return 0.0;
        }
        let wh = (wo + wi).normalize();
        self.data.microfacet_distribution.pdf(wo, wh) / (4.0 * wo.dot(wh))
    }
}
