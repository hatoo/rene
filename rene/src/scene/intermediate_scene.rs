use std::{f32::consts::PI, ffi::OsStr, fs::File, io::Read, path::Path};

use blackbody::temperature_to_rgb;
use glam::{vec2, vec3a, Mat4, Vec2, Vec3A};
use image::GenericImageView;
use pbrt_parser::Object;
use ply::ply::{Ply, PropertyAccess};
use ply_rs as ply;
use rene_shader::Vertex;
use thiserror::Error;

use crate::scene::pfm_parser::parse_pfm_rgb;

use super::{image::Image, spectrum::parse_spd, subdivision::loop_subdivision};

#[derive(PartialEq, Debug)]
pub struct LookAt {
    pub eye: Vec3A,
    pub look_at: Vec3A,
    pub up: Vec3A,
}

pub enum SceneObject {
    Camera(Camera),
}

pub struct Perspective {
    pub fov: f32,
}

pub enum Camera {
    Perspective(Perspective),
}

pub enum IntermediateWorld {
    Attribute(Vec<IntermediateWorld>),
    TransformBeginEnd(Vec<IntermediateWorld>),
    ObjectBeginEnd(String, Vec<IntermediateWorld>),
    ObjectInstance(String),
    WorldObject(WorldObject),
    Matrix(Mat4),
    Transform(Mat4),
    Texture(Texture),
    NamedMaterial(String),
    CoordSysTransform(String),
    MediumInterface(String, String),
    ReverseOrientation,
}

pub enum WorldObject {
    LightSource(LightSource),
    AreaLightSource(AreaLightSource),
    Material(Material),
    MakeNamedMaterial(String, Material),
    MakeNamedMedium(String, Medium),
    Shape(Shape),
}

pub enum AreaLightSource {
    Diffuse(Vec3A),
}

pub enum LightSource {
    Infinite(Infinite),
    Distant(Distant),
}

pub struct Infinite {
    pub color: Vec3A,
    pub image_map: Option<Image>,
}

pub struct Distant {
    pub from: Vec3A,
    pub to: Vec3A,
    pub color: Vec3A,
}

#[derive(Clone)]
pub enum TextureOrColor {
    Color(Vec3A),
    Texture(String),
}

pub struct CheckerBoard {
    pub tex1: TextureOrColor,
    pub tex2: TextureOrColor,
    pub uscale: f32,
    pub vscale: f32,
}

pub enum InnerTexture {
    Constant(Vec3A),
    CheckerBoard(CheckerBoard),
    ImageMap(Image),
    Scale(TextureOrColor, TextureOrColor),
    Mix(MixTexture),
}

pub struct MixTexture {
    pub tex1: TextureOrColor,
    pub tex2: TextureOrColor,
    pub amount: TextureOrColor,
}

pub struct Texture {
    pub name: String,
    pub inner: InnerTexture,
}
pub enum Material {
    None,
    Matte(Matte),
    Glass(Glass),
    Substrate(Substrate),
    Metal(Metal),
    Mirror(Mirror),
    Uber(Uber),
    Plastic(Plastic),
    Mix(MixMaterial),
}

pub struct Matte {
    pub albedo: TextureOrColor,
}

pub struct Glass {
    pub index: f32,
}

pub struct Substrate {
    pub diffuse: TextureOrColor,
    pub specular: TextureOrColor,
    pub rough_u: TextureOrColor,
    pub rough_v: TextureOrColor,
    pub remap_roughness: bool,
}

pub struct Metal {
    pub eta: TextureOrColor,
    pub k: TextureOrColor,
    pub rough_u: TextureOrColor,
    pub rough_v: TextureOrColor,
    pub remap_roughness: bool,
}

pub struct Mirror {
    pub r: TextureOrColor,
}

pub struct Uber {
    pub kd: TextureOrColor,
    pub ks: TextureOrColor,
    pub kr: TextureOrColor,
    pub kt: TextureOrColor,
    pub rough_u: TextureOrColor,
    pub rough_v: TextureOrColor,
    pub eta: f32,
    pub opacity: TextureOrColor,
    pub remap_roughness: bool,
}

pub struct Plastic {
    pub kd: TextureOrColor,
    pub ks: TextureOrColor,
    pub rough: TextureOrColor,
    pub remap_roughness: bool,
}

pub struct MixMaterial {
    pub mat1: String,
    pub mat2: String,
    pub amount: TextureOrColor,
}

pub enum Medium {
    Homogeneous(Homogeneous),
}

pub struct Homogeneous {
    pub sigma_s: Vec3A,
    pub sigma_a: Vec3A,
    pub g: f32,
}

