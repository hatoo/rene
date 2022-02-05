use spirv_std::glam::{vec3a, Vec3A, Vec4, Vec4Swizzles};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::asm::f32_clamp;

use super::Packed4;

pub trait Fresnel {
    fn evaluate(&self, cos_i: f32) -> Vec3A;
}

#[repr(u32)]
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
enum FresnelType {
    FresnelConductor,
}

impl Default for FresnelType {
    fn default() -> Self {
        Self::FresnelConductor
    }
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumFresnelData {
    v0: Packed4<FresnelType>,
    v1: Vec4,
    v2: Vec4,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumFresnel {
    data: EnumFresnelData,
}

impl Default for EnumFresnel {
    fn default() -> Self {
        Self {
            data: Default::default(),
        }
    }
}

struct FresnelConductor<'a> {
    data: &'a EnumFresnelData,
}

impl<'a> FresnelConductor<'a> {
    fn new_data(eta_i: Vec3A, eta_t: Vec3A, k: Vec3A) -> EnumFresnelData {
        EnumFresnelData {
            v0: Packed4 {
                t: FresnelType::FresnelConductor,
                x: eta_i.x,
                y: eta_i.y,
                z: eta_i.z,
            },
            v1: eta_t.extend(0.0),
            v2: k.extend(0.0),
        }
    }

    fn eta_i(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }

    fn eta_t(&self) -> Vec3A {
        self.data.v1.xyz().into()
    }

    fn k(&self) -> Vec3A {
        self.data.v2.xyz().into()
    }
}

fn fr_conductor(cos_theta_i: f32, eta_i: Vec3A, eta_t: Vec3A, k: Vec3A) -> Vec3A {
    let cos_theta_i = f32_clamp(cos_theta_i, -1.0, 1.0);
    let eta = eta_t / eta_i;
    let eta_k = k / eta_i;

    let cos_theta_i2 = cos_theta_i * cos_theta_i;
    let sin_theta_i2 = 1.0 - cos_theta_i2;
    let eta2 = eta * eta;
    let eta_k2 = eta_k * eta_k;

    let t0 = eta2 - eta_k2 - sin_theta_i2;
    let a2plusb2 = t0 * t0 + 4.0 * eta2 * eta_k2;
    let a2plusb2 = vec3a(a2plusb2.x.sqrt(), a2plusb2.y.sqrt(), a2plusb2.z.sqrt());
    let t1 = a2plusb2 + cos_theta_i2;
    let a = 0.5 * (a2plusb2 + t0);
    let a = vec3a(a.x.sqrt(), a.y.sqrt(), a.z.sqrt());
    let t2 = 2.0 * cos_theta_i * a;
    let rs = (t1 - t2) / (t1 + t2);

    let t3 = cos_theta_i2 * a2plusb2 + sin_theta_i2 * sin_theta_i2;
    let t4 = t2 * sin_theta_i2;
    let rp = rs * (t3 - t4) / (t3 + t4);

    0.5 * (rp + rs)
}

impl<'a> Fresnel for FresnelConductor<'a> {
    fn evaluate(&self, cos_i: f32) -> Vec3A {
        fr_conductor(cos_i.abs(), self.eta_i(), self.eta_t(), self.k())
    }
}

impl EnumFresnel {
    pub fn new_fresnel_conductor(eta_i: Vec3A, eta_t: Vec3A, k: Vec3A) -> Self {
        Self {
            data: FresnelConductor::new_data(eta_i, eta_t, k),
        }
    }
}

impl Fresnel for EnumFresnel {
    fn evaluate(&self, cos_i: f32) -> Vec3A {
        match self.data.v0.t {
            FresnelType::FresnelConductor => FresnelConductor { data: &self.data }.evaluate(cos_i),
        }
    }
}
