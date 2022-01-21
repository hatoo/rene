use spirv_std::glam::{Vec3A, Vec4, Vec4Swizzles};

pub trait Light {
    fn ray_target(&self, position: Vec3A) -> (Vec3A, f32);
    fn color(&self, position: Vec3A) -> Vec3A;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumLightData {
    v0: Vec4,
    v1: Vec4,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(u32)]
enum LightType {
    Distant,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumLight {
    t: LightType,
    data: EnumLightData,
}

impl EnumLight {
    pub fn new_distant(from: Vec3A, to: Vec3A, color: Vec3A) -> Self {
        Self {
            t: LightType::Distant,
            data: EnumLightData {
                v0: (from - to).normalize().extend(0.0),
                v1: color.extend(0.0),
            },
        }
    }
}

struct Distant<'a> {
    data: &'a EnumLightData,
}

impl<'a> Light for Distant<'a> {
    fn ray_target(&self, position: Vec3A) -> (Vec3A, f32) {
        (position + Vec3A::from(self.data.v0.xyz()), 1e5)
    }

    fn color(&self, _position: Vec3A) -> Vec3A {
        self.data.v1.xyz().into()
    }
}

impl Light for EnumLight {
    fn ray_target(&self, position: Vec3A) -> (Vec3A, f32) {
        match self.t {
            LightType::Distant => Distant { data: &self.data }.ray_target(position),
        }
    }

    fn color(&self, position: Vec3A) -> Vec3A {
        match self.t {
            LightType::Distant => Distant { data: &self.data }.color(position),
        }
    }
}