pub enum Shape {
    Sphere(Sphere),
    TriangleMesh(TriangleMesh),
}

pub struct Sphere {
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct TriangleMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug)]
pub struct Film {
    pub filename: String,
    pub xresolution: u32,
    pub yresolution: u32,
}

impl Default for Film {
    fn default() -> Self {
        Self {
            filename: "out.png".to_string(),
            xresolution: 640,
            yresolution: 480,
        }
    }
}

pub enum IntermediateScene {
    Matrix(Mat4),
    Transform(Mat4),
    SceneObject(SceneObject),
    World(Vec<IntermediateWorld>),
    // TODO implement it
    Sampler,
    // TODO implement it
    Integrator(Integrator),
    // TODO implement it
    PixelFilter,
    Film(Film),
}

#[derive(Debug)]
pub enum Integrator {
    Path,
    VolPath,
}

impl Default for Integrator {
    fn default() -> Self {
        Self::Path
    }
}

#[derive(Error, Debug)]
pub enum ArgumentError {
    #[error("unmatched value length")]
    UnmatchedValueLength,
    #[error("unmatched type on {0}")]
    UnmatchedType(String),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid Camera type {0}")]
    InvalidCamera(String),
    #[error("Invalid Film type {0}")]
    InvalidFilm(String),
    #[error("Invalid LightSource type {0}")]
    InvalidLightSource(String),
    #[error("Invalid AreaLightSource type {0}")]
    InvalidAreaLightSource(String),
    #[error("Invalid Material type {0}")]
    InvalidMaterial(String),
    #[error("Invalid Texture type {0}")]
    InvalidTexture(String),
    #[error("Invalid Shape type {0}")]
    InvalidShape(String),
    #[error("Invalid Argument {0}")]
    InvalidArgument(#[from] ArgumentError),
    #[error("Argument not found {0}")]
    ArgumentNotFound(String),
    #[error("IO Error {0}")]
    IO(#[from] std::io::Error),
    #[error("Image Error {0}")]
    Image(#[from] image::error::ImageError),
    #[error("PFM decode error")]
    Pfm,
    #[error("SPD decode error")]
    Spd,
    #[error("Ply error")]
    Ply,
    #[error("Exr Error")]
    Exr(#[from] exr::error::Error),
}

trait GetValue {
    fn get_bool(&self, name: &str) -> Result<Result<bool, ArgumentError>, Error>;
    fn get_float(&self, name: &str) -> Result<Result<f32, ArgumentError>, Error>;
    fn get_floats(&self, name: &str) -> Result<Result<&[f32], ArgumentError>, Error>;
    fn get_integer(&self, name: &str) -> Result<Result<i32, ArgumentError>, Error>;
    fn get_integers(&self, name: &str) -> Result<Result<&[i32], ArgumentError>, Error>;
    fn get_points(&self, name: &str) -> Result<Result<&[Vec3A], ArgumentError>, Error>;
    fn get_normals(&self, name: &str) -> Result<Result<&[Vec3A], ArgumentError>, Error>;
    fn get_str(&self, name: &str) -> Result<Result<&str, ArgumentError>, Error>;
    fn get_point(&self, name: &str) -> Result<Result<Vec3A, ArgumentError>, Error>;
    fn get_rgb<P: AsRef<Path>>(
        &self,
        name: &str,
        base_path: &P,
    ) -> Result<Result<Vec3A, ArgumentError>, Error>;
    fn get_texture_or_color<P: AsRef<Path>>(
        &self,
        name: &str,
        base_path: &P,
    ) -> Result<Result<TextureOrColor, ArgumentError>, Error>;
    fn get_material<P: AsRef<Path>>(&self, base_path: &P) -> Result<Material, Error>;
}

impl<'a, T> GetValue for Object<'a, T> {
    fn get_rgb<P: AsRef<Path>>(
        &self,
        name: &str,
        base_path: &P,
    ) -> Result<Result<Vec3A, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Rgb(v) => {
                    if v.len() != 3 {
                        Err(ArgumentError::UnmatchedValueLength)
                    } else {
                        Ok(vec3a(v[0], v[1], v[2]))
                    }
                }
                pbrt_parser::Value::BlackBody(v) => {
                    if v.len() % 2 != 0 {
                        Err(ArgumentError::UnmatchedValueLength)
                    } else {
                        let mut color = Vec3A::ZERO;

                        for v in v.chunks(2) {
                            color += v[1] * Vec3A::from(temperature_to_rgb(v[0]));
                        }
                        Ok(color)
                    }
                }
                pbrt_parser::Value::Spectrum(file) => {
                    let mut path = base_path.as_ref().to_path_buf();
                    path.push(file);
                    // TODO Error handling
                    Ok(load_spd(&path).unwrap())
                }
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_texture_or_color<P: AsRef<Path>>(
        &self,
        name: &str,
        base_path: &P,
    ) -> Result<Result<TextureOrColor, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Float(v) => {
                    if v.len() != 1 {
                        Err(ArgumentError::UnmatchedValueLength)
                    } else {
                        Ok(TextureOrColor::Color(vec3a(v[0], v[0], v[0])))
                    }
                }
                pbrt_parser::Value::Rgb(v) => {
                    if v.len() != 3 {
                        Err(ArgumentError::UnmatchedValueLength)
                    } else {
                        Ok(TextureOrColor::Color(vec3a(v[0], v[1], v[2])))
                    }
                }
                pbrt_parser::Value::BlackBody(v) => {
                    if v.len() % 2 != 0 {
                        Err(ArgumentError::UnmatchedValueLength)
                    } else {
                        let mut color = Vec3A::ZERO;

                        for v in v.chunks(2) {
                            color += v[1] * Vec3A::from(temperature_to_rgb(v[0]));
                        }
                        Ok(TextureOrColor::Color(color))
                    }
                }
                pbrt_parser::Value::Spectrum(file) => {
                    let mut path = base_path.as_ref().to_path_buf();
                    path.push(file);
                    // TODO Error handling
                    Ok(TextureOrColor::Color(load_spd(&path).unwrap()))
                }
                pbrt_parser::Value::Texture(s) => Ok(TextureOrColor::Texture(s[0].to_string())),
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_float(&self, name: &str) -> Result<Result<f32, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Float(v) => {
                    if v.len() == 1 {
                        Ok(v[0])
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_floats(&self, name: &str) -> Result<Result<&[f32], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Float(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_integer(&self, name: &str) -> Result<Result<i32, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Integer(v) => {
                    if v.len() == 1 {
                        Ok(v[0])
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_integers(&self, name: &str) -> Result<Result<&[i32], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Integer(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_points(&self, name: &str) -> Result<Result<&[Vec3A], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Point(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_normals(&self, name: &str) -> Result<Result<&[Vec3A], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Normal(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_str(&self, name: &str) -> Result<Result<&str, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::String(s) => {
                    if s.len() == 1 {
                        Ok(s[0])
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_point(&self, name: &str) -> Result<Result<Vec3A, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Point(v) => {
                    if v.len() == 1 {
                        Ok(v[0])
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_material<P: AsRef<Path>>(&self, base_path: &P) -> Result<Material, Error> {
        match self.t {
            "none" | "" => Ok(Material::None),
            "matte" => {
                let albedo = self
                    .get_texture_or_color("Kd", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;

                Ok(Material::Matte(Matte { albedo }))
            }
            "glass" => {
                let index = self.get_float("index").unwrap_or(Ok(1.5))?;
                Ok(Material::Glass(Glass { index }))
            }
            "substrate" => {
                let diffuse = self
                    .get_texture_or_color("Kd", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;
                let specular = self
                    .get_texture_or_color("Ks", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;

                let (rough_u, rough_v) =
                    if let Ok(roughness) = self.get_texture_or_color("roughness", base_path) {
                        let r = roughness?;
                        (r.clone(), r)
                    } else if let (Ok(Ok(rough_u)), Ok(Ok(rough_v))) = (
                        self.get_texture_or_color("uroughness", base_path),
                        self.get_texture_or_color("vroughness", base_path),
                    ) {
                        (rough_u, rough_v)
                    } else {
                        (
                            TextureOrColor::Color(vec3a(0.0, 0.0, 0.0)),
                            TextureOrColor::Color(vec3a(0.0, 0.0, 0.0)),
                        )
                    };

                let remap_roughness = self.get_bool("remaproughness").unwrap_or(Ok(true))?;

                Ok(Material::Substrate(Substrate {
                    diffuse,
                    specular,
                    rough_u,
                    rough_v,
                    remap_roughness,
                }))
            }
            "metal" => {
                let eta = self
                    .get_texture_or_color("eta", base_path)
                    .unwrap_or_else(|_| {
                        Ok(TextureOrColor::Color(vec3a(
                            0.19999069,
                            0.922_084_6,
                            1.099_875_9,
                        )))
                    })?;
                let k = self
                    .get_texture_or_color("k", base_path)
                    .unwrap_or_else(|_| {
                        Ok(TextureOrColor::Color(vec3a(
                            3.904_635_4,
                            2.447_633_3,
                            2.137_652_6,
                        )))
                    })?;

                let (rough_u, rough_v) =
                    if let Ok(roughness) = self.get_texture_or_color("roughness", base_path) {
                        let r = roughness?;
                        (r.clone(), r)
                    } else if let (Ok(Ok(rough_u)), Ok(Ok(rough_v))) = (
                        self.get_texture_or_color("uroughness", base_path),
                        self.get_texture_or_color("vroughness", base_path),
                    ) {
                        (rough_u, rough_v)
                    } else {
                        (
                            TextureOrColor::Color(vec3a(0.01, 0.01, 0.01)),
                            TextureOrColor::Color(vec3a(0.01, 0.01, 0.01)),
                        )
                    };

                let remap_roughness = self.get_bool("remaproughness").unwrap_or(Ok(true))?;

                Ok(Material::Metal(Metal {
                    eta,
                    k,
                    rough_u,
                    rough_v,
                    remap_roughness,
                }))
            }
            "mirror" => {
                let r = self
                    .get_texture_or_color("Kd", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.9, 0.9, 0.9))))?;

                Ok(Material::Mirror(Mirror { r }))
            }
            "uber" => {
                let kd = self
                    .get_texture_or_color("Kd", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.25, 0.25, 0.25))))?;
                let ks = self
                    .get_texture_or_color("Ks", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.25, 0.25, 0.25))))?;
                let kr = self
                    .get_texture_or_color("Kr", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(Vec3A::ZERO)))?;
                let kt = self
                    .get_texture_or_color("Kt", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(Vec3A::ZERO)))?;

                let (rough_u, rough_v) =
                    if let Ok(roughness) = self.get_texture_or_color("roughness", base_path) {
                        let r = roughness?;
                        (r.clone(), r)
                    } else if let (Ok(Ok(rough_u)), Ok(Ok(rough_v))) = (
                        self.get_texture_or_color("uroughness", base_path),
                        self.get_texture_or_color("vroughness", base_path),
                    ) {
                        (rough_u, rough_v)
                    } else {
                        (
                            TextureOrColor::Color(vec3a(0.1, 0.1, 0.1)),
                            TextureOrColor::Color(vec3a(0.1, 0.1, 0.1)),
                        )
                    };

                let eta = self.get_float("eta").unwrap_or(Ok(1.5))?;

                let opacity = self
                    .get_texture_or_color("opacity", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(1.0, 1.0, 1.0))))?;

                let remap_roughness = self.get_bool("remaproughness").unwrap_or(Ok(true))?;

                Ok(Material::Uber(Uber {
                    kd,
                    ks,
                    kr,
                    kt,
                    rough_u,
                    rough_v,
                    eta,
                    opacity,
                    remap_roughness,
                }))
            }
            "plastic" => {
                let kd = self
                    .get_texture_or_color("Kd", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.25, 0.25, 0.25))))?;
                let ks = self
                    .get_texture_or_color("Ks", base_path)
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.25, 0.25, 0.25))))?;
                let rough = self
                    .get_texture_or_color("roughness", base_path)
                    .unwrap_or(Ok(TextureOrColor::Color(vec3a(0.1, 0.1, 0.1))))?;
                let remap_roughness = self.get_bool("remaproughness").unwrap_or(Ok(true))?;

                Ok(Material::Plastic(Plastic {
                    kd,
                    ks,
                    rough,
                    remap_roughness,
                }))
            }
            "mix" => Ok(Material::Mix(MixMaterial {
                mat1: self.get_str("namenamedmaterial1")??.to_string(),
                mat2: self.get_str("namenamedmaterial2")??.to_string(),
                amount: self
                    .get_texture_or_color("amount")
                    .unwrap_or_else(|_| Ok(vec3a(0.5, 0.5, 0.5)))?,
            })),
            t => Err(Error::InvalidMaterial(t.to_string())),
        }
    }

    fn get_bool(&self, name: &str) -> Result<Result<bool, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Bool(v) => {
                    if v.len() == 1 {
                        Ok(v[0])
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType(name.to_string())),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }
}

