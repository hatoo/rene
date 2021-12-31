use glam::{vec3a, Affine3A, Vec3A};
use pbrt_parser::ArgumentError;
use rene_shader::camera::PerspectiveCamera;
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
}

pub struct Sphere {
    pub radius: f32,
}

pub enum IntermediateScene {
    LookAt(LookAt),
    SceneObject(SceneObject),
    World(Vec<IntermediateWorld>),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid Camera type")]
    InvalidCamera(String),
    #[error("Invalid LightSource type")]
    InvalidLightSource(String),
    #[error("Invalid Material type")]
    InvalidMaterial(String),
    #[error("Invalid Shape type")]
    InvalidShape(String),
    #[error("Invalid Argument")]
    InvalidArgument(#[from] ArgumentError),
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
