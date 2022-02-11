#![cfg_attr(
    target_arch = "spirv",
    no_std,
    feature(register_attr, asm_experimental_arch),
    register_attr(spirv)
)]

use crate::rand::DefaultRng;
use area_light::{AreaLight, EnumAreaLight};
use camera::PerspectiveCamera;
use core::f32::consts::{FRAC_1_PI, PI};
use light::{EnumLight, Light};
use material::{EnumMaterial, Material};
use math::sphere_uv;
use medium::{EnumMedium, Medium};
use reflection::{onb::Onb, Bsdf, BxdfKind};
#[cfg(not(target_arch = "spirv"))]
use spirv_std::macros::spirv;
use surface_sample::{EnumSurfaceSample, SurfaceSample};
use texture::EnumTexture;

#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::{report_intersection, IndexUnchecked},
    glam::{uvec2, vec2, vec3a, Mat4, UVec3, Vec2, Vec3A, Vec4, Vec4Swizzles},
    image::{Image, SampledImage},
    ray_tracing::{AccelerationStructure, RayFlags},
    RuntimeArray,
};

pub mod area_light;
mod asm;
pub mod camera;
pub mod light;
pub mod material;
pub mod math;
pub mod medium;
pub mod rand;
pub mod reflection;
pub mod surface_sample;
pub mod texture;

pub type InputImage = SampledImage<Image!(2D, format=rgba32f, sampled=true)>;

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct Ray {
    pub origin: Vec3A,
    pub direction: Vec3A,
}
#[derive(Clone, Default)]
pub struct RayPayload {
    pub is_miss: u32,
    pub t: f32,
    pub position: Vec3A,
    pub normal: Vec3A,
    pub material: u32,
    pub area_light: u32,
    pub uv: Vec2,
    pub medium: u32,
}

impl RayPayload {
    /*
    pub fn new_miss(color: Vec3A) -> Self {
        Self {
            is_miss: 1,
            position: color,
            ..Default::default()
        }
    }
    */

    pub fn set_miss(&mut self, color: Vec3A) {
        self.is_miss = 1;
        self.position = color;
    }

