use spirv_std::glam::{vec3a, Vec3A, Vec4, Vec4Swizzles};

pub trait AreaLight {
    fn emit(&self, wo: Vec3A, normal: Vec3A) -> Vec3A;
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
    pub fn is_null(&self) -> bool {
        self.t == 0
    }

    pub fn new_null() -> Self {
        Self {
            t: 0,
            data: EnumAreaLightData { v0: Vec4::ZERO },
        }
    }
    pub fn new_diffuse(color: Vec3A) -> Self {
        Self {
            t: 1,
            data: EnumAreaLightData {
                v0: color.extend(0.0),
            },
        }
    }
}

impl<'a> AreaLight for Diffuse<'a> {
    fn emit(&self, wo: Vec3A, normal: Vec3A) -> Vec3A {
        if wo.dot(normal) > 0.0 {
            self.data.v0.xyz().into()
        } else {
            Vec3A::ZERO
        }
    }
}

impl AreaLight for EnumAreaLight {
    fn emit(&self, wo: Vec3A, normal: Vec3A) -> Vec3A {
        match self.t {
            0 => vec3a(0.0, 0.0, 0.0),
            _ => Diffuse { data: &self.data }.emit(wo, normal),
        }
    }
}
