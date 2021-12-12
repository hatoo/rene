use glam::{vec3a, Affine3A};
use rene_shader::{material::EnumMaterial, LookAt, Uniform};
use thiserror::Error;

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
}

#[derive(Error, Debug)]
pub enum CreateSceneError<'a> {
    #[error("Invalid Camera type")]
    InvalidCamera(&'a str),
    #[error("Invalid LightSource type")]
    InvalidLightSource(&'a str),
    #[error("Invalid Material type")]
    InvalidMaterial(&'a str),
    #[error("No Material")]
    NoMaterial,
}

#[derive(Default, Clone, Copy)]
struct WorldState {
    current_material_index: Option<usize>,
    current_matrix: Affine3A,
}

impl Scene {
    pub fn create<'a>(
        scene_description: &[pbrt_parser::Scene<'a>],
    ) -> Result<Self, CreateSceneError<'a>> {
        let mut scene = Self::default();
        for desc in scene_description {
            match desc {
                pbrt_parser::Scene::LookAt(pbrt_parser::LookAt { eye, look_at, up }) => {
                    scene.uniform.look_at = LookAt {
                        eye: *eye,
                        look_at: *look_at,
                        up: *up,
                    };
                }
                pbrt_parser::Scene::SceneObject(obj) => match obj.object_type {
                    pbrt_parser::SceneObjectType::Camera => {
                        if obj.t != "perspective" {
                            return Err(CreateSceneError::InvalidCamera(obj.t));
                        }
                        scene.uniform.camera.fov = obj.get_float("fov").unwrap_or(90.0);
                    }
                },
                pbrt_parser::Scene::World(worlds) => {
                    scene.append_world(Default::default(), &worlds)?;
                }
            }
        }
        Ok(scene)
    }

    fn append_world<'a>(
        &mut self,
        mut state: WorldState,
        worlds: &[pbrt_parser::World<'a>],
    ) -> Result<(), CreateSceneError<'a>> {
        for w in worlds {
            match w {
                pbrt_parser::World::Attribute(worlds) => {
                    self.append_world(state, worlds.as_slice())?
                }
                pbrt_parser::World::Translate(v) => {
                    state.current_matrix.w_axis += *v;
                }
                pbrt_parser::World::WorldObject(obj) => match obj.object_type {
                    pbrt_parser::WorldObjectType::LightSource => {
                        if obj.t != "infinite" {
                            return Err(CreateSceneError::InvalidLightSource(obj.t));
                        }
                        self.uniform.background += obj.get_rgb("L").unwrap_or(vec3a(1.0, 1.0, 1.0));
                    }
                    pbrt_parser::WorldObjectType::Material => {
                        if obj.t != "matte" {
                            return Err(CreateSceneError::InvalidMaterial(obj.t));
                        }
                        state.current_material_index = Some(self.materials.len());
                        self.materials.push(EnumMaterial::new_lambertian(
                            obj.get_rgb("Kd").unwrap_or(vec3a(0.5, 0.5, 0.5)),
                        ));
                    }
                    pbrt_parser::WorldObjectType::Shape => self.tlas.push(TlasInstance {
                        shader_offset: 0,
                        matrix: state.current_matrix,
                        material_index: state
                            .current_material_index
                            .ok_or(CreateSceneError::NoMaterial)?,
                        blas_index: None,
                    }),
                },
            }
        }
        Ok(())
    }
}