    pub fn new_hit(
        t: f32,
        position: Vec3A,
        normal: Vec3A,
        material: u32,
        area_light: u32,
        uv: Vec2,
        medium: u32,
    ) -> Self {
        Self {
            is_miss: 0,
            t,
            position,
            normal,
            material,
            area_light,
            uv,
            medium,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct Uniform {
    pub camera_to_world: Mat4,
    pub background_matrix: Mat4,
    pub background_color: Vec4,
    pub background_texture: u32,
    pub camera: PerspectiveCamera,
    pub lights_len: u32,
    pub emit_object_len: u32,
    pub emit_primitives: u32,
}

pub struct PushConstants {
    seed: u32,
}

#[derive(Copy, Clone)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct IndexData {
    pub material_index: u32,
    pub area_light_index: u32,
    pub index_offset: u32,
    pub primitive_count: u32,
    pub medium_index: u32,
}

#[spirv(miss)]
pub fn main_miss(
    #[spirv(incoming_ray_payload)] out: &mut RayPayload,
    #[spirv(world_ray_direction)] ray_direction: Vec3A,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] uniform: &Uniform,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 7)] textures: &[EnumTexture],
    #[spirv(descriptor_set = 0, binding = 8)] images: &RuntimeArray<InputImage>,
) {
    let uv = sphere_uv(
        uniform
            .background_matrix
            .transform_vector3a(ray_direction)
            .normalize(),
    );
    let color = Vec3A::from(uniform.background_color.xyz())
        * unsafe { textures.index_unchecked(uniform.background_texture as usize) }
            .color(textures, images, uv);

    out.set_miss(color);
}

#[spirv(ray_generation)]
#[allow(clippy::too_many_arguments)]
pub fn main_ray_generation_path(
    #[spirv(launch_id)] launch_id: UVec3,
    #[spirv(launch_size)] launch_size: UVec3,
    #[spirv(push_constant)] constants: &PushConstants,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] uniform: &Uniform,
    #[spirv(descriptor_set = 0, binding = 1)] tlases: &RuntimeArray<AccelerationStructure>,
    #[spirv(descriptor_set = 0, binding = 2)] image: &Image!(2D, format=rgba32f, sampled=false, arrayed=true),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] lights: &[EnumLight],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] area_lights: &[EnumAreaLight],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] emit_objects: &[EnumSurfaceSample],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 6)] materials: &[EnumMaterial],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 7)] textures: &[EnumTexture],
    #[spirv(descriptor_set = 0, binding = 8)] images: &RuntimeArray<InputImage>,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] indices: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 11)] vertices: &[Vertex],
    #[spirv(ray_payload)] payload: &mut RayPayload,
    #[spirv(ray_payload)] payload_pdf: &mut RayPayloadPDF,
) {
    let tlas_main = unsafe { tlases.index(0) };
    let tlas_emit = unsafe { tlases.index(1) };

    let add_image = |i: u32, v: Vec3A| {
        let pos = uvec2(launch_id.x, launch_size.y - 1 - launch_id.y).extend(i);
        let prev: Vec4 = image.read(pos);

        unsafe {
            image.write(pos, prev + v.extend(0.0));
        }
    };

    let rand_seed = (launch_id.y * launch_size.x + launch_id.x) ^ constants.seed;
    let mut rng = DefaultRng::new(rand_seed);
    let mut frame_wide_rng = DefaultRng::new(constants.seed);

    let u = (launch_id.x as f32 + rng.next_f32()) / (launch_size.x - 1) as f32;
    let v = (launch_id.y as f32 + rng.next_f32()) / (launch_size.y - 1) as f32;

    let cull_mask = 0xff;
    let tmin = 0.001;
    let tmax = 100000.0;

    let mut bsdf = Bsdf::default();

    let mut color = vec3a(1.0, 1.0, 1.0);

    let mut ray = uniform.camera.get_ray(vec2(u, v), uniform.camera_to_world);

    let mut i = 0;
    while i < 50 {
        *payload = RayPayload::default();
        unsafe {
            tlas_main.trace_ray(
                RayFlags::OPAQUE,
                cull_mask,
                0,
                0,
                0,
                ray.origin,
                tmin,
                ray.direction,
                tmax,
                payload,
            );
        }

        if payload.is_miss != 0 {
            add_image(0, color * payload.position);
            break;
        } else {
            let wo = -ray.direction.normalize();
            let normal = payload.normal.normalize();
            let position = payload.position;
            let uv = payload.uv;
            let material = unsafe { materials.index_unchecked(payload.material as usize) };
            let area_light = unsafe { area_lights.index_unchecked(payload.area_light as usize) };

            bsdf.clear(normal, Onb::from_w(normal));
            material.compute_bsdf(&mut bsdf, uv, textures, images);

            if !area_light.is_null() {
                add_image(0, color * area_light.emit(wo, normal));
            }

            if i == 0 {
                add_image(1, normal);
                add_image(2, material.albedo(uv, textures, images));
            }

            let mut l = 0;
            while l < uniform.lights_len {
                let (target, t_max) =
                    unsafe { lights.index_unchecked(l as usize) }.ray_target(position);
                let wi = (target - position).normalize();
                let light_ray = Ray {
                    origin: position,
                    direction: wi,
                };

                *payload = RayPayload::default();
                unsafe {
                    tlas_main.trace_ray(
                        RayFlags::empty(),
                        cull_mask,
                        0,
                        0,
                        0,
                        light_ray.origin,
                        tmin,
                        light_ray.direction,
                        t_max,
                        payload,
                    );
                }

                if payload.is_miss != 0 {
                    let f = bsdf.f(wo, wi);

                    add_image(
                        0,
                        color
                            * f
                            * wi.dot(normal).abs()
                            * unsafe { lights.index_unchecked(l as usize) }.color(position),
                    );
                }
                l += 1;
            }

            if uniform.emit_object_len > 0 && bsdf.contains(BxdfKind::DIFFUSE) {
                // Use frame wide RNG to reduce warp divergence
                let (wi, pdf, f) = if frame_wide_rng.next_f32() > 0.5 {
                    let emit_object = unsafe {
                        emit_objects.index_unchecked(
                            (frame_wide_rng.next_u32() % uniform.emit_object_len) as usize,
                        )
                    };

                    let wi = (emit_object.sample(indices, vertices, &mut frame_wide_rng)
                        - position)
                        .normalize();

                    (wi, bsdf.pdf(wi, normal), bsdf.f(wo, wi))
                } else {
                    let sampled_f = bsdf.sample_f(wo, &mut rng);

                    (sampled_f.wi, sampled_f.pdf, sampled_f.f)
                };

                ray = Ray {
                    origin: position,
                    direction: wi,
                };

                *payload_pdf = RayPayloadPDF::default();

                unsafe {
                    tlas_emit.trace_ray(
                        RayFlags::OPAQUE,
                        cull_mask,
                        2,
                        0,
                        1,
                        ray.origin,
                        tmin,
                        ray.direction,
                        tmax,
                        payload_pdf,
                    );
                }

                color *= f * normal.dot(wi).abs();

                let pdf = 0.5 * pdf + 0.5 * payload_pdf.pdf / uniform.emit_object_len as f32;

                if pdf < 1e-5 {
                    break;
                }

                color /= pdf;
            } else {
                let sampled_f = bsdf.sample_f(wo, &mut rng);

                if sampled_f.pdf < 1e-5 {
                    break;
                }

                color *= sampled_f.f * normal.dot(sampled_f.wi).abs() / sampled_f.pdf;
                ray = Ray {
                    origin: position,
                    direction: sampled_f.wi,
                };
            }
        }

        if color == Vec3A::ZERO {
            break;
        }

        // russian roulette
        if i > 12 {
            let rr_coin = frame_wide_rng.next_f32();
            let continue_p = color.max_element();

            if rr_coin > continue_p {
                break;
            } else {
                color /= continue_p;
            }
        }
        i += 1;
    }
}

#[inline(always)]
fn tr(
    tlas_main: &AccelerationStructure,
    mut ray: Ray,
    mut medium: EnumMedium,
    mediums: &[EnumMedium],
    payload: &mut RayPayload,
) -> Vec3A {
    let mut tr = vec3a(1.0, 1.0, 1.0);

    loop {
        *payload = RayPayload::default();

        unsafe {
            tlas_main.trace_ray(
                RayFlags::empty(),
                0xff,
                0,
                0,
                0,
                ray.origin,
                0.001,
                ray.direction,
                1e5,
                payload,
            );
        }

        if payload.is_miss != 0 {
            break tr;
        } else if !mediums[payload.medium as usize].is_vaccum() {
            break Vec3A::ZERO;
        } else {
            if medium.is_vaccum() {
                medium = mediums[payload.medium as usize];
            } else {
                tr *= medium.tr(ray, payload.t);
                medium = EnumMedium::default();
            }

            ray.origin = payload.position;
        }
    }
}

#[spirv(ray_generation)]
#[allow(clippy::too_many_arguments)]
pub fn main_ray_generation_volpath(
    #[spirv(launch_id)] launch_id: UVec3,
    #[spirv(launch_size)] launch_size: UVec3,
    #[spirv(push_constant)] constants: &PushConstants,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] uniform: &Uniform,
    #[spirv(descriptor_set = 0, binding = 1)] tlases: &RuntimeArray<AccelerationStructure>,
    #[spirv(descriptor_set = 0, binding = 2)] image: &Image!(2D, format=rgba32f, sampled=false, arrayed=true),
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] lights: &[EnumLight],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] area_lights: &[EnumAreaLight],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] emit_objects: &[EnumSurfaceSample],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 6)] materials: &[EnumMaterial],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 7)] textures: &[EnumTexture],
    #[spirv(descriptor_set = 0, binding = 8)] images: &RuntimeArray<InputImage>,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] indices: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 11)] vertices: &[Vertex],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 12)] mediums: &[EnumMedium],
    #[spirv(ray_payload)] payload: &mut RayPayload,
    #[spirv(ray_payload)] payload_pdf: &mut RayPayloadPDF,
) {
    let tlas_main = unsafe { tlases.index(0) };
    let tlas_emit = unsafe { tlases.index(1) };

    let add_image = |i: u32, v: Vec3A| {
        let pos = uvec2(launch_id.x, launch_size.y - 1 - launch_id.y).extend(i);
        let prev: Vec4 = image.read(pos);

        unsafe {
            image.write(pos, prev + v.extend(0.0));
        }
    };

    let rand_seed = (launch_id.y * launch_size.x + launch_id.x) ^ constants.seed;
    let mut rng = DefaultRng::new(rand_seed);
    let mut frame_wide_rng = DefaultRng::new(constants.seed);

    let u = (launch_id.x as f32 + rng.next_f32()) / (launch_size.x - 1) as f32;
    let v = (launch_id.y as f32 + rng.next_f32()) / (launch_size.y - 1) as f32;

    let cull_mask = 0xff;
    let tmin = 0.001;
    let tmax = 100000.0;

    let mut bsdf = Bsdf::default();

    let mut color = vec3a(1.0, 1.0, 1.0);

    let mut ray = uniform.camera.get_ray(vec2(u, v), uniform.camera_to_world);

    let mut medium = EnumMedium::new_vaccum();

    let mut i = 0;
    while i < 50 {
        if uniform.lights_len > 0 && !medium.is_vaccum() {
            let sampled_medium = medium.sample(ray, tmax, &mut rng);

            color *= sampled_medium.tr;

            if sampled_medium.sampled {
                ray.origin = sampled_medium.position;

                let mut l = 0;
                while l < uniform.lights_len {
                    let (target, _t_max) =
                        unsafe { lights.index_unchecked(l as usize) }.ray_target(ray.origin);
                    let wi = (target - ray.origin).normalize();
                    let light_ray = Ray {
                        origin: ray.origin,
                        direction: wi,
                    };

                    let tr = tr(tlas_main, light_ray, medium, mediums, payload);
                    add_image(
                        0,
                        color
                            * tr
                            * medium.phase(-ray.direction.normalize(), wi)
                            * unsafe { lights.index_unchecked(l as usize) }.color(ray.origin),
                    );
                    l += 1;
                }
            }
        }

        *payload = RayPayload::default();
        unsafe {
            tlas_main.trace_ray(
                RayFlags::OPAQUE,
                cull_mask,
                0,
                0,
                0,
                ray.origin,
                tmin,
                ray.direction,
                tmax,
                payload,
            );
        }

        if payload.is_miss != 0 {
            add_image(0, color * payload.position);
            break;
        } else {
            let wo = -ray.direction.normalize();
            let normal = payload.normal.normalize();
            let position = payload.position;
            let uv = payload.uv;
            let material = unsafe { materials.index_unchecked(payload.material as usize) };
            let area_light = unsafe { area_lights.index_unchecked(payload.area_light as usize) };

            bsdf.clear(normal, Onb::from_w(normal));
            material.compute_bsdf(&mut bsdf, uv, textures, images);

            if !area_light.is_null() {
                add_image(0, color * area_light.emit(wo, normal));
            }

            if i == 0 {
                add_image(1, normal);
                add_image(2, material.albedo(uv, textures, images));
            }

            let mut l = 0;
            while l < uniform.lights_len {
                let (target, t_max) =
                    unsafe { lights.index_unchecked(l as usize) }.ray_target(position);
                let wi = (target - position).normalize();
                let light_ray = Ray {
                    origin: position,
                    direction: wi,
                };

                *payload = RayPayload::default();
                unsafe {
                    tlas_main.trace_ray(
                        RayFlags::empty(),
                        cull_mask,
                        0,
                        0,
                        0,
                        light_ray.origin,
                        tmin,
                        light_ray.direction,
                        t_max,
                        payload,
                    );
                }

                if payload.is_miss != 0 {
                    let f = bsdf.f(wo, wi);

                    add_image(
                        0,
                        color
                            * f
                            * wi.dot(normal).abs()
                            * unsafe { lights.index_unchecked(l as usize) }.color(position),
                    );
                }
                l += 1;
            }

            if uniform.emit_object_len > 0 && bsdf.contains(BxdfKind::DIFFUSE) {
                // Use frame wide RNG to reduce warp divergence
                let (wi, pdf, f) = if frame_wide_rng.next_f32() > 0.5 {
                    let emit_object = unsafe {
                        emit_objects.index_unchecked(
                            (frame_wide_rng.next_u32() % uniform.emit_object_len) as usize,
                        )
                    };

                    let wi = (emit_object.sample(indices, vertices, &mut frame_wide_rng)
                        - position)
                        .normalize();

                    (wi, bsdf.pdf(wi, normal), bsdf.f(wo, wi))
                } else {
                    let sampled_f = bsdf.sample_f(wo, &mut rng);

                    (sampled_f.wi, sampled_f.pdf, sampled_f.f)
                };

                ray = Ray {
                    origin: position,
                    direction: wi,
                };

                *payload_pdf = RayPayloadPDF::default();

                unsafe {
                    tlas_emit.trace_ray(
                        RayFlags::OPAQUE,
                        cull_mask,
                        2,
                        0,
                        1,
                        ray.origin,
                        tmin,
                        ray.direction,
                        tmax,
                        payload_pdf,
                    );
                }

                color *= f * normal.dot(wi).abs();

                let pdf = 0.5 * pdf + 0.5 * payload_pdf.pdf / uniform.emit_object_len as f32;

                if pdf < 1e-5 {
                    break;
                }

                color /= pdf;
            } else {
                let sampled_f = bsdf.sample_f(wo, &mut rng);

                if sampled_f.pdf < 1e-5 {
                    break;
                }

                color *= sampled_f.f * normal.dot(sampled_f.wi).abs() / sampled_f.pdf;
                ray = Ray {
                    origin: position,
                    direction: sampled_f.wi,
                };
            }
        }

        if color == Vec3A::ZERO {
            break;
        }

        // russian roulette
        if i > 12 {
            let rr_coin = frame_wide_rng.next_f32();
            let continue_p = color.max_element();

            if rr_coin > continue_p {
                break;
            } else {
                color /= continue_p;
            }
        }
        i += 1;
    }
}

