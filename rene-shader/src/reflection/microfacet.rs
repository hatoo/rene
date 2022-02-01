use core::f32::consts::{PI, TAU};
use spirv_std::glam::{vec2, vec3a, vec4, Vec2, Vec3A, Vec4};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::rand::DefaultRng;

use super::onb::Onb;

pub trait MicrofacetDistribution {
    fn d(&self, wh: Vec3A) -> f32;
    fn lambda(&self, w: Vec3A) -> f32;
    fn sample_wh(&self, wo: Vec3A, rng: &mut DefaultRng) -> Vec3A;
    fn pdf(&self, wo: Vec3A, wh: Vec3A) -> f32;

    fn g(&self, wo: Vec3A, wi: Vec3A) -> f32 {
        1.0 / (1.0 + self.lambda(wo) + self.lambda(wi))
    }

    fn g1(&self, w: Vec3A) -> f32 {
        1.0 / (1.0 + self.lambda(w))
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(u32)]
pub enum MicrofacetDistributionType {
    TrowbridgeReitz,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumMicrofacetDistributionData {
    v0: Vec4,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumMicrofacetDistribution {
    t: MicrofacetDistributionType,
    data: EnumMicrofacetDistributionData,
}

#[repr(transparent)]
pub struct TrowbridgeReitz<'a> {
    pub data: &'a EnumMicrofacetDistributionData,
}

impl<'a> TrowbridgeReitz<'a> {
    fn new_data(alpha_x: f32, alpha_y: f32) -> EnumMicrofacetDistributionData {
        EnumMicrofacetDistributionData {
            v0: vec4(alpha_x, alpha_y, 0.0, 0.0),
        }
    }

    fn alpha_x(&self) -> f32 {
        self.data.v0.x
    }

    fn alpha_y(&self) -> f32 {
        self.data.v0.y
    }

    pub fn roughness_to_alpha(roughness: f32) -> f32 {
        let roughness = roughness.max(1e-3);
        let x = roughness.ln();

        1.62142
            + 0.819955 * x
            + 0.1734 * x * x
            + 0.0171201 * x * x * x
            + 0.000640711 * x * x * x * x
    }
}

fn trowbridge_reitz_sample11(cos_theta: f32, rng: &mut DefaultRng) -> Vec2 {
    let u1 = rng.next_f32();
    let mut u2 = rng.next_f32();

    if cos_theta > 0.9999 {
        let r = (u1 / (1.0 - u1)).sqrt();
        let phi = TAU * u2;

        return vec2(r * phi.cos(), r * phi.sin());
    }

    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let tan_theta = sin_theta / cos_theta;
    let a0 = 1.0 / tan_theta;
    let g1 = 2.0 / (1.0 + (1.0 + 1.0 / (a0 * a0).sqrt()));

    let a = 2.0 * u1 / g1 - 1.0;
    let mut tmp = 1.0 / (a * a - 1.0);
    if tmp > 1e10 {
        tmp = 1e10;
    }
    let b = tan_theta;
    let d = (b * b * tmp * tmp - (a * a - b * b) * tmp).max(0.0).sqrt();
    let slope_x_1 = b * tmp - d;
    let slope_x_2 = b * tmp + d;

    let slope_x = if a < 0.0 || slope_x_2 > 1.0 / tan_theta {
        slope_x_1
    } else {
        slope_x_2
    };

    let s;

    if u2 > 0.5 {
        s = 1.0;
        u2 = 2.0 * (u2 - 0.5);
    } else {
        s = -1.0;
        u2 = 2.0 * (0.5 - u2);
    }

    let z = (u2 * (u2 * (u2 * 0.27385 - 0.73369) + 0.46341))
        / (u2 * (u2 * (u2 * 0.093073 + 0.309420) - 1.000000) + 0.597999);

    let slope_y = s * z * (1.0 + slope_x * slope_x).sqrt();

    vec2(slope_x, slope_y)
}

fn trowbridge_reitz_sample(wi: Vec3A, alpha_x: f32, alpha_y: f32, rng: &mut DefaultRng) -> Vec3A {
    let wi_stretched = vec3a(alpha_x * wi.x, alpha_y * wi.y, wi.z).normalize();

    let slope = trowbridge_reitz_sample11(Onb::local_cos_theta(wi_stretched), rng);

    let slope_x =
        Onb::local_cos_phi(wi_stretched) * slope.x - Onb::local_sin_phi(wi_stretched) * slope.y;
    let slope_y =
        Onb::local_sin_phi(wi_stretched) * slope.x + Onb::local_cos_phi(wi_stretched) * slope.y;

    let slope_x = alpha_x * slope_x;
    let slope_y = alpha_y * slope_y;

    vec3a(-slope_x, -slope_y, 1.0).normalize()
}

impl<'a> MicrofacetDistribution for TrowbridgeReitz<'a> {
    fn d(&self, wh: Vec3A) -> f32 {
        let tan2_theta = Onb::local_tan2_theta(wh);

        if tan2_theta.is_infinite() {
            return 0.0;
        }

        let cos2_theta = Onb::local_cos2_theta(wh);
        let cos4_thata = cos2_theta * cos2_theta;
        let e = (Onb::local_cos2_phi(wh) / (self.alpha_x() * self.alpha_x())
            + Onb::local_sin2_phi(wh) / (self.alpha_y() * self.alpha_y()))
            * tan2_theta;

        1.0 / (PI * self.alpha_x() * self.alpha_y() * cos4_thata * (1.0 + e) * (1.0 + e))
    }

