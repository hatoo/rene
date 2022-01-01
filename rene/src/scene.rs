use glam::{vec3, Affine3A};
use rene_shader::{material::EnumMaterial, LookAt, Uniform};
use thiserror::Error;

use crate::ShaderIndex;

use self::intermediate_scene::{
    Camera, Infinite, IntermediateScene, IntermediateWorld, LightSource, Material, Matte,
    SceneObject, Shape, Sphere, TriangleMesh, WorldObject,
};

pub mod intermediate_scene;

#[derive(Debug)]
pub struct TlasInstance {
    pub shader_offset: u32,
    pub matrix: Affine3A,
    pub material_index: usize,
    pub blas_index: Option<usize>,
}

#[derive(Default)]
pub struct Scene {
    pub uniform: Uniform,
    pub tlas: Vec<TlasInstance>,
    pub materials: Vec<EnumMaterial>,
    pub blases: Vec<TriangleMesh>,
}

#[derive(Error, Debug)]
pub enum CreateSceneError {
    #[error("Failed to convert to intermediate type")]
    IntermediateError(#[from] intermediate_scene::Error),
    #[error("No Material")]
    NoMaterial,
}

#[derive(Default, Clone, Copy)]
struct WorldState {
    current_material_index: Option<usize>,
    current_matrix: Affine3A,
}

impl Scene {
    pub fn create(scene_description: Vec<pbrt_parser::Scene>) -> Result<Self, CreateSceneError> {
        let mut scene = Self::default();
        for desc in scene_description {
            match IntermediateScene::from_scene(desc)? {
                IntermediateScene::LookAt(intermediate_scene::LookAt { eye, look_at, up }) => {
                    scene.uniform.look_at = LookAt { eye, look_at, up };
                }
                IntermediateScene::SceneObject(obj) => match obj {
                    SceneObject::Camera(camera) => match camera {
                        Camera::Perspective(p) => {
                            scene.uniform.camera.fov = p.fov;
                        }
                    },
                },
                IntermediateScene::World(worlds) => {
                    scene.append_world(Default::default(), worlds)?;
                }
            }
        }
        Ok(scene)
    }

    fn append_world(
        &mut self,
        mut state: WorldState,
        worlds: Vec<IntermediateWorld>,
    ) -> Result<(), CreateSceneError> {
        for w in worlds {
            match w {
                IntermediateWorld::Attribute(worlds) => self.append_world(state, worlds)?,
                IntermediateWorld::Matrix(m) => {
                    state.current_matrix = state.current_matrix * m;
                }
                IntermediateWorld::WorldObject(obj) => match obj {
                    WorldObject::LightSource(lightsource) => match lightsource {
                        LightSource::Infinite(Infinite { color }) => {
                            self.uniform.background += color;
                        }
                    },
                    WorldObject::Material(material) => match material {
                        Material::Matte(Matte { color }) => {
                            state.current_material_index = Some(self.materials.len());
                            self.materials.push(EnumMaterial::new_lambertian(color));
                        }
                    },
                    WorldObject::Shape(shape) => match shape {
                        Shape::Sphere(Sphere { radius }) => self.tlas.push(TlasInstance {
                            shader_offset: ShaderIndex::SPHERE,
                            matrix: state.current_matrix
                                * Affine3A::from_scale(vec3(radius, radius, radius)),
                            material_index: state
                                .current_material_index
                                .ok_or(CreateSceneError::NoMaterial)?,
                            blas_index: None,
                        }),
                        Shape::TriangleMesh(trianglemesh) => {
                            let blass_index = self.blases.len();
                            self.blases.push(trianglemesh);
                            self.tlas.push(TlasInstance {
                                shader_offset: ShaderIndex::TRIANGLE,
                                matrix: state.current_matrix,
                                material_index: state
                                    .current_material_index
                                    .ok_or(CreateSceneError::NoMaterial)?,
                                blas_index: Some(blass_index),
                            })
                        }
                    },
                },
            }
        }
        Ok(())
    }
}
