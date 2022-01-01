use std::borrow::Cow;

use glam::{vec3a, Affine3A, Vec3A};
use pbrt_parser::ArgumentError;
use rene_shader::{camera::PerspectiveCamera, Vertex};
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
}

pub enum WorldObject {
    LightSource(LightSource),
    Material(Material),
    Shape(Shape),
}

pub enum LightSource {
    Infinite(Infinite),
}

pub struct Infinite {
    pub color: Vec3A,
}

pub enum Material {
    Matte(Matte),
}

pub struct Matte {
    pub color: Vec3A,
}

pub enum Shape {
    Sphere(Sphere),
    TriangleMesh(TriangleMesh),
}

pub struct Sphere {
    pub radius: f32,
}

#[derive(Clone)]
pub struct TriangleMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub enum IntermediateScene {
    LookAt(LookAt),
    SceneObject(SceneObject),
    World(Vec<IntermediateWorld>),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid Camera type {0}")]
    InvalidCamera(String),
    #[error("Invalid LightSource type {0}")]
    InvalidLightSource(String),
    #[error("Invalid Material type {0}")]
    InvalidMaterial(String),
    #[error("Invalid Shape type {0}")]
    InvalidShape(String),
    #[error("Invalid Argument")]
    InvalidArgument(#[from] ArgumentError),
    #[error("Argument not found {0}")]
    ArgumentNotFound(String),
}

impl IntermediateWorld {
    fn from_world(world: &pbrt_parser::World) -> Result<Self, Error> {
        match world {
            pbrt_parser::World::WorldObject(obj) => match obj.object_type {
                pbrt_parser::WorldObjectType::LightSource => match obj.t {
                    "infinite" => {
                        let color = obj.get_rgb("L").unwrap_or(Ok(vec3a(1.0, 1.0, 1.0)))?;
                        Ok(Self::WorldObject(WorldObject::LightSource(
                            LightSource::Infinite(Infinite { color }),
                        )))
                    }
                    t => Err(Error::InvalidLightSource(t.to_string())),
                },
                pbrt_parser::WorldObjectType::Material => match obj.t {
                    "matte" => {
                        let color = obj.get_rgb("Kd").unwrap_or(Ok(vec3a(0.5, 0.5, 0.5)))?;
                        Ok(Self::WorldObject(WorldObject::Material(Material::Matte(
                            Matte { color },
                        ))))
                    }
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
                            .map(|r| r.map(|v| Cow::Borrowed(v)))
                            .unwrap_or_else(|| {
                                let mut normal = vec![Vec3A::ZERO; vertices.len()];
                                for prim in indices.chunks(3) {
                                    let v0 = vertices[prim[0] as usize];
                                    let v1 = vertices[prim[1] as usize];
                                    let v2 = vertices[prim[2] as usize];

                                    normal[prim[0] as usize] = (normal[prim[0] as usize]
                                        + (v1 - v0).cross(v2 - v0).normalize())
                                    .normalize();
                                    normal[prim[1] as usize] = (normal[prim[1] as usize]
                                        + (v2 - v1).cross(v0 - v1).normalize())
                                    .normalize();
                                    normal[prim[2] as usize] = (normal[prim[2] as usize]
                                        + (v0 - v2).cross(v1 - v2).normalize())
                                    .normalize();
                                }
                                Ok(Cow::Owned(normal))
                            })?;

                        // TODO check length

                        Ok(Self::WorldObject(WorldObject::Shape(Shape::TriangleMesh(
                            TriangleMesh {
                                indices,
                                vertices: vertices
                                    .iter()
                                    .zip(normal.iter())
                                    .map(|(position, normal)| Vertex {
                                        position: *position,
                                        normal: *normal,
                                    })
                                    .collect(),
                            },
                        ))))
                    }
                    t => Err(Error::InvalidShape(t.to_string())),
                },
            },
            pbrt_parser::World::Attribute(worlds) => worlds
                .into_iter()
                .map(Self::from_world)
                .collect::<Result<Vec<Self>, Error>>()
                .map(IntermediateWorld::Attribute),
            pbrt_parser::World::Translate(translation) => Ok(Self::Matrix(
                Affine3A::from_translation((*translation).into()),
            )),
        }
    }
}

impl IntermediateScene {
    pub fn from_scene(scene: &pbrt_parser::Scene) -> Result<Self, Error> {
        match scene {
            pbrt_parser::Scene::LookAt(look_at) => Ok(Self::LookAt(LookAt {
                eye: look_at.eye,
                look_at: look_at.look_at,
                up: look_at.up,
            })),
            pbrt_parser::Scene::SceneObject(obj) => match obj.object_type {
                pbrt_parser::SceneObjectType::Camera => match obj.t {
                    "perspective" => {
                        let fov = obj.get_float("fov").unwrap_or(Ok(90.0))?;
                        Ok(Self::SceneObject(SceneObject::Camera(Camera::Perspective(
                            PerspectiveCamera { fov },
                        ))))
                    }
                    t => Err(Error::InvalidCamera(t.to_string())),
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