    fn lambda(&self, w: Vec3A) -> f32 {
        let abs_tan_theta = Onb::local_tan_theta(w).abs();
        if abs_tan_theta.is_infinite() {
            return 0.0;
        }

        let alpha = (Onb::local_cos2_phi(w) * self.alpha_x() * self.alpha_x()
            + Onb::local_sin2_phi(w) * self.alpha_y() * self.alpha_y())
        .sqrt();

        let a = 1.0 / (alpha * abs_tan_theta);

        if a >= 1.6 {
            return 0.0;
        }

        (1.0 - 1.259 * a + 0.396 * a * a) / (3.535 * a + 2.181 * a * a)
    }

    fn sample_wh(&self, wo: Vec3A, rng: &mut DefaultRng) -> Vec3A {
        let flip = wo.z < 0.0;
        let wh = trowbridge_reitz_sample(
            if flip { -wo } else { wo },
            self.alpha_x(),
            self.alpha_y(),
            rng,
        );

        if flip {
            -wh
        } else {
            wh
        }
    }

    fn pdf(&self, wo: Vec3A, wh: Vec3A) -> f32 {
        self.d(wh) * self.g1(wo) * wo.dot(wh).abs() / Onb::local_abs_cos_theta(wo)
    }
}

impl Default for EnumMicrofacetDistribution {
    fn default() -> Self {
        Self {
            t: MicrofacetDistributionType::TrowbridgeReitz,
            data: EnumMicrofacetDistributionData {
                v0: vec4(0.0, 0.0, 0.0, 0.0),
            },
        }
    }
}

impl EnumMicrofacetDistribution {
    pub fn new_trowbridge_reitz(alpha_x: f32, alpha_y: f32) -> Self {
        Self {
            t: MicrofacetDistributionType::TrowbridgeReitz,
            data: TrowbridgeReitz::new_data(alpha_x, alpha_y),
        }
    }
}

impl MicrofacetDistribution for EnumMicrofacetDistribution {
    fn d(&self, wh: Vec3A) -> f32 {
        match self.t {
            MicrofacetDistributionType::TrowbridgeReitz => {
                TrowbridgeReitz { data: &self.data }.d(wh)
            }
        }
    }

    fn lambda(&self, w: Vec3A) -> f32 {
        match self.t {
            MicrofacetDistributionType::TrowbridgeReitz => {
                TrowbridgeReitz { data: &self.data }.lambda(w)
            }
        }
    }

    fn sample_wh(&self, wo: Vec3A, rng: &mut DefaultRng) -> Vec3A {
        match self.t {
            MicrofacetDistributionType::TrowbridgeReitz => {
                TrowbridgeReitz { data: &self.data }.sample_wh(wo, rng)
            }
        }
    }

    fn pdf(&self, wo: Vec3A, wh: Vec3A) -> f32 {
        match self.t {
            MicrofacetDistributionType::TrowbridgeReitz => {
                TrowbridgeReitz { data: &self.data }.pdf(wo, wh)
            }
        }
    }

    fn g(&self, wi: Vec3A, wo: Vec3A) -> f32 {
        match self.t {
            MicrofacetDistributionType::TrowbridgeReitz => {
                TrowbridgeReitz { data: &self.data }.g(wo, wi)
            }
        }
    }

    fn g1(&self, w: Vec3A) -> f32 {
        match self.t {
            MicrofacetDistributionType::TrowbridgeReitz => {
                TrowbridgeReitz { data: &self.data }.g1(w)
            }
        }
    }
}
