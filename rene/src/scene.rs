use std::{collections::HashMap, f32::consts::PI, path::Path};

use glam::{vec3, vec3a, Affine3A, Mat4};
use rene_shader::{
    area_light::EnumAreaLight, light::EnumLight, material::EnumMaterial, medium::EnumMedium,
    texture::EnumTexture, Uniform,
};
use thiserror::Error;

use crate::ShaderOffset;

use self::intermediate_scene::{
    AreaLightSource, Camera, Film, Glass, Homogeneous, Infinite, InnerTexture, Integrator,
    IntermediateScene, IntermediateWorld, LightSource, Material, Matte, Medium, Metal, Mirror,
    Plastic, SceneObject, Shape, Sphere, Substrate, TextureOrColor, TriangleMesh, Uber,
    WorldObject,
};

pub mod image;
pub mod intermediate_scene;
mod pfm_parser;
mod spectrum;
mod subdivision;

use crate::scene::image::Image;

#[derive(Debug)]
pub struct TlasInstance {
    pub shader_offset: ShaderOffset,
    pub matrix: Affine3A,
    pub material_index: usize,
    pub interior_medium_index: usize,
    pub exterior_medium_index: usize,
    pub area_light_index: usize,
    pub blas_index: Option<usize>,
}

#[derive(Default, Debug)]
pub struct Scene {
    pub integrator: Integrator,
    pub film: Film,
    pub uniform: Uniform,
    pub tlas: Vec<TlasInstance>,
    pub materials: Vec<EnumMaterial>,
    pub mediums: Vec<EnumMedium>,
    pub area_lights: Vec<EnumAreaLight>,
    pub textures: Vec<EnumTexture>,
    pub blases: Vec<TriangleMesh>,
    pub lights: Vec<EnumLight>,
    pub images: Vec<Image>,
}

#[derive(Error, Debug)]
pub enum CreateSceneError {
    #[error("Failed to convert pbrt scene to intermediate type: {0}")]
    IntermediateError(#[from] intermediate_scene::Error),
    #[error("No Material")]
    NoMaterial,
    #[error("Unknown Material {0}")]
    UnknownMaterial(String),
    #[error("Unknown Medium {0}")]
    UnknownMedium(String),
    #[error("Not Found Texture: {0}")]
    NotFoundTexture(String),
    #[error("Not Found Coord system: {0}")]
    NotFoundCoordSystem(String),
}

#[derive(Default, Clone)]
struct WorldState {
    current_material_index: Option<usize>,
    current_medium_index: Option<(usize, usize)>,
    current_area_light_index: usize,
    current_matrix: Mat4,
    textures: HashMap<String, u32>,
    materials: HashMap<String, u32>,
    mediums: HashMap<String, u32>,
    coord_system: HashMap<String, Mat4>,
}

impl Scene {
    fn texture(
        &mut self,
        texture_or_color: TextureOrColor,
        state: &WorldState,
    ) -> Result<u32, CreateSceneError> {
        match texture_or_color {
            TextureOrColor::Color(color) => {
                let texture_index = self.textures.len();
                self.textures.push(EnumTexture::new_solid(color));
                Ok(texture_index as u32)
            }
            TextureOrColor::Texture(name) => state
                .textures
                .get(&name)
                .ok_or(CreateSceneError::NotFoundTexture(name))
                .copied(),
        }
    }

    pub fn create<P: AsRef<Path>>(
        scene_description: Vec<pbrt_parser::Scene>,
        base_dir: &P,
    ) -> Result<Self, CreateSceneError> {
        let mut scene = Self::default();
        let mut wolrd_to_camera = Mat4::default();
        // 90 degree
        let mut fov = 0.5 * PI;

        scene.area_lights.push(EnumAreaLight::new_null());
        scene.mediums.push(EnumMedium::new_vaccum());

        // Default infinite light texture
        scene
            .textures
            .push(EnumTexture::new_solid(vec3a(1.0, 1.0, 1.0)));

        for desc in scene_description {
            match IntermediateScene::from_scene(desc, base_dir)? {
                IntermediateScene::Sampler => {
                    log::info!("Sampler is not yet implemented. Continue.");
                }
                IntermediateScene::Integrator(integrator) => {
                    scene.integrator = integrator;
                }
                IntermediateScene::PixelFilter => {
                    log::info!("PixelFilter is not yet implemented. Continue.");
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
                            fov = p.fov;
                        }
                    },
                },
                IntermediateScene::World(worlds) => {
                    let mut state = WorldState::default();
                    state
                        .coord_system
                        .insert("camera".to_string(), wolrd_to_camera);
                    scene.append_world(&mut state, worlds)?;
                }
            }
        }

