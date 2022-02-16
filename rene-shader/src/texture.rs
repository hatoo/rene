#[allow(unused_imports)]
use spirv_std::num_traits::Float;
use spirv_std::{
    arch::IndexUnchecked,
    glam::{uvec4, vec2, vec3a, vec4, UVec4, Vec2, Vec3A, Vec4, Vec4Swizzles},
    RuntimeArray,
};

use crate::{
    asm::{f32_to_u32, fract},
    InputImage,
};

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

struct IndexUV {
    index: u32,
    uv: Vec2,
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
    pub fn new_data(tex1: u32, tex2: u32) -> EnumTextureData {
        EnumTextureData {
            u0: uvec4(tex1, tex2, 0, 0),
            ..Default::default()
        }
    }
}

impl<'a> CheckerBoard<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, uv: Vec2) -> IndexUV {
        let w = self.data.v0.x;
        let h = self.data.v0.y;

        let tex1 = self.data.u0.x;
        let tex2 = self.data.u0.y;

        let x = uv.x * w;
        let y = uv.y * h;

        if (f32_to_u32(x) % 2 == 0) == (f32_to_u32(y) % 2 == 0) {
            IndexUV {
                index: tex1,
                uv: vec2(fract(x), fract(y)),
            }
        } else {
            IndexUV {
                index: tex2,
                uv: vec2(fract(x), fract(y)),
            }
        }
    }
}

impl<'a> ImageMap<'a> {
    fn color(&self, images: &RuntimeArray<InputImage>, uv: Vec2) -> Vec3A {
        let image = unsafe { images.index(self.data.u0.x as usize) };
        let color: Vec4 = unsafe { image.sample_by_lod(vec2(uv.x, 1.0 - uv.y), 0.0) };
        color.xyz().into()
    }
}

impl<'a> Solid<'a> {
    fn color(&self, _images: &RuntimeArray<InputImage>, _uv: Vec2) -> Vec3A {
        self.data.v0.xyz().into()
    }
}

impl<'a> Scale<'a> {
    fn tex1(&self) -> u32 {
        self.data.u0.x
    }

    fn tex2(&self) -> u32 {
        self.data.u0.y
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

    pub fn new_scale(tex1: u32, tex2: u32) -> Self {
        Self {
            t: TextureType::Scale,
            data: Scale::new_data(tex1, tex2),
        }
    }
}

impl EnumTexture {
    pub fn color_non_recursive(
        &self,
        index: u32,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        uv: Vec2,
    ) -> Vec3A {
        let mut index_uv = IndexUV { index, uv };
        loop {
            let tex = unsafe { textures.index_unchecked(index_uv.index as usize) };
            index_uv = match tex.t {
                TextureType::Solid => return Solid { data: &self.data }.color(images, index_uv.uv),
                TextureType::ImageMap => {
                    return ImageMap { data: &self.data }.color(images, index_uv.uv)
                }
                TextureType::CheckerBoard => {
                    CheckerBoard { data: &self.data }.color(images, index_uv.uv)
                }
                TextureType::Scale => return vec3a(1.0, 1.0, 1.0),
            };
        }
    }

    pub fn color(
        &self,
        textures: &[EnumTexture],
        images: &RuntimeArray<InputImage>,
        uv: Vec2,
    ) -> Vec3A {
        match self.t {
            TextureType::Solid => Solid { data: &self.data }.color(images, uv),
            TextureType::ImageMap => ImageMap { data: &self.data }.color(images, uv),
            TextureType::CheckerBoard => {
                let index_uv = CheckerBoard { data: &self.data }.color(images, uv);
                self.color_non_recursive(index_uv.index, textures, images, index_uv.uv)
            }
            TextureType::Scale => {
                let scale = Scale { data: &self.data };
                self.color_non_recursive(scale.tex1(), textures, images, uv)
                    * self.color_non_recursive(scale.tex2(), textures, images, uv)
            }
        }
    }
}
