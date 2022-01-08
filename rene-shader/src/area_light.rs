use spirv_std::glam::{Vec3A, Vec4, Vec4Swizzles};

use crate::RayPayload;

pub trait AreaLight {
    fn emit(&self, payload: &RayPayload) -> Vec3A;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumAreaLight {
    t: u32,
    data: EnumAreaLightData,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumAreaLightData {
    v0: Vec4,
}

struct Diffuse<'a> {
    data: &'a EnumAreaLightData,
}

impl EnumAreaLight {
    pub fn new_null() -> Self {
        Self {
            t: 0,
            data: EnumAreaLightData { v0: Vec4::ZERO },
        }
    }
    pub fn new_diffuse(color: Vec3A) -> Self {
        Self {
            t: 0,
            data: EnumAreaLightData {
                v0: color.extend(0.0),
            },
        }
    }
}

impl<'a> AreaLight for Diffuse<'a> {
    fn emit(&self, payload: &RayPayload) -> Vec3A {
        if payload.front_face != 0 {
            self.data.v0.xyz().into()
        } else {
            Vec3A::ZERO
        }
    }
}

impl AreaLight for EnumAreaLight {
    fn emit(&self, payload: &RayPayload) -> Vec3A {
        match self.t {
            _ => Diffuse { data: &self.data }.emit(payload),
        }
    }
}
