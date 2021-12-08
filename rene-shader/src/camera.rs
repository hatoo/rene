use spirv_std::glam::{UVec3, Vec2, Vec3A};
#[allow(unused_imports)]
use spirv_std::num_traits::Float;

use crate::math::random_in_unit_disk;
use crate::rand::DefaultRng;
use crate::{LookAt, Ray};

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
pub struct PerspectiveCamera {
    pub fov: f32,
}

impl PerspectiveCamera {
    pub fn get_ray(&self, size: UVec3, st: Vec2, look_at: &LookAt) -> Ray {
        let theta = self.fov / 180.0 * core::f32::consts::PI;
        let (viewport_width, viewport_height) = if size.x < size.y {
            let w = (theta / 2.0).tan();
            let viewport_width = 2.0 * w;
            let viewport_height = viewport_width * size.y as f32 / size.x as f32;
            (viewport_width, viewport_height)
        } else {
            let h = (theta / 2.0).tan();
            let viewport_height = 2.0 * h;
            let viewport_width = viewport_height * size.x as f32 / size.y as f32;
            (viewport_width, viewport_height)
        };

        let w = (look_at.eye - look_at.look_at).normalize();
        let u = look_at.up.cross(w).normalize();
        let v = w.cross(u);

        let origin = look_at.eye;
        let horizontal = viewport_width * u;
        let vertical = viewport_height * v;
        let lower_left_corner = -horizontal / 2.0 - vertical / 2.0 - w;

        Ray {
            origin,
            direction: lower_left_corner + st.x * horizontal + st.y * vertical,
        }
    }
}