fn deg_to_radian(angle: f32) -> f32 {
    angle * PI / 180.0
}

fn inverse_gamma_correct(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn load_spd<P: AsRef<Path>>(path: &P) -> Result<Vec3A, Error> {
    let mut content = String::new();
    File::open(path)?.read_to_string(&mut content)?;

    Ok(parse_spd(&content).map_err(|_| Error::Spd)?.1)
}

fn load_image<P: AsRef<Path>>(path: P) -> Result<Image, Error> {
    let pfm = OsStr::new("pfm");
    let exr = OsStr::new("exr");

    match path.as_ref().extension() {
        Some(ext) if ext == pfm => {
            let mut content = Vec::new();
            File::open(path)?.read_to_end(&mut content)?;

            Ok(parse_pfm_rgb(&content).map_err(|_| Error::Pfm)?.1)
        }
        Some(ext) if ext == exr => {
            let image = exr::prelude::read_first_rgba_layer_from_file(
                path,
                |resolution, _| {
                    Image::new(
                        resolution.width() as u32,
                        resolution.height() as u32,
                        vec![[0.0, 0.0, 0.0, 0.0]; resolution.width() * resolution.height()],
                    )
                },
                |pixel_vector, position, (r, g, b, a): (f32, f32, f32, f32)| {
                    pixel_vector.data[pixel_vector.width as usize * position.y() + position.x()] =
                        [r, g, b, a];
                },
            )?;

            Ok(image.layer_data.channel_data.pixels)
        }
        _ => {
            let image = image::io::Reader::open(path)?.decode()?;

            let mut data = Vec::new();

            for (_, _, p) in image.pixels() {
                data.push([
                    inverse_gamma_correct(p.0[0] as f32 / 255.0),
                    inverse_gamma_correct(p.0[1] as f32 / 255.0),
                    inverse_gamma_correct(p.0[2] as f32 / 255.0),
                    p.0[3] as f32 / 255.0,
                ]);
            }

            Ok(Image::new(image.width(), image.height(), data))
        }
    }
}

fn load_ply<E: PropertyAccess>(ply: &Ply<E>) -> Result<TriangleMesh, Error> {
    let vertex = ply.payload.get("vertex").unwrap();
    let faces = ply.payload.get("face").unwrap();

    let x_string = "x".to_string();
    let y_string = "y".to_string();
    let z_string = "z".to_string();

    let nx_string = "nx".to_string();
    let ny_string = "ny".to_string();
    let nz_string = "nz".to_string();

    let u = "u".to_string();
    let v = "v".to_string();

    let vertex_indices_string = "vertex_indices".to_string();

    let vertices: Vec<Vertex> = vertex
        .iter()
        .map(|e| {
            let x = e.get_float(&x_string).ok_or(Error::Ply)?;
            let y = e.get_float(&y_string).ok_or(Error::Ply)?;
            let z = e.get_float(&z_string).ok_or(Error::Ply)?;

            let normal = if let (Some(nx), Some(ny), Some(nz)) = (
                e.get_float(&nx_string),
                e.get_float(&ny_string),
                e.get_float(&nz_string),
            ) {
                vec3a(nx, ny, nz)
            } else {
                Vec3A::ZERO
            };

            let uv = if let (Some(u), Some(v)) = (e.get_float(&u), e.get_float(&v)) {
                vec2(u, v)
            } else {
                Vec2::ZERO
            };

            Ok(Vertex {
                position: vec3a(x, y, z),
                normal,
                uv,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let mut indices = Vec::new();

    for e in faces {
        let face: Vec<u32> = e
            .get_list_int(&vertex_indices_string)
            .map(|v| v.iter().map(|&i| i as u32).collect())
            .or_else(|| e.get_list_uint(&vertex_indices_string).map(|v| v.to_vec()))
            .ok_or(Error::Ply)?;

        assert!(face.iter().all(|&i| (i as usize) < vertices.len()));
        match face.len() {
            3 => {
                indices.extend(face.into_iter());
            }
            4 => {
                indices.extend_from_slice(&[face[0], face[1], face[2]]);
                indices.extend_from_slice(&[face[0], face[2], face[3]]);
            }
            x => {
                log::info!("Unsupported face len {}", x);
                return Err(Error::Ply);
            }
        }
    }
    Ok(TriangleMesh { vertices, indices })
}

impl IntermediateWorld {
    fn from_world<P: AsRef<Path>>(world: pbrt_parser::World, base_dir: &P) -> Result<Self, Error> {
        match world {
            pbrt_parser::World::ReverseOrientation => Ok(Self::ReverseOrientation),
            pbrt_parser::World::ObjectInstance(name) => Ok(Self::ObjectInstance(name.to_string())),
            pbrt_parser::World::Transform(m) => Ok(Self::Transform(m)),
            pbrt_parser::World::ConcatTransform(m) => Ok(Self::Matrix(m)),
            pbrt_parser::World::NamedMaterial(name) => Ok(Self::NamedMaterial(name.to_string())),
            pbrt_parser::World::MediumInterface(interior, exterior) => Ok(Self::MediumInterface(
                interior.to_string(),
                exterior.to_string(),
            )),
            pbrt_parser::World::CoordSysTransform(name) => {
                Ok(Self::CoordSysTransform(name.to_string()))
            }
            pbrt_parser::World::Texture(texture) => match texture.obj.t {
                "constant" => {
                    let value = if let Ok(Ok(v)) = texture.obj.get_float("value") {
                        vec3a(v, v, v)
                    } else if let Ok(Ok(rgb)) = texture.obj.get_rgb("value", base_dir) {
                        rgb
                    } else {
                        vec3a(1.0, 1.0, 1.0)
                    };

                    Ok(Self::Texture(Texture {
                        name: texture.name.to_string(),
                        inner: InnerTexture::Constant(value),
                    }))
                }
                "scale" => {
                    let tex1 = texture
                        .obj
                        .get_texture_or_color("tex1", base_dir)
                        .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(1.0, 1.0, 1.0))))?;
                    let tex2 = texture
                        .obj
                        .get_texture_or_color("tex2", base_dir)
                        .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(1.0, 1.0, 1.0))))?;

                    Ok(Self::Texture(Texture {
                        name: texture.name.to_string(),
                        inner: InnerTexture::Scale(tex1, tex2),
                    }))
                }
                "mix" => Ok(Self::Texture(Texture {
                    name: texture.name.to_string(),
                    inner: InnerTexture::Mix(MixTexture {
                        tex1: texture
                            .obj
                            .get_texture_or_color("tex1", base_dir)
                            .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.0, 0.0, 0.0))))?,
                        tex2: texture
                            .obj
                            .get_texture_or_color("tex2", base_dir)
                            .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(1.0, 1.0, 1.0))))?,
                        amount: texture
                            .obj
                            .get_texture_or_color("amount", base_dir)
                            .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?,
                    }),
                })),
                "checkerboard" => {
                    let tex1 = texture
                        .obj
                        .get_texture_or_color("tex1", base_dir)
                        .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.0, 0.0, 0.0))))?;
                    let tex2 = texture
                        .obj
                        .get_texture_or_color("tex2", base_dir)
                        .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(1.0, 1.0, 1.0))))?;

                    let uscale = texture.obj.get_float("uscale").unwrap_or(Ok(2.0))?;
                    let vscale = texture.obj.get_float("vscale").unwrap_or(Ok(2.0))?;

                    Ok(Self::Texture(Texture {
                        name: texture.name.to_string(),
                        inner: InnerTexture::CheckerBoard(CheckerBoard {
                            tex1,
                            tex2,
                            uscale,
                            vscale,
                        }),
                    }))
                }
                "imagemap" => {
                    let filename = texture.obj.get_str("filename")??;
                    let mut pathbuf = base_dir.as_ref().to_path_buf();
                    pathbuf.push(filename);
                    let image = load_image(pathbuf)?;
                    Ok(Self::Texture(Texture {
                        name: texture.name.to_string(),
                        inner: InnerTexture::ImageMap(image),
                    }))
                }
                t => Err(Error::InvalidTexture(t.to_string())),
            },
            pbrt_parser::World::WorldObject(obj) => match obj.object_type {
                pbrt_parser::WorldObjectType::LightSource => match obj.t {
                    "infinite" => {
                        let color = obj
                            .get_rgb("L", base_dir)
                            .unwrap_or_else(|_| Ok(vec3a(1.0, 1.0, 1.0)))?;

                        let image_map = if let Ok(filename) = obj.get_str("mapname") {
                            let filename = filename?;
                            let mut pathbuf = base_dir.as_ref().to_path_buf();
                            pathbuf.push(filename);
                            Some(load_image(pathbuf)?)
                        } else {
                            None
                        };

                        Ok(Self::WorldObject(WorldObject::LightSource(
                            LightSource::Infinite(Infinite { color, image_map }),
                        )))
                    }
                    "distant" => {
                        let from = obj
                            .get_point("from")
                            .unwrap_or_else(|_| Ok(vec3a(0.0, 0.0, 0.0)))?;
                        let to = obj
                            .get_point("to")
                            .unwrap_or_else(|_| Ok(vec3a(0.0, 0.0, 1.0)))?;
                        let color = obj
                            .get_rgb("L", base_dir)
                            .unwrap_or_else(|_| Ok(vec3a(1.0, 1.0, 1.0)))?;
                        Ok(Self::WorldObject(WorldObject::LightSource(
                            LightSource::Distant(Distant { from, to, color }),
                        )))
                    }
                    t => Err(Error::InvalidLightSource(t.to_string())),
                },
                pbrt_parser::WorldObjectType::AreaLightSource => match obj.t {
                    "diffuse" | "area" => {
                        let l = obj.get_rgb("L", base_dir)??;
                        Ok(Self::WorldObject(WorldObject::AreaLightSource(
                            AreaLightSource::Diffuse(l),
                        )))
                    }
                    t => Err(Error::InvalidAreaLightSource(t.to_string())),
                },
                pbrt_parser::WorldObjectType::Material => Ok(Self::WorldObject(
                    WorldObject::Material(obj.get_material(base_dir)?),
                )),
                pbrt_parser::WorldObjectType::MakeNamedMaterial => {
                    let t = obj.get_str("type")??;
                    let name = obj.t.to_string();
                    let mut obj = obj.clone();
                    obj.t = t;

                    Ok(Self::WorldObject(WorldObject::MakeNamedMaterial(
                        name,
                        obj.get_material(base_dir)?,
                    )))
                }
                pbrt_parser::WorldObjectType::MakeNamedMedium => {
                    let name = obj.t.to_string();

                    let sigma_a = obj
                        .get_rgb("sigma_a", base_dir)
                        .unwrap_or(Ok(vec3a(0.0011, 0.0024, 0.014)))?;

                    let sigma_s = obj
                        .get_rgb("sigma_s", base_dir)
                        .unwrap_or(Ok(vec3a(2.55, 3.21, 3.77)))?;

                    let g = obj.get_float("g").unwrap_or(Ok(0.0))?;

                    Ok(Self::WorldObject(WorldObject::MakeNamedMedium(
                        name,
                        Medium::Homogeneous(Homogeneous {
                            sigma_a,
                            sigma_s,
                            g,
                        }),
                    )))
                }
                pbrt_parser::WorldObjectType::Shape => match obj.t {
                    "sphere" => {
                        let radius = obj.get_float("radius").unwrap_or(Ok(1.0))?;
                        Ok(Self::WorldObject(WorldObject::Shape(Shape::Sphere(
                            Sphere { radius },
                        ))))
                    }
                    "trianglemesh" | "loopsubdiv" => {
                        let indices = obj.get_integers("indices")??;
                        let indices: Vec<u32> = indices.iter().map(|&i| i as u32).collect();
                        let vertices = obj.get_points("P")??;

                        let normal = obj
                            .get_normals("N")
                            .map(|r| r.map(Some))
                            .unwrap_or(Ok(None))?;

                        let uv = obj
                            .get_floats("st")
                            .or_else(|_| obj.get_floats("uv"))
                            .map(|r| r.map(Some))
                            .unwrap_or(Ok(None))?;

                        // TODO st length check

                        if indices.len() % 3 != 0 {
                            return Err(Error::InvalidArgument(
                                ArgumentError::UnmatchedValueLength,
                            ));
                        }

                        let mesh = if let Some(normal) = normal {
                            if normal.len() != vertices.len() {
                                return Err(Error::InvalidArgument(
                                    ArgumentError::UnmatchedValueLength,
                                ));
                            }

                            TriangleMesh {
                                indices,
                                vertices: vertices
                                    .iter()
                                    .zip(normal.iter())
                                    .enumerate()
                                    .map(|(i, (position, normal))| Vertex {
                                        position: *position,
                                        normal: *normal,
                                        uv: uv
                                            .map(|uv| vec2(uv[2 * i], uv[2 * i + 1]))
                                            .unwrap_or(Vec2::ZERO),
                                    })
                                    .collect(),
                            }
                        } else {
                            TriangleMesh {
                                indices,
                                vertices: vertices
                                    .iter()
                                    .enumerate()
                                    .map(|(i, position)| Vertex {
                                        position: *position,
                                        normal: Vec3A::ZERO,
                                        uv: uv
                                            .map(|st| vec2(st[2 * i], st[2 * i + 1]))
                                            .unwrap_or(Vec2::ZERO),
                                    })
                                    .collect(),
                            }
                        };

                        if obj.t == "loopsubdiv" {
                            let nlevels = obj.get_integer("nlevels")??;

                            Ok(Self::WorldObject(WorldObject::Shape(Shape::TriangleMesh(
                                loop_subdivision(mesh, nlevels as usize),
                            ))))
                        } else {
                            Ok(Self::WorldObject(WorldObject::Shape(Shape::TriangleMesh(
                                mesh,
                            ))))
                        }
                    }
                    "plymesh" => {
                        let filename = obj.get_str("filename")??;
                        let mut pathbuf = base_dir.as_ref().to_path_buf();
                        pathbuf.push(filename);
                        let mut f = std::fs::File::open(pathbuf)?;

                        let p = ply::parser::Parser::<ply::ply::DefaultElement>::new();

                        let ply = p.read_ply(&mut f)?;

                        let triangle_mesh = load_ply(&ply)?;

                        Ok(Self::WorldObject(WorldObject::Shape(Shape::TriangleMesh(
                            triangle_mesh,
                        ))))
                    }
                    t => Err(Error::InvalidShape(t.to_string())),
                },
            },
            pbrt_parser::World::Attribute(worlds) => worlds
                .into_iter()
                .map(|w| Self::from_world(w, base_dir))
                .collect::<Result<Vec<Self>, Error>>()
                .map(IntermediateWorld::Attribute),
            pbrt_parser::World::TransformBeginEnd(worlds) => worlds
                .into_iter()
                .map(|w| Self::from_world(w, base_dir))
                .collect::<Result<Vec<Self>, Error>>()
                .map(IntermediateWorld::TransformBeginEnd),
            pbrt_parser::World::Translate(translation) => {
                Ok(Self::Matrix(Mat4::from_translation(translation.into())))
            }
            pbrt_parser::World::ObjectBeginEnd(name, worlds) => worlds
                .into_iter()
                .map(|w| Self::from_world(w, base_dir))
                .collect::<Result<Vec<Self>, Error>>()
                .map(|worlds| IntermediateWorld::ObjectBeginEnd(name.to_string(), worlds)),
            pbrt_parser::World::Scale(scale) => Ok(Self::Matrix(Mat4::from_scale(scale.into()))),
            pbrt_parser::World::Rotate(axis_angle) => Ok(Self::Matrix(Mat4::from_axis_angle(
                axis_angle.axis.normalize().into(),
                deg_to_radian(axis_angle.angle),
            ))),
        }
    }
}

