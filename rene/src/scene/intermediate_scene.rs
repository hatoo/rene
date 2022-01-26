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

use super::{image::Image, subdivision::loop_subdivision};

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
    WorldObject(WorldObject),
    Matrix(Mat4),
    Texture(Texture),
    NamedMaterial(String),
}

pub enum WorldObject {
    LightSource(LightSource),
    AreaLightSource(AreaLightSource),
    Material(Material),
    MakeNamedMaterial(String, Material),
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
    CheckerBoard(CheckerBoard),
    ImageMap(Image),
}

pub struct Texture {
    pub name: String,
    pub inner: InnerTexture,
}
pub enum Material {
    Matte(Matte),
    Glass,
    Substrate(Substrate),
}

pub struct Matte {
    pub albedo: TextureOrColor,
}

pub struct Substrate {
    pub diffuse: TextureOrColor,
    pub specular: TextureOrColor,
    pub rough_u: f32,
    pub rough_v: f32,
    pub remap_roughness: bool,
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
    SceneObject(SceneObject),
    World(Vec<IntermediateWorld>),
    // TODO implement it
    Sampler,
    // TODO implement it
    Integrator,
    // TODO implement it
    PixelFilter,
    Film(Film),
}

#[derive(Error, Debug)]
pub enum ArgumentError {
    #[error("unmatched value length")]
    UnmatchedValueLength,
    #[error("unmatched type")]
    UnmatchedType,
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
    #[error("Ply error")]
    Ply,
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
    fn get_rgb(&self, name: &str) -> Result<Result<Vec3A, ArgumentError>, Error>;
    fn get_texture_or_color(
        &self,
        name: &str,
    ) -> Result<Result<TextureOrColor, ArgumentError>, Error>;
    fn get_material(&self) -> Result<Material, Error>;
}

