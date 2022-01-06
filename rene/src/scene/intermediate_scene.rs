use blackbody::temperature_to_rgb;
use glam::{vec2, vec3a, Affine3A, Vec2, Vec3A};
use pbrt_parser::{ArgumentError, Object};
use rene_shader::{camera::PerspectiveCamera, Vertex};
use std::f32::consts::PI;
use thiserror::Error;

#[derive(PartialEq, Debug)]
pub struct LookAt {
    pub eye: Vec3A,
    pub look_at: Vec3A,
    pub up: Vec3A,
}

pub enum SceneObject {
    Camera(Camera),
}

pub enum Camera {
    Perspective(PerspectiveCamera),
}

pub enum IntermediateWorld {
    Attribute(Vec<IntermediateWorld>),
    WorldObject(WorldObject),
    Matrix(Affine3A),
    Texture(Texture),
}

pub enum WorldObject {
    LightSource(LightSource),
    Material(Material),
    Shape(Shape),
}

pub enum LightSource {
    Infinite(Infinite),
    Distant(Distant),
}

pub struct Infinite {
    pub color: Vec3A,
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
}

pub struct Texture {
    pub name: String,
    pub inner: InnerTexture,
}
pub enum Material {
    Matte(Matte),
    Glass,
}

pub struct Matte {
    pub albedo: TextureOrColor,
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
    LookAt(LookAt),
    SceneObject(SceneObject),
    World(Vec<IntermediateWorld>),
    // TODO implement it
    Sampler,
    // TODO implement it
    Integrator,
    Film(Film),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid Camera type {0}")]
    InvalidCamera(String),
    #[error("Invalid Film type {0}")]
    InvalidFilm(String),
    #[error("Invalid LightSource type {0}")]
    InvalidLightSource(String),
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
}

fn get_rgb<T>(obj: &Object<T>, name: &str) -> Option<Result<Vec3A, Error>> {
    obj.get_value(name).map(|value| match value {
        pbrt_parser::Value::Rgb(v) => {
            if v.len() != 3 {
                Err(Error::InvalidArgument(ArgumentError::UnmatchedValueLength))
            } else {
                Ok(vec3a(v[0], v[1], v[2]))
            }
        }
        pbrt_parser::Value::BlackBody(v) => {
            if v.len() % 2 != 0 {
                Err(Error::InvalidArgument(ArgumentError::UnmatchedValueLength))
            } else {
                let mut color = Vec3A::ZERO;

                for v in v.chunks(2) {
                    color += v[1] * Vec3A::from(temperature_to_rgb(v[0]));
                }
                Ok(color)
            }
        }
        _ => Err(Error::InvalidArgument(ArgumentError::UnmatchedType)),
    })
}

fn get_texture_or_color<T>(obj: &Object<T>, name: &str) -> Option<Result<TextureOrColor, Error>> {
    obj.get_value(name).map(|value| match value {
        pbrt_parser::Value::Rgb(v) => {
            if v.len() != 3 {
                Err(Error::InvalidArgument(ArgumentError::UnmatchedValueLength))
            } else {
                Ok(TextureOrColor::Color(vec3a(v[0], v[1], v[2])))
            }
        }
        pbrt_parser::Value::BlackBody(v) => {
            if v.len() % 2 != 0 {
                Err(Error::InvalidArgument(ArgumentError::UnmatchedValueLength))
            } else {
                let mut color = Vec3A::ZERO;

                for v in v.chunks(2) {
                    color += v[1] * Vec3A::from(temperature_to_rgb(v[0]));
                }
                Ok(TextureOrColor::Color(color))
            }
        }
        pbrt_parser::Value::Texture(s) => Ok(TextureOrColor::Texture(s.to_string())),
        _ => Err(Error::InvalidArgument(ArgumentError::UnmatchedType)),
    })
}