impl IntermediateScene {
    pub fn from_scene<P: AsRef<Path>>(
        scene: pbrt_parser::Scene,
        base_dir: &P,
    ) -> Result<Self, Error> {
        match scene {
            pbrt_parser::Scene::LookAt(look_at) => Ok(Self::Matrix(Mat4::look_at_lh(
                look_at.eye.into(),
                look_at.look_at.into(),
                look_at.up.into(),
            ))),
            pbrt_parser::Scene::Translate(translation) => {
                Ok(Self::Matrix(Mat4::from_translation(translation.into())))
            }
            pbrt_parser::Scene::Rotate(axis_angle) => Ok(Self::Matrix(Mat4::from_axis_angle(
                axis_angle.axis.normalize().into(),
                deg_to_radian(axis_angle.angle),
            ))),
            pbrt_parser::Scene::Scale(scale) => Ok(Self::Matrix(Mat4::from_scale(scale.into()))),
            pbrt_parser::Scene::ConcatTransform(m) => Ok(Self::Matrix(m)),
            pbrt_parser::Scene::Transform(m) => Ok(Self::Transform(m)),
            pbrt_parser::Scene::SceneObject(obj) => match obj.object_type {
                pbrt_parser::SceneObjectType::Sampler => Ok(Self::Sampler),
                pbrt_parser::SceneObjectType::Integrator => match obj.t {
                    "volpath" => Ok(Self::Integrator(Integrator::VolPath)),
                    "path" => Ok(Self::Integrator(Integrator::Path)),
                    i => {
                        log::info!("{} integrator is not implemented. Use volpath.", i);
                        Ok(Self::Integrator(Integrator::VolPath))
                    }
                },
                pbrt_parser::SceneObjectType::PixelFilter => Ok(Self::PixelFilter),
                pbrt_parser::SceneObjectType::Camera => match obj.t {
                    "perspective" => {
                        let fov = obj.get_float("fov").unwrap_or(Ok(90.0))?;
                        Ok(Self::SceneObject(SceneObject::Camera(Camera::Perspective(
                            Perspective {
                                fov: deg_to_radian(fov),
                            },
                        ))))
                    }
                    t => Err(Error::InvalidCamera(t.to_string())),
                },
                pbrt_parser::SceneObjectType::Film => match obj.t {
                    "image" => {
                        let filename = obj.get_str("filename").unwrap_or(Ok("out.png"))?;
                        let xresolution = obj.get_integer("xresolution").unwrap_or(Ok(640))? as u32;
                        let yresolution = obj.get_integer("yresolution").unwrap_or(Ok(480))? as u32;
                        Ok(Self::Film(Film {
                            filename: filename.to_string(),
                            xresolution,
                            yresolution,
                        }))
                    }
                    t => Err(Error::InvalidFilm(t.to_string())),
                },
            },
            pbrt_parser::Scene::World(worlds) => worlds
                .into_iter()
                .map(|w| IntermediateWorld::from_world(w, base_dir))
                .collect::<Result<Vec<IntermediateWorld>, _>>()
                .map(IntermediateScene::World),
        }
    }
}
