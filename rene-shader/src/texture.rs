#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    glam::{uvec4, vec2, vec3a, vec4, UVec4, Vec2, Vec3A, Vec4, Vec4Swizzles},
    RuntimeArray,
};

use crate::InputImage;

trait Texture {
    fn color(&self, images: &RuntimeArray<InputImage>, uv: Vec2) -> ColorOrTarget;
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumTextureData {
    u0: UVec4,
    v0: Vec4,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumTexture {
    t: u32,
    data: EnumTextureData,
}

struct Solid<'a> {
    data: &'a EnumTextureData,
}

struct ImageMap<'a> {
    data: &'a EnumTextureData,
}

struct CheckerBoard<'a> {
    data: &'a EnumTextureData,
}

struct ColorOrTarget {
    t: bool,
    index: u32,
    color_or_uv: Vec3A,
}

impl<'a> Texture for CheckerBoard<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, uv: Vec2) -> ColorOrTarget {
        let w = self.data.v0.x;
        let h = self.data.v0.y;

        let tex1 = self.data.u0.x;
        let tex2 = self.data.u0.y;

        let x = uv.x * w;
        let y = uv.y * h;

        if (x as u32 % 2 == 0) == (y as u32 % 2 == 0) {
            ColorOrTarget {
                t: true,
                index: tex1,
                color_or_uv: vec3a(x.fract(), y.fract(), 0.0),
            }
        } else {
            ColorOrTarget {
                t: true,
                index: tex2,
                color_or_uv: vec3a(x.fract(), y.fract(), 0.0),
            }
        }
    }
}

impl<'a> Texture for ImageMap<'a> {
    fn color(&self, images: &RuntimeArray<InputImage>, uv: Vec2) -> ColorOrTarget {
        let image = unsafe { images.index(self.data.u0.x as usize) };
        let color: Vec4 = unsafe { image.sample_by_lod(uv, 0.0) };
        ColorOrTarget {
            t: false,
            index: 0,
            color_or_uv: color.xyz().into(),
        }
    }
}

impl<'a> Texture for Solid<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, _uv: Vec2) -> ColorOrTarget {
        ColorOrTarget {
            t: false,
            index: 0,
            color_or_uv: self.data.v0.xyz().into(),
        }
    }
}

impl EnumTexture {
    pub fn new_solid(color: Vec3A) -> Self {
        Self {
            t: 0,
            data: EnumTextureData {
                u0: UVec4::ZERO,
                v0: vec4(color.x, color.y, color.z, 0.0),
            },
        }
    }

    pub fn new_checkerboard(tex1: u32, tex2: u32, uscale: f32, vscale: f32) -> Self {
        Self {
            t: 1,
            data: EnumTextureData {
                u0: uvec4(tex1, tex2, 0, 0),
                v0: vec4(uscale, vscale, 0.0, 0.0),
            },
        }
    }

    pub fn new_image_map(image: u32) -> Self {
        Self {
            t: 2,
            data: EnumTextureData {
                u0: uvec4(image, 0, 0, 0),
                v0: Vec4::ZERO,
            },
        }
    }
}

impl EnumTexture {
    pub fn color(
        &self,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        uv: Vec2,
    ) -> Vec3A {
        let mut color_or_target = match self.t {
            0 => Solid { data: &self.data }.color(images, uv),
            1 => CheckerBoard { data: &self.data }.color(images, uv),
            _ => ImageMap { data: &self.data }.color(images, uv),
        };

        while color_or_target.t {
            let tex = textures[color_or_target.index as usize];
            color_or_target = match tex.t {
                0 => Solid { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
                1 => CheckerBoard { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
                _ => ImageMap { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
            };
        }

        color_or_target.color_or_uv
    }
}
