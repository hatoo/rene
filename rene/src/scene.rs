use glam::{vec3a, Affine3A};
use rene_shader::{material::EnumMaterial, LookAt, Uniform};

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

impl Scene {
    pub fn create(scene_description: &[pbrt_parser::Scene]) -> Self {
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
                        assert_eq!(obj.t, "perspective");
                        scene.uniform.camera.fov = obj.get_float("fov").unwrap();
                        dbg!(scene.uniform.camera.fov);
                    }
                },
                pbrt_parser::Scene::World(worlds) => {
                    scene.append_world(&worlds);
                }
            }
        }
        scene
    }

    fn append_world(&mut self, worlds: &[pbrt_parser::World]) {
        let mut current_material_index = None;
        for w in worlds {
            match w {
                pbrt_parser::World::Attribute(worlds) => self.append_world(worlds.as_slice()),
                pbrt_parser::World::WorldObject(obj) => match obj.object_type {
                    pbrt_parser::WorldObjectType::LightSource => {
                        assert_eq!(obj.t, "infinite");
                        self.uniform.background += obj.get_rgb("L").unwrap();
                    }
                    pbrt_parser::WorldObjectType::Material => {
                        current_material_index = Some(self.materials.len());
                        self.materials
                            .push(EnumMaterial::new_lambertian(vec3a(0.8, 0.8, 0.8)));
                    }
                    pbrt_parser::WorldObjectType::Shape => self.tlas.push(TlasInstance {
                        shader_offset: 0,
                        matrix: Default::default(),
                        material_index: current_material_index.unwrap(),
                        blas_index: None,
                    }),
                },
            }
        }
    }
}