impl<'a, T> GetValue for Object<'a, T> {
    fn get_rgb(&self, name: &str) -> Result<Result<Vec3A, ArgumentError>, Error> {
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
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_texture_or_color(
        &self,
        name: &str,
    ) -> Result<Result<TextureOrColor, ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
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
                pbrt_parser::Value::Texture(s) => Ok(TextureOrColor::Texture(s[0].to_string())),
                _ => Err(ArgumentError::UnmatchedType),
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
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_floats(&self, name: &str) -> Result<Result<&[f32], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Float(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType),
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
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_integers(&self, name: &str) -> Result<Result<&[i32], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Integer(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_points(&self, name: &str) -> Result<Result<&[Vec3A], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Point(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_normals(&self, name: &str) -> Result<Result<&[Vec3A], ArgumentError>, Error> {
        self.get_value(name)
            .map(|value| match value {
                pbrt_parser::Value::Normal(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType),
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
                _ => Err(ArgumentError::UnmatchedType),
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
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }

    fn get_material(&self) -> Result<Material, Error> {
        match self.t {
            "matte" => {
                let albedo = self
                    .get_texture_or_color("Kd")
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;

                Ok(Material::Matte(Matte { albedo }))
            }
            "glass" => Ok(Material::Glass),
            "substrate" => {
                let diffuse = self
                    .get_texture_or_color("Kd")
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;
                let specular = self
                    .get_texture_or_color("Ks")
                    .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;

                let rough_u = self.get_float("uroughness")??;
                let rough_v = self.get_float("vroughness")??;

                let remap_roughness = self.get_bool("remaproughness").unwrap_or(Ok(true))?;

                Ok(Material::Substrate(Substrate {
                    diffuse,
                    specular,
                    rough_u,
                    rough_v,
                    remap_roughness,
                }))
            }
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
                _ => Err(ArgumentError::UnmatchedType),
            })
            .ok_or_else(|| Error::ArgumentNotFound(name.to_string()))
    }
}

fn deg_to_radian(angle: f32) -> f32 {
    angle * PI / 180.0
}

fn load_image<P: AsRef<Path>>(path: P) -> Result<Image, Error> {
    if path.as_ref().extension() == Some(OsStr::new("pfm")) {
        let mut content = Vec::new();
        File::open(path)?.read_to_end(&mut content)?;

        Ok(parse_pfm_rgb(&content).map_err(|_| Error::Pfm)?.1)
    } else {
        let image = image::io::Reader::open(path)?.decode()?;

        let mut data = Vec::new();

        for (_, _, p) in image.pixels() {
            data.push([
                p.0[0] as f32 / 255.0,
                p.0[1] as f32 / 255.0,
                p.0[2] as f32 / 255.0,
                p.0[3] as f32 / 255.0,
            ]);
        }

        Ok(Image::new(image.width(), image.height(), data))
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

            Ok(Vertex {
                position: vec3a(x, y, z),
                normal,
                uv: Vec2::ZERO,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let mut indices = Vec::new();

    for e in faces {
        let face = e.get_list_int(&vertex_indices_string).ok_or(Error::Ply)?;

        assert_eq!(face.len(), 3);
        assert!(face.iter().all(|&i| (i as usize) < vertices.len()));
        indices.extend(face.iter().map(|&i| i as u32));
    }
    Ok(TriangleMesh { vertices, indices })
}

impl IntermediateWorld {
    fn from_world<P: AsRef<Path>>(world: pbrt_parser::World, base_dir: &P) -> Result<Self, Error> {
        match world {
            pbrt_parser::World::Transform(m) => Ok(Self::Matrix(m)),
            pbrt_parser::World::NamedMaterial(name) => Ok(Self::NamedMaterial(name.to_string())),
            pbrt_parser::World::Texture(texture) => match texture.obj.t {
                "checkerboard" => {
                    let tex1 = texture
                        .obj
                        .get_texture_or_color("tex1")
                        .unwrap_or_else(|_| Ok(TextureOrColor::Color(vec3a(0.0, 0.0, 0.0))))?;
                    let tex2 = texture
                        .obj
                        .get_texture_or_color("tex2")
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
                            .get_rgb("L")
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
                            .get_rgb("L")
                            .unwrap_or_else(|_| Ok(vec3a(1.0, 1.0, 1.0)))?;
                        Ok(Self::WorldObject(WorldObject::LightSource(
                            LightSource::Distant(Distant { from, to, color }),
                        )))
                    }
                    t => Err(Error::InvalidLightSource(t.to_string())),
                },
                pbrt_parser::WorldObjectType::AreaLightSource => match obj.t {
                    "diffuse" => {
                        let l = obj.get_rgb("L")??;
                        Ok(Self::WorldObject(WorldObject::AreaLightSource(
                            AreaLightSource::Diffuse(l),
                        )))
                    }
                    t => Err(Error::InvalidAreaLightSource(t.to_string())),
                },
                pbrt_parser::WorldObjectType::Material => Ok(Self::WorldObject(
                    WorldObject::Material(obj.get_material()?),
                )),
                pbrt_parser::WorldObjectType::MakeNamedMaterial => {
                    let t = obj.get_str("type")??;
                    let name = obj.t.to_string();
                    let mut obj = obj.clone();
                    obj.t = t;

                    Ok(Self::WorldObject(WorldObject::MakeNamedMaterial(
                        name,
                        obj.get_material()?,
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
            pbrt_parser::Scene::Rotate(axis_angle) => Ok(Self::Matrix(Mat4::from_axis_angle(
                axis_angle.axis.normalize().into(),
                deg_to_radian(axis_angle.angle),
            ))),
            pbrt_parser::Scene::Transform(m) => Ok(Self::Matrix(m)),
            pbrt_parser::Scene::SceneObject(obj) => match obj.object_type {
                pbrt_parser::SceneObjectType::Sampler => Ok(Self::Sampler),
                pbrt_parser::SceneObjectType::Integrator => Ok(Self::Integrator),
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
                        let filename = obj.get_str("filename")??;
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