#[spirv(intersection)]
pub fn sphere_intersection(
    #[spirv(object_ray_origin)] ray_origin: Vec3A,
    #[spirv(object_ray_direction)] ray_direction: Vec3A,
    #[spirv(ray_tmin)] t_min: f32,
    #[spirv(ray_tmax)] t_max: f32,
) {
    let oc = ray_origin;
    let a = ray_direction.length_squared();
    let half_b = oc.dot(ray_direction);
    let c = oc.length_squared() - 1.0;

    let discriminant = half_b * half_b - a * c;
    if discriminant < 0.0 {
        return;
    }

    let sqrtd = discriminant.sqrt();

    let root0 = (-half_b - sqrtd) / a;
    let root1 = (-half_b + sqrtd) / a;

    if root0 >= t_min && root0 <= t_max {
        unsafe {
            report_intersection(root0, 0);
        }
        return;
    }

    if root1 >= t_min && root1 <= t_max {
        unsafe {
            report_intersection(root1, 0);
        }
    }
}

#[derive(Clone, Copy)]
#[spirv(matrix)]
#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct Affine3 {
    pub x: Vec3A,
    pub y: Vec3A,
    pub z: Vec3A,
    pub w: Vec3A,
}

#[spirv(closest_hit)]
#[allow(clippy::too_many_arguments)]
pub fn sphere_closest_hit(
    #[spirv(ray_tmax)] t: f32,
    #[spirv(world_to_object)] world_to_object: Affine3,
    #[spirv(object_ray_origin)] object_ray_origin: Vec3A,
    #[spirv(world_ray_origin)] world_ray_origin: Vec3A,
    #[spirv(object_ray_direction)] object_ray_direction: Vec3A,
    #[spirv(world_ray_direction)] world_ray_direction: Vec3A,
    #[spirv(incoming_ray_payload)] out: &mut RayPayload,
    #[spirv(instance_custom_index)] instance_custom_index: u32,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] index_data: &[IndexData],
) {
    let hit_pos = world_ray_origin + t * world_ray_direction;
    let object_hit_pos = object_ray_origin + t * object_ray_direction;

    let phi = object_hit_pos.y.atan2(object_hit_pos.x);
    let phi = if phi < 0.0 { phi + 2.0 * PI } else { phi };
    let theta = object_hit_pos.z.acos();

    let u = phi * FRAC_1_PI * 0.5;
    let v = (theta - PI) * -FRAC_1_PI;

    let normal = vec3a(
        world_to_object.x.dot(object_hit_pos),
        world_to_object.y.dot(object_hit_pos),
        world_to_object.z.dot(object_hit_pos),
    );

    let index = unsafe { index_data.index_unchecked(instance_custom_index as usize) };
    let material_index = index.material_index;
    let area_light_index = index.area_light_index;
    let medium_index = index.medium_index;

    *out = RayPayload::new_hit(
        t,
        hit_pos,
        normal,
        material_index,
        area_light_index,
        vec2(u, v),
        medium_index,
    );
}

