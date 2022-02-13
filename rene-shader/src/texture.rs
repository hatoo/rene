#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{uvec4, vec2, vec3a, vec4, UVec4, Vec2, Vec3A, Vec4, Vec4Swizzles},
    RuntimeArray,
};

use crate::{
    asm::{f32_to_u32, fract},
    reflection::Packed4,
    InputImage,
};

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

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(u32)]
enum TextureType {
    Solid,
    CheckerBoard,
    ImageMap,
    Scale,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumTexture {
    t: TextureType,
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

struct Scale<'a> {
    data: &'a EnumTextureData,
}

struct ColorOrTarget {
    color_or_uv: Packed4<u32>,
    scale: Vec3A,
}

impl<'a> Solid<'a> {
    pub fn new_data(color: Vec3A) -> EnumTextureData {
        EnumTextureData {
            u0: UVec4::ZERO,
            v0: vec4(color.x, color.y, color.z, 0.0),
        }
    }
}

impl<'a> CheckerBoard<'a> {
    pub fn new_data(tex1: u32, tex2: u32, uscale: f32, vscale: f32) -> EnumTextureData {
        EnumTextureData {
            u0: uvec4(tex1, tex2, 0, 0),
            v0: vec4(uscale, vscale, 0.0, 0.0),
        }
    }
}

impl<'a> ImageMap<'a> {
    pub fn new_data(image: u32) -> EnumTextureData {
        EnumTextureData {
            u0: uvec4(image, 0, 0, 0),
            v0: Vec4::ZERO,
        }
    }
}

impl<'a> Scale<'a> {
    pub fn new_data(scale: Vec3A, tex: u32) -> EnumTextureData {
        EnumTextureData {
            u0: uvec4(tex, 0, 0, 0),
            v0: scale.extend(0.0),
        }
    }
}

impl<'a> Texture for CheckerBoard<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, uv: Vec2) -> ColorOrTarget {
        let w = self.data.v0.x;
        let h = self.data.v0.y;

        let tex1 = self.data.u0.x;
        let tex2 = self.data.u0.y;

        let x = uv.x * w;
        let y = uv.y * h;

        if (f32_to_u32(x) % 2 == 0) == (f32_to_u32(y) % 2 == 0) {
            ColorOrTarget {
                color_or_uv: Packed4::new(tex1, vec3a(fract(x), fract(y), 0.0)),
                scale: vec3a(1.0, 1.0, 1.0),
            }
        } else {
            ColorOrTarget {
                color_or_uv: Packed4::new(tex2, vec3a(fract(x), fract(y), 0.0)),
                scale: vec3a(1.0, 1.0, 1.0),
            }
        }
    }
}

impl<'a> Texture for ImageMap<'a> {
    fn color(&self, images: &RuntimeArray<InputImage>, uv: Vec2) -> ColorOrTarget {
        let image = unsafe { images.index(self.data.u0.x as usize) };
        let color: Vec4 = unsafe { image.sample_by_lod(vec2(uv.x, 1.0 - uv.y), 0.0) };
        ColorOrTarget {
            color_or_uv: Packed4::new(u32::MAX, color.xyz().into()),
            scale: vec3a(1.0, 1.0, 1.0),
        }
    }
}

impl<'a> Texture for Solid<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, _uv: Vec2) -> ColorOrTarget {
        ColorOrTarget {
            color_or_uv: Packed4::new(u32::MAX, self.data.v0.xyz().into()),
            scale: vec3a(1.0, 1.0, 1.0),
        }
    }
}

impl<'a> Texture for Scale<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, uv: Vec2) -> ColorOrTarget {
        ColorOrTarget {
            color_or_uv: Packed4::new(self.data.u0.x, uv.extend(0.0).into()),
            scale: self.data.v0.xyz().into(),
        }
    }
}

impl EnumTexture {
    pub fn new_solid(color: Vec3A) -> Self {
        Self {
            t: TextureType::Solid,
            data: Solid::new_data(color),
        }
    }

    pub fn new_checkerboard(tex1: u32, tex2: u32, uscale: f32, vscale: f32) -> Self {
        Self {
            t: TextureType::CheckerBoard,
            data: CheckerBoard::new_data(tex1, tex2, uscale, vscale),
        }
    }

    pub fn new_image_map(image: u32) -> Self {
        Self {
            t: TextureType::ImageMap,
            data: ImageMap::new_data(image),
        }
    }

    pub fn new_scale(scale: Vec3A, tex: u32) -> Self {
        Self {
            t: TextureType::Scale,
            data: Scale::new_data(scale, tex),
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
            TextureType::Solid => Solid { data: &self.data }.color(images, uv),
            TextureType::CheckerBoard => CheckerBoard { data: &self.data }.color(images, uv),
            TextureType::ImageMap => ImageMap { data: &self.data }.color(images, uv),
            TextureType::Scale => Scale { data: &self.data }.color(images, uv),
        };

        let mut scale = color_or_target.scale;
        while color_or_target.color_or_uv.t != u32::MAX {
            let tex = unsafe { textures.index_unchecked(color_or_target.color_or_uv.t as usize) };
            color_or_target = match tex.t {
                TextureType::Solid => Solid { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
                TextureType::CheckerBoard => CheckerBoard { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
                TextureType::ImageMap => ImageMap { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
                TextureType::Scale => Scale { data: &tex.data }.color(
                    images,
                    vec2(color_or_target.color_or_uv.x, color_or_target.color_or_uv.y),
                ),
            };
            scale *= color_or_target.scale;
        }

        scale * color_or_target.color_or_uv.xyz()
    }
}
