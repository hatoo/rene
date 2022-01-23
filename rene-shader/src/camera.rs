use spirv_std::glam::{vec3a, Mat4, Vec2, Vec3A};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::math::random_in_unit_disk;
use crate::rand::DefaultRng;
use crate::Ray;

#[derive(Copy, Clone)]
pub struct Camera {
    origin: Vec3A,
    lower_left_corner: Vec3A,
    horizontal: Vec3A,
    vertical: Vec3A,
    u: Vec3A,
    v: Vec3A,
    // w: Vec3,
    lens_radius: f32,
}

impl Camera {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        look_from: Vec3A,
        look_at: Vec3A,
        vup: Vec3A,
        vfov: f32,
        aspect_ratio: f32,
        aperture: f32,
        focus_dist: f32,
    ) -> Self {
        let theta = vfov;
        let h = (theta / 2.0).tan();
        let viewport_height = 2.0 * h;
        let viewport_width = aspect_ratio * viewport_height;

        let w = (look_from - look_at).normalize();
        let u = vup.cross(w).normalize();
        let v = w.cross(u);

        let origin = look_from;
        let horizontal = focus_dist * viewport_width * u;
        let vertical = focus_dist * viewport_height * v;
        let lower_left_corner = origin - horizontal / 2.0 - vertical / 2.0 - focus_dist * w;

        Self {
            origin,
            lower_left_corner,
            horizontal,
            vertical,
            u,
            v,
            // w,
            lens_radius: aperture / 2.0,
        }
    }

    pub fn get_ray(&self, s: f32, t: f32, rng: &mut DefaultRng) -> Ray {
        let rd = self.lens_radius * random_in_unit_disk(rng);
        let offset = self.u * rd.x + self.v * rd.y;

        Ray {
            origin: self.origin + offset,
            direction: (self.lower_left_corner + s * self.horizontal + t * self.vertical
                - self.origin
                - offset),
        }
    }
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct PerspectiveCamera {
    pub projection: Mat4,
}

impl PerspectiveCamera {
    pub fn get_ray(&self, st: Vec2, camera_to_world: Mat4) -> Ray {
        let origin = camera_to_world.transform_point3a(vec3a(0.0, 0.0, 0.0));
        let target =
            self.projection
                .transform_point3a(vec3a(st.x * 2.0 - 1.0, st.y * 2.0 - 1.0, 1.0));
        let target = camera_to_world.transform_point3a(target);

        Ray {
            origin,
            direction: (target - origin).normalize(),
        }
    }
}