impl IntermediateWorld {
    fn from_world(world: pbrt_parser::World) -> Result<Self, Error> {
        match world {
            pbrt_parser::World::Texture(texture) => match texture.obj.t {
                "checkerboard" => {
                    let tex1 = get_texture_or_color(&texture.obj, "tex1")
                        .unwrap_or(Ok(TextureOrColor::Color(vec3a(0.0, 0.0, 0.0))))?;
                    let tex2 = get_texture_or_color(&texture.obj, "tex2")
                        .unwrap_or(Ok(TextureOrColor::Color(vec3a(1.0, 1.0, 1.0))))?;

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
                t => Err(Error::InvalidTexture(t.to_string())),
            },
            pbrt_parser::World::WorldObject(obj) => match obj.object_type {
                pbrt_parser::WorldObjectType::LightSource => match obj.t {
                    "infinite" => {
                        let color = get_rgb(&obj, "L").unwrap_or(Ok(vec3a(1.0, 1.0, 1.0)))?;
                        Ok(Self::WorldObject(WorldObject::LightSource(
                            LightSource::Infinite(Infinite { color }),
                        )))
                    }
                    "distant" => {
                        let from = obj.get_point("from").unwrap_or(Ok(vec3a(0.0, 0.0, 0.0)))?;
                        let to = obj.get_point("to").unwrap_or(Ok(vec3a(0.0, 0.0, 1.0)))?;
                        let color = get_rgb(&obj, "L").unwrap_or(Ok(vec3a(1.0, 1.0, 1.0)))?;
                        Ok(Self::WorldObject(WorldObject::LightSource(
                            LightSource::Distant(Distant { from, to, color }),
                        )))
                    }
                    t => Err(Error::InvalidLightSource(t.to_string())),
                },
                pbrt_parser::WorldObjectType::Material => match obj.t {
                    "matte" => {
                        let albedo = get_texture_or_color(&obj, "Kd")
                            .unwrap_or(Ok(TextureOrColor::Color(vec3a(0.5, 0.5, 0.5))))?;

                        Ok(Self::WorldObject(WorldObject::Material(Material::Matte(
                            Matte { albedo },
                        ))))
                    }
                    "glass" => Ok(Self::WorldObject(WorldObject::Material(Material::Glass))),
                    t => Err(Error::InvalidMaterial(t.to_string())),
                },
                pbrt_parser::WorldObjectType::Shape => match obj.t {
                    "sphere" => {
                        let radius = obj.get_float("radius").unwrap_or(Ok(1.0))?;
                        Ok(Self::WorldObject(WorldObject::Shape(Shape::Sphere(
                            Sphere { radius },
                        ))))
                    }
                    "trianglemesh" => {
                        let indices = obj
                            .get_integers("indices")
                            .ok_or(Error::ArgumentNotFound("indices".to_string()))??;
                        let indices: Vec<u32> = indices.into_iter().map(|&i| i as u32).collect();
                        let vertices = obj
                            .get_points("P")
                            .ok_or(Error::ArgumentNotFound("P".to_string()))??;

                        let normal = obj
                            .get_normals("N")
                            .map(|r| r.map(Some))
                            .unwrap_or(Ok(None))?;

                        let st = obj
                            .get_floats("st")
                            .map(|r| r.map(Some))
                            .unwrap_or(Ok(None))?;

                        // TODO st length check

                        if indices.len() % 3 != 0 {
                            return Err(Error::InvalidArgument(
                                ArgumentError::UnmatchedValueLength,
                            ));
                        }

                        if let Some(normal) = normal {
                            if normal.len() != vertices.len() {
                                return Err(Error::InvalidArgument(
                                    ArgumentError::UnmatchedValueLength,
                                ));
                            }

                            Ok(Self::WorldObject(WorldObject::Shape(Shape::TriangleMesh(
                                TriangleMesh {
                                    indices,
                                    vertices: vertices
                                        .iter()
                                        .zip(normal.iter())
                                        .enumerate()
                                        .map(|(i, (position, normal))| Vertex {
                                            position: *position,
                                            normal: *normal,
                                            uv: st
                                                .map(|st| vec2(st[2 * i], st[2 * i + 1]))
                                                .unwrap_or(Vec2::ZERO),
                                        })
                                        .collect(),
                                },
                            ))))
                        } else {
                            Ok(Self::WorldObject(WorldObject::Shape(Shape::TriangleMesh(
                                TriangleMesh {
                                    indices,
                                    vertices: vertices
                                        .iter()
                                        .enumerate()
                                        .map(|(i, position)| Vertex {
                                            position: *position,
                                            normal: Vec3A::ZERO,
                                            uv: st
                                                .map(|st| vec2(st[2 * i], st[2 * i + 1]))
                                                .unwrap_or(Vec2::ZERO),
                                        })
                                        .collect(),
                                },
                            ))))
                        }
                    }
                    t => Err(Error::InvalidShape(t.to_string())),
                },
            },
            pbrt_parser::World::Attribute(worlds) => worlds
                .into_iter()
                .map(Self::from_world)
                .collect::<Result<Vec<Self>, Error>>()
                .map(IntermediateWorld::Attribute),
            pbrt_parser::World::Translate(translation) => {
                Ok(Self::Matrix(Affine3A::from_translation(translation.into())))
            }
            pbrt_parser::World::Scale(scale) => {
                Ok(Self::Matrix(Affine3A::from_scale(scale.into())))
            }
            pbrt_parser::World::Rotate(axis_angle) => Ok(Self::Matrix(Affine3A::from_axis_angle(
                axis_angle.axis.normalize().into(),
                -axis_angle.angle * PI / 180.0,
            ))),
        }
    }
}

impl IntermediateScene {
    pub fn from_scene(scene: pbrt_parser::Scene) -> Result<Self, Error> {
        match scene {
            pbrt_parser::Scene::LookAt(look_at) => Ok(Self::LookAt(LookAt {
                eye: look_at.eye,
                look_at: look_at.look_at,
                up: look_at.up,
            })),
            pbrt_parser::Scene::SceneObject(obj) => match obj.object_type {
                pbrt_parser::SceneObjectType::Sampler => Ok(Self::Sampler),
                pbrt_parser::SceneObjectType::Integrator => Ok(Self::Integrator),
                pbrt_parser::SceneObjectType::Camera => match obj.t {
                    "perspective" => {
                        let fov = obj.get_float("fov").unwrap_or(Ok(90.0))?;
                        Ok(Self::SceneObject(SceneObject::Camera(Camera::Perspective(
                            PerspectiveCamera { fov },
                        ))))
                    }
                    t => Err(Error::InvalidCamera(t.to_string())),
                },
                pbrt_parser::SceneObjectType::Film => match obj.t {
                    "image" => {
                        let filename = obj
                            .get_str("filename")
                            .ok_or(Error::ArgumentNotFound("filename".to_string()))??;
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
                .map(IntermediateWorld::from_world)
                .collect::<Result<Vec<IntermediateWorld>, _>>()
                .map(IntermediateScene::World),
        }
    }
}