#[derive(Copy, Clone)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3A,
    pub normal: Vec3A,
    pub uv: Vec2,
}

#[spirv(closest_hit)]
#[allow(clippy::too_many_arguments)]
pub fn triangle_closest_hit(
    #[spirv(ray_tmax)] t: f32,
    #[spirv(hit_attribute)] attribute: &Vec2,
    #[spirv(object_to_world)] object_to_world: Affine3,
    #[spirv(world_to_object)] world_to_object: Affine3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] index_data: &[IndexData],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] indices: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 11)] vertices: &[Vertex],
    #[spirv(incoming_ray_payload)] out: &mut RayPayload,
    #[spirv(primitive_id)] primitive_id: u32,
    #[spirv(instance_custom_index)] instance_custom_index: u32,
) {
    let index_data = unsafe { index_data.index_unchecked(instance_custom_index as usize) };

    let index_offset = index_data.index_offset as usize;
    let material_index = index_data.material_index;
    let area_light_index = index_data.area_light_index;
    let medium_index = index_data.medium_index;

    let v0 = unsafe {
        vertices.index_unchecked(
            *indices.index_unchecked(index_offset + 3 * primitive_id as usize) as usize,
        )
    };
    let v1 = unsafe {
        vertices.index_unchecked(
            *indices.index_unchecked(index_offset + 3 * primitive_id as usize + 1) as usize,
        )
    };
    let v2 = unsafe {
        vertices.index_unchecked(
            *indices.index_unchecked(index_offset + 3 * primitive_id as usize + 2) as usize,
        )
    };

    let barycentrics = vec3a(1.0 - attribute.x - attribute.y, attribute.x, attribute.y);

    let pos =
        v0.position * barycentrics.x + v1.position * barycentrics.y + v2.position * barycentrics.z;

    let nrm = if v0.normal == Vec3A::ZERO && v1.normal == Vec3A::ZERO && v2.normal == Vec3A::ZERO {
        (v1.position - v0.position).cross(v2.position - v0.position)
    } else {
        v0.normal * barycentrics.x + v1.normal * barycentrics.y + v2.normal * barycentrics.z
    };

    let uv = v0.uv * barycentrics.x + v1.uv * barycentrics.y + v2.uv * barycentrics.z;

    let hit_pos = pos.x * object_to_world.x
        + pos.y * object_to_world.y
        + pos.z * object_to_world.z
        + object_to_world.w;

    let normal = vec3a(
        world_to_object.x.dot(nrm),
        world_to_object.y.dot(nrm),
        world_to_object.z.dot(nrm),
    )
    .normalize();

    *out = RayPayload::new_hit(
        t,
        hit_pos,
        normal,
        material_index,
        area_light_index,
        uv,
        medium_index,
    );
}

