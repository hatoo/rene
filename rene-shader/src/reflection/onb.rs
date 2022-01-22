use spirv_std::glam::{vec3a, Vec3A};

pub struct Onb {
    u: Vec3A,
    v: Vec3A,
    w: Vec3A,
}

impl Onb {
    pub fn from_w(w: Vec3A) -> Self {
        let a = if w.x > 0.9 {
            vec3a(0.0, 1.0, 0.0)
        } else {
            vec3a(1.0, 0.0, 0.0)
        };
        let v = w.cross(a).normalize();
        let u = w.cross(v);

        Self { u, v, w }
    }

    pub fn local(&self, v: Vec3A) -> Vec3A {
        v.x * self.u + v.y * self.v + v.z * self.w
    }
}