        let aspect_ratio = scene.film.xresolution as f32 / scene.film.yresolution as f32;
        if scene.film.yresolution > scene.film.xresolution {
            // TODO remove this ad-hoc
            fov = ((fov * 0.5).tan() / scene.film.xresolution as f32
                * scene.film.yresolution as f32)
                .atan()
                * 2.0;
        }
        scene.uniform.camera.projection =
            Mat4::perspective_lh(fov, aspect_ratio, 0.01, 1000.0).inverse();
        scene.uniform.camera_to_world = wolrd_to_camera.inverse();
        scene.uniform.lights_len = scene.lights.len() as u32;
        Ok(scene)
    }

    fn material(
        &mut self,
        state: &WorldState,
        material: Material,
    ) -> Result<EnumMaterial, CreateSceneError> {
        match material {
            Material::Matte(Matte { albedo }) => {
                let texture_index = self.texture(albedo, state)?;
                Ok(EnumMaterial::new_matte(texture_index))
            }
            Material::Glass(Glass { index }) => Ok(EnumMaterial::new_glass(index)),
            Material::Substrate(Substrate {
                diffuse,
                specular,
                rough_u,
                rough_v,
                remap_roughness,
            }) => {
                let diffuse_index = self.texture(diffuse, state)?;
                let specular_index = self.texture(specular, state)?;

                Ok(EnumMaterial::new_substrate(
                    diffuse_index,
                    specular_index,
                    rough_u,
                    rough_v,
                    remap_roughness,
                ))
            }
            Material::Metal(Metal {
                eta,
                k,
                rough_u,
                rough_v,
                remap_roughness,
            }) => {
                let eta_index = self.texture(eta, state)?;
                let k_index = self.texture(k, state)?;

                Ok(EnumMaterial::new_metal(
                    eta_index,
                    k_index,
                    rough_u,
                    rough_v,
                    remap_roughness,
                ))
            }
            Material::Mirror(Mirror { r }) => {
                let texture_index = self.texture(r, state)?;
                Ok(EnumMaterial::new_mirror(texture_index))
            }
            Material::Uber(Uber {
                kd,
                ks,
                kr,
                kt,
                rough_u,
                rough_v,
                eta,
                opacity,
                remap_roughness,
            }) => Ok(EnumMaterial::new_uber(
                self.texture(kd, state)?,
                self.texture(ks, state)?,
                self.texture(kr, state)?,
                self.texture(kt, state)?,
                rough_u,
                rough_v,
                self.texture(opacity, state)?,
                eta,
                remap_roughness,
            )),
            Material::Plastic(Plastic {
                kd,
                ks,
                rough,
                remap_roughness,
            }) => Ok(EnumMaterial::new_plastic(
                self.texture(kd, state)?,
                self.texture(ks, state)?,
                rough,
                remap_roughness,
            )),
            Material::None => Ok(EnumMaterial::new_none()),
        }
    }

    fn append_world(
        &mut self,
        state: &mut WorldState,
        worlds: Vec<IntermediateWorld>,
    ) -> Result<(), CreateSceneError> {
        for w in worlds {
            match w {
                IntermediateWorld::Attribute(worlds) => {
                    let mut state = state.clone();
                    self.append_world(&mut state, worlds)?
                }
                IntermediateWorld::TransformBeginEnd(worlds) => {
                    let matrix = state.current_matrix;
                    self.append_world(state, worlds)?;
                    state.current_matrix = matrix;
                }
                IntermediateWorld::Matrix(m) => {
                    state.current_matrix *= m;
                }
                IntermediateWorld::NamedMaterial(name) => {
                    state.current_material_index = Some(
                        *state
                            .materials
                            .get(&name)
                            .ok_or(CreateSceneError::UnknownMaterial(name))?
                            as usize,
                    );
                }
                IntermediateWorld::CoordSysTransform(name) => {
                    if let Some(mat) = state.coord_system.get(&name) {
                        state.current_matrix = *mat;
                    } else {
                        return Err(CreateSceneError::NotFoundCoordSystem(name));
                    }
                }
                IntermediateWorld::MediumInterface(interior, exterior) => {
                    state.current_medium_index = Some((
                        if interior == "" {
                            0
                        } else {
                            *state
                                .mediums
                                .get(&interior)
                                .ok_or(CreateSceneError::UnknownMedium(interior))?
                                as usize
                        },
                        if exterior == "" {
                            0
                        } else {
                            *state
                                .mediums
                                .get(&exterior)
                                .ok_or(CreateSceneError::UnknownMedium(exterior))?
                                as usize
                        },
                    ));
                }
                IntermediateWorld::Texture(texture) => {
                    let inner = match texture.inner {
                        InnerTexture::Constant(value) => EnumTexture::new_solid(value),
                        InnerTexture::CheckerBoard(checkerboard) => {
                            let tex1 = self.texture(checkerboard.tex1, state)?;
                            let tex2 = self.texture(checkerboard.tex2, state)?;
                            EnumTexture::new_checkerboard(
                                tex1,
                                tex2,
                                checkerboard.uscale,
                                checkerboard.vscale,
                            )
                        }
                        InnerTexture::ImageMap(image) => {
                            let image_index = self.images.len();
                            self.images.push(image);
                            EnumTexture::new_image_map(image_index as u32)
                        }
                    };
                    let texture_index = self.textures.len();
                    self.textures.push(inner);
                    state.textures.insert(texture.name, texture_index as u32);
                }
                IntermediateWorld::WorldObject(obj) => {
                    match obj {
                        WorldObject::LightSource(lightsource) => match lightsource {
                            LightSource::Infinite(Infinite { color, image_map }) => {
                                self.uniform.background_color = color.extend(0.0);

                                if let Some(image) = image_map {
                                    let image_index = self.images.len();
                                    self.images.push(image);

                                    let texture_index = self.textures.len();
                                    self.textures
                                        .push(EnumTexture::new_image_map(image_index as u32));

                                    self.uniform.background_matrix = state.current_matrix.inverse();
                                    self.uniform.background_texture = texture_index as u32;
                                }
                            }
                            LightSource::Distant(distant) => self.lights.push(
                                EnumLight::new_distant(distant.from, distant.to, distant.color),
                            ),
                        },
                        WorldObject::AreaLightSource(AreaLightSource::Diffuse(l)) => {
                            state.current_area_light_index = self.area_lights.len();
                            self.area_lights.push(EnumAreaLight::new_diffuse(l));
                        }
                        WorldObject::Material(material) => {
                            let material = self.material(state, material)?;
                            state.current_material_index = Some(self.materials.len());
                            self.materials.push(material);
                        }
                        WorldObject::MakeNamedMaterial(name, material) => {
                            let material = self.material(state, material)?;
                            state.materials.insert(name, self.materials.len() as u32);
                            state.current_material_index = Some(self.materials.len());
                            self.materials.push(material);
                        }
                        WorldObject::MakeNamedMedium(
                            name,
                            Medium::Homogeneous(Homogeneous {
                                sigma_a,
                                sigma_s,
                                g,
                            }),
                        ) => {
                            let medium = EnumMedium::new_homogeneous(sigma_a, sigma_s, g);
                            state.mediums.insert(name, self.mediums.len() as u32);
                            self.mediums.push(medium);
                        }
                        WorldObject::Shape(shape) => match shape {
                            Shape::Sphere(Sphere { radius }) => self.tlas.push(TlasInstance {
                                shader_offset: ShaderOffset::Sphere,
                                matrix: Affine3A::from_mat4(
                                    state.current_matrix
                                        * Mat4::from_scale(vec3(radius, radius, radius)),
                                ),
                                material_index: state
                                    .current_material_index
                                    .ok_or(CreateSceneError::NoMaterial)?,
                                area_light_index: state.current_area_light_index,
                                blas_index: None,
                                interior_medium_index: state
                                    .current_medium_index
                                    .map(|t| t.0)
                                    .unwrap_or(0),
                                exterior_medium_index: state
                                    .current_medium_index
                                    .map(|t| t.1)
                                    .unwrap_or(0),
                            }),
                            Shape::TriangleMesh(trianglemesh) => {
                                let blass_index = self.blases.len();
                                self.blases.push(trianglemesh);
                                self.tlas.push(TlasInstance {
                                    shader_offset: ShaderOffset::Triangle,
                                    matrix: Affine3A::from_mat4(state.current_matrix),
                                    material_index: state
                                        .current_material_index
                                        .ok_or(CreateSceneError::NoMaterial)?,
                                    area_light_index: state.current_area_light_index,
                                    interior_medium_index: state
                                        .current_medium_index
                                        .map(|t| t.0)
                                        .unwrap_or(0),
                                    exterior_medium_index: state
                                        .current_medium_index
                                        .map(|t| t.1)
                                        .unwrap_or(0),
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
