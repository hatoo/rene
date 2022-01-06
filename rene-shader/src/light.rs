use spirv_std::glam::{Vec3A, Vec4, Vec4Swizzles};

pub trait Light {
    fn position(&self) -> Vec3A;
    fn color(&self, position: Vec3A) -> Vec3A;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumLightData {
    v0: Vec4,
    v1: Vec4,
    v2: Vec4,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumLight {
    t: u32,
    data: EnumLightData,
}

impl EnumLight {
    pub fn new_distant(from: Vec3A, to: Vec3A, color: Vec3A) -> Self {
        Self {
            t: 0,
            data: EnumLightData {
                v0: from.extend(0.0),
                v1: to.extend(0.0),
                v2: color.extend(0.0),
            },
        }
    }
}

struct Distant<'a> {
    data: &'a EnumLightData,
}

impl<'a> Light for Distant<'a> {
    fn position(&self) -> Vec3A {
        self.data.v0.xyz().into()
    }

    fn color(&self, position: Vec3A) -> Vec3A {
        let v1 = Vec3A::from(self.data.v1.xyz() - self.data.v0.xyz()).normalize();
        let v2 = (position - Vec3A::from(self.data.v0.xyz())).normalize();

        if v1.dot(v2) > 0.0 {
            self.data.v2.xyz().into()
        } else {
            Vec3A::ZERO
        }
    }
}

impl Light for EnumLight {
    fn position(&self) -> Vec3A {
        Distant { data: &self.data }.position()
    }

    fn color(&self, position: Vec3A) -> Vec3A {
        Distant { data: &self.data }.color(position)
    }
}
