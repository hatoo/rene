use std::collections::HashMap;

use glam::{vec3, Affine3A, Mat4};
use rene_shader::{light::EnumLight, material::EnumMaterial, texture::EnumTexture, Uniform};
use thiserror::Error;

use crate::ShaderIndex;

use self::intermediate_scene::{
    Camera, Film, Infinite, InnerTexture, IntermediateScene, IntermediateWorld, LightSource,
    Material, Matte, SceneObject, Shape, Sphere, TextureOrColor, TriangleMesh, WorldObject,
};

pub mod intermediate_scene;

#[derive(Debug)]
pub struct TlasInstance {
    pub shader_offset: u32,
    pub matrix: Affine3A,
    pub material_index: usize,
    pub blas_index: Option<usize>,
}

#[derive(Default, Debug)]
pub struct Scene {
    pub film: Film,
    pub uniform: Uniform,
    pub tlas: Vec<TlasInstance>,
    pub materials: Vec<EnumMaterial>,
    pub textures: Vec<EnumTexture>,
    pub blases: Vec<TriangleMesh>,
    pub lights: Vec<EnumLight>,
}

#[derive(Error, Debug)]
pub enum CreateSceneError {
    #[error("Failed to convert pbrt scene to intermediate type: {0}")]
    IntermediateError(#[from] intermediate_scene::Error),
    #[error("No Material")]
    NoMaterial,
    #[error("Not Found Texture: {0}")]
    NotFoundTexture(String),
}

#[derive(Default, Clone)]
struct WorldState {
    current_material_index: Option<usize>,
    current_matrix: Affine3A,
    textures: HashMap<String, u32>,
}

impl Scene {
    pub fn create(scene_description: Vec<pbrt_parser::Scene>) -> Result<Self, CreateSceneError> {
        let mut scene = Self::default();
        let mut wolrd_to_camera = Mat4::default();
        let mut fov = 90.0;

        for desc in scene_description {
            match IntermediateScene::from_scene(desc)? {
                IntermediateScene::Sampler => {
                    log::info!("Sampler is not yet implemented. Continue.");
                }
                IntermediateScene::Integrator => {
                    log::info!("Integrator is not yet implemented. Continue.");
                }
                IntermediateScene::Film(film) => {
                    scene.film = film;
                }
                IntermediateScene::Matrix(m) => {
                    wolrd_to_camera *= m;
                }
                IntermediateScene::SceneObject(obj) => match obj {
                    SceneObject::Camera(camera) => match camera {
                        Camera::Perspective(p) => {
                            fov = p.fov / 180.0 * core::f32::consts::PI;
                        }
                    },
                },
                IntermediateScene::World(worlds) => {
                    scene.append_world(Default::default(), worlds)?;
                }
            }
        }

        let aspect_ratio = scene.film.xresolution as f32 / scene.film.yresolution as f32;
        if scene.film.yresolution > scene.film.xresolution {
            fov = ((fov * 0.5).tan() / scene.film.xresolution as f32
                * scene.film.yresolution as f32)
                .atan()
                * 2.0;
        }
        scene.uniform.camera.camera_to_screen =
            Mat4::perspective_lh(fov, aspect_ratio, 0.01, 1000.0).inverse();
        scene.uniform.camera_to_world = wolrd_to_camera.inverse();
        scene.uniform.lights_len = scene.lights.len() as u32;
        Ok(scene)
    }

    fn append_world(
        &mut self,
        mut state: WorldState,
        worlds: Vec<IntermediateWorld>,
    ) -> Result<(), CreateSceneError> {
        for w in worlds {
            match w {
                IntermediateWorld::Attribute(worlds) => self.append_world(state.clone(), worlds)?,
                IntermediateWorld::Matrix(m) => {
                    state.current_matrix = state.current_matrix * m;
                }
                IntermediateWorld::Texture(texture) => {
                    let inner = match texture.inner {
                        InnerTexture::CheckerBoard(checkerboard) => {
                            let tex1 = match checkerboard.tex1 {
                                TextureOrColor::Color(color) => {
                                    let texture_index = self.textures.len();
                                    self.textures.push(EnumTexture::new_solid(color));
                                    texture_index as u32
                                }
                                TextureOrColor::Texture(name) => *state
                                    .textures
                                    .get(&name)
                                    .ok_or(CreateSceneError::NotFoundTexture(name))?,
                            };
                            let tex2 = match checkerboard.tex2 {
                                TextureOrColor::Color(color) => {
                                    let texture_index = self.textures.len();
                                    self.textures.push(EnumTexture::new_solid(color));
                                    texture_index as u32
                                }
                                TextureOrColor::Texture(name) => *state
                                    .textures
                                    .get(&name)
                                    .ok_or(CreateSceneError::NotFoundTexture(name))?,
                            };
                            EnumTexture::new_checkerboard(
                                tex1,
                                tex2,
                                checkerboard.uscale,
                                checkerboard.vscale,
                            )
                        }
                    };
                    let texture_index = self.textures.len();
                    self.textures.push(inner);
                    state.textures.insert(texture.name, texture_index as u32);
                }
                IntermediateWorld::WorldObject(obj) => {
                    match obj {
                        WorldObject::LightSource(lightsource) => match lightsource {
                            LightSource::Infinite(Infinite { color }) => {
                                self.uniform.background += color;
                            }
                            LightSource::Distant(distant) => self.lights.push(
                                EnumLight::new_distant(distant.from, distant.to, distant.color),
                            ),
                        },
                        WorldObject::Material(material) => match material {
                            Material::Matte(Matte { albedo }) => {
                                let texture_index = match albedo {
                                    TextureOrColor::Color(color) => {
                                        let texture_index = self.textures.len();
                                        self.textures.push(EnumTexture::new_solid(color));
                                        texture_index as u32
                                    }
                                    TextureOrColor::Texture(name) => *state
                                        .textures
                                        .get(&name)
                                        .ok_or(CreateSceneError::NotFoundTexture(name))?,
                                };

                                state.current_material_index = Some(self.materials.len());
                                self.materials
                                    .push(EnumMaterial::new_lambertian(texture_index));
                            }
                            Material::Glass => {
                                state.current_material_index = Some(self.materials.len());
                                self.materials.push(EnumMaterial::new_dielectric(1.5));
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
                    }
                }
            }
        }
        Ok(())
    }
}