#[derive(Default)]
pub struct RayPayloadPDF {
    pdf: f32,
}

#[spirv(miss)]
pub fn main_miss_pdf(#[spirv(incoming_ray_payload)] out: &mut RayPayloadPDF) {
    *out = RayPayloadPDF { pdf: 0.0 };
}

#[spirv(closest_hit)]
#[allow(clippy::too_many_arguments)]
pub fn triangle_closest_hit_pdf(
    #[spirv(hit_attribute)] attribute: &Vec2,
    #[spirv(object_to_world)] object_to_world: Affine3,
    #[spirv(world_to_object)] world_to_object: Affine3,
    #[spirv(world_ray_direction)] world_ray_direction: Vec3A,
    #[spirv(world_ray_origin)] world_ray_origin: Vec3A,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] index_data: &[IndexData],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] indices: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 11)] vertices: &[Vertex],
    #[spirv(primitive_id)] primitive_id: u32,
    #[spirv(instance_custom_index)] instance_custom_index: u32,
    #[spirv(incoming_ray_payload)] out: &mut RayPayloadPDF,
) {
    let index_data = unsafe { index_data.index_unchecked(instance_custom_index as usize) };

    let index_offset = index_data.index_offset as usize;

    let v0 = unsafe {
        vertices.index_unchecked(
            *indices.index_unchecked(index_offset + 3 * primitive_id as usize) as usize,
        )
    };
    let v1 = unsafe {
        vertices.index_unchecked(
            *indices.index_unchecked(index_offset + 3 * primitive_id as usize + 1) as usize,
        )
    };
    let v2 = unsafe {
        vertices.index_unchecked(
            *indices.index_unchecked(index_offset + 3 * primitive_id as usize + 2) as usize,
        )
    };

    let barycentrics = vec3a(1.0 - attribute.x - attribute.y, attribute.x, attribute.y);

    let pos =
        v0.position * barycentrics.x + v1.position * barycentrics.y + v2.position * barycentrics.z;

    let nrm = (v1.position - v0.position).cross(v2.position - v0.position);

    let p0 = v0.position.x * object_to_world.x
        + v0.position.y * object_to_world.y
        + v0.position.z * object_to_world.z
        + object_to_world.w;

    let p1 = v1.position.x * object_to_world.x
        + v1.position.y * object_to_world.y
        + v1.position.z * object_to_world.z
        + object_to_world.w;

    let p2 = v2.position.x * object_to_world.x
        + v2.position.y * object_to_world.y
        + v2.position.z * object_to_world.z
        + object_to_world.w;

    let hit_pos = pos.x * object_to_world.x
        + pos.y * object_to_world.y
        + pos.z * object_to_world.z
        + object_to_world.w;

    let normal = vec3a(
        world_to_object.x.dot(nrm),
        world_to_object.y.dot(nrm),
        world_to_object.z.dot(nrm),
    )
    .normalize();

    let ab = p1 - p0;
    let ac = p2 - p0;

    let area = 0.5 * ab.cross(ac).length();
    let distance_squared = (world_ray_origin - hit_pos).length_squared();
    // let cosine = (-world_ray_direction).normalize().dot(normal).abs();
    // Same Value
    let cosine = world_ray_direction.normalize().dot(normal).abs();

    *out = RayPayloadPDF {
        pdf: distance_squared / (cosine * area) / index_data.primitive_count as f32,
    };
}

#[spirv(closest_hit)]
pub fn sphere_closest_hit_pdf(
    #[spirv(object_to_world)] object_to_world: Affine3,
    #[spirv(world_ray_origin)] world_ray_origin: Vec3A,
    #[spirv(incoming_ray_payload)] out: &mut RayPayloadPDF,
) {
    // TODO
    let radius =
        (object_to_world.x.x.abs() + object_to_world.y.y.abs() + object_to_world.z.z.abs()) / 3.0;
    let center = object_to_world.w;

    let cos_theta_max = (1.0 - radius * radius / (center - world_ray_origin).length_squared())
        .max(0.0)
        .sqrt();
    let solid_angle = 2.0 * PI * (1.0 - cos_theta_max);

    *out = RayPayloadPDF {
        pdf: 1.0 / solid_angle,
    };
}
