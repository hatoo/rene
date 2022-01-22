use std::{
    borrow::Cow,
    collections::HashSet,
    ffi::{c_void, CStr, CString},
    fs::File,
    io::Read,
    os::raw::c_char,
    path::PathBuf,
    ptr::{self, null},
};

use ash::{
    extensions::khr::AccelerationStructure,
    prelude::VkResult,
    util::Align,
    vk::{self, AccelerationStructureKHR},
};

use clap::{ArgEnum, Parser};
use glam::{Vec2, Vec3A};
use image::{DynamicImage, GenericImageView};
use nom::error::convert_error;
use pbrt_parser::include::expand_include;
use rand::prelude::*;
use rene_shader::{
    area_light::EnumAreaLight, light::EnumLight, material::EnumMaterial,
    surface_sample::SurfaceSample, texture::EnumTexture, IndexData, Uniform, Vertex,
};
use scene::Scene;

mod scene;

#[derive(Debug, Clone, Copy)]
pub enum ShaderOffset {
    Triangle = 0,
    Sphere = 1,
}

#[derive(ArgEnum, Debug, PartialEq, Eq, Clone, Copy)]
enum Denoiser {
    None,
    Optix,
    Oidn,
}

#[derive(Parser)]
struct Opts {
    #[clap(help = "pbrt file")]
    pbrt_path: PathBuf,
    #[clap(help = "AOV normal", long = "aov-normal")]
    aov_normal: Option<PathBuf>,
    #[clap(help = "AOV albedo", long = "aov-albedo")]
    aov_albedo: Option<PathBuf>,
    #[clap(
        arg_enum,
        help = "Set Denoiser",
        long = "denoiser",
        default_value = "none"
    )]
    denoiser: Denoiser,
}

fn main() {
    simple_logger::init().unwrap();

    const ENABLE_VALIDATION_LAYER: bool = true;
    const COLOR_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;

    const N_SAMPLES: u32 = 5000;
    const N_SAMPLES_ITER: u32 = 100;

    let mut opts: Opts = Opts::parse();
    let mut pbrt_file = String::new();

    #[cfg(not(feature = "optix-denoiser"))]
    if opts.denoiser == Denoiser::Optix {
        log::warn!(
            "Optix Denoiser was enabled but built without \"optix-denoiser\" feature. Ignore."
        );
    }

    #[cfg(not(feature = "oidn-denoiser"))]
    if opts.denoiser == Denoiser::Oidn {
        log::warn!(
            "Oidn Denoiser was enabled but built without \"oidn-denoiser\" feature. Ignore."
        );
    }

    File::open(&opts.pbrt_path)
        .unwrap()
        .read_to_string(&mut pbrt_file)
        .unwrap();

    opts.pbrt_path.pop();

    match expand_include(pbrt_file.as_str(), &opts.pbrt_path).unwrap() {
        Cow::Borrowed(_) => {}
        Cow::Owned(s) => pbrt_file = s,
    }

    let parsed_scene = match pbrt_parser::parse_pbrt(&pbrt_file) {
        Ok(scene) => scene,
        Err(e) => {
            println!("{}", convert_error(pbrt_file.as_str(), e));
            return;
        }
    };
    let scene = match scene::Scene::create(parsed_scene, &opts.pbrt_path) {
        Ok(scene) => scene,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    let validation_layers: Vec<CString> = if ENABLE_VALIDATION_LAYER {
        vec![CString::new("VK_LAYER_KHRONOS_validation").unwrap()]
    } else {
        Vec::new()
    };
    let validation_layers_ptr: Vec<*const i8> = validation_layers
        .iter()
        .map(|c_str| c_str.as_ptr())
        .collect();

    let entry = unsafe { ash::Entry::load() }.unwrap();

    assert_eq!(
        check_validation_layer_support(
            &entry,
            validation_layers.iter().map(|cstring| cstring.as_c_str())
        ),
        Ok(true)
    );

    let instance = {
        let application_name = CString::new("Hello Triangle").unwrap();
        let engine_name = CString::new("No Engine").unwrap();

        let mut debug_utils_create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING |
            // vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE |
            // vk::DebugUtilsMessageSeverityFlagsEXT::INFO |
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
            )
            .pfn_user_callback(Some(default_vulkan_debug_utils_callback))
            .build();

        let application_info = vk::ApplicationInfo::builder()
            .application_name(application_name.as_c_str())
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(engine_name.as_c_str())
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_2)
            .build();

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_info)
            .enabled_layer_names(validation_layers_ptr.as_slice());

        let instance_create_info = if ENABLE_VALIDATION_LAYER {
            instance_create_info.push_next(&mut debug_utils_create_info)
        } else {
            instance_create_info
        }
        .build();

        unsafe { entry.create_instance(&instance_create_info, None) }
            .expect("failed to create instance!")
    };

    let (physical_device, queue_family_index) = pick_physical_device_and_queue_family_indices(
        &instance,
        &[
            ash::extensions::khr::AccelerationStructure::name(),
            ash::extensions::khr::DeferredHostOperations::name(),
            ash::extensions::khr::RayTracingPipeline::name(),
        ],
    )
    .unwrap()
    .unwrap();

    let device: ash::Device = {
        let priorities = [1.0];

        let queue_create_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities)
            .build();

        let mut features2 = vk::PhysicalDeviceFeatures2::default();
        unsafe {
            instance
                .fp_v1_1()
                .get_physical_device_features2(physical_device, &mut features2)
        };

        let mut features12 = vk::PhysicalDeviceVulkan12Features::builder()
            .shader_int8(true)
            .buffer_device_address(true)
            .vulkan_memory_model(true)
            .runtime_descriptor_array(true)
            .build();

        let mut as_feature = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::builder()
            .acceleration_structure(true)
            .build();

        let mut raytracing_pipeline = vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::builder()
            .ray_tracing_pipeline(true)
            .build();

        let queue_create_infos = [queue_create_info];
        let enabled_extension_names = [
            ash::extensions::khr::RayTracingPipeline::name().as_ptr(),
            ash::extensions::khr::AccelerationStructure::name().as_ptr(),
            ash::extensions::khr::DeferredHostOperations::name().as_ptr(),
            vk::KhrSpirv14Fn::name().as_ptr(),
            vk::ExtScalarBlockLayoutFn::name().as_ptr(),
            vk::KhrGetMemoryRequirements2Fn::name().as_ptr(),
        ];

        let device_create_info = vk::DeviceCreateInfo::builder()
            .push_next(&mut features2)
            .push_next(&mut features12)
            .push_next(&mut as_feature)
            .push_next(&mut raytracing_pipeline)
            .queue_create_infos(&queue_create_infos)
            .enabled_layer_names(validation_layers_ptr.as_slice())
            .enabled_extension_names(&enabled_extension_names)
            .build();

        unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .expect("Failed to create logical Device!")
    };

    let mut rt_pipeline_properties = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();

    {
        let mut physical_device_properties2 = vk::PhysicalDeviceProperties2::builder()
            .push_next(&mut rt_pipeline_properties)
            .build();

        unsafe {
            instance
                .get_physical_device_properties2(physical_device, &mut physical_device_properties2);
        }
    }
    let acceleration_structure =
        ash::extensions::khr::AccelerationStructure::new(&instance, &device);

    let rt_pipeline = ash::extensions::khr::RayTracingPipeline::new(&instance, &device);

    let graphics_queue = unsafe { device.get_device_queue(queue_family_index, 0) };

    let command_pool = {
        let command_pool_create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .build();

        unsafe { device.create_command_pool(&command_pool_create_info, None) }
            .expect("Failed to create Command Pool!")
    };

    let device_memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    let image = {
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(COLOR_FORMAT)
            .extent(
                vk::Extent3D::builder()
                    .width(scene.film.xresolution)
                    .height(scene.film.yresolution)
                    .depth(1)
                    .build(),
            )
            .mip_levels(1)
            .array_layers(3)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        unsafe { device.create_image(&image_create_info, None) }.unwrap()
    };

    let device_memory = {
        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };
        let mem_alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_reqs.size)
            .memory_type_index(get_memory_type_index(
                device_memory_properties,
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ));

        unsafe { device.allocate_memory(&mem_alloc_info, None) }.unwrap()
    };

    unsafe { device.bind_image_memory(image, device_memory, 0) }.unwrap();

    let image_view = {
        let image_view_create_info = vk::ImageViewCreateInfo::builder()
            .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
            .format(COLOR_FORMAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 3,
            })
            .image(image)
            .build();

        unsafe { device.create_image_view(&image_view_create_info, None) }.unwrap()
    };

    {
        let command_buffer = {
            let allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            let command_buffers =
                unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap();
            command_buffers[0]
        };

        unsafe {
            device.begin_command_buffer(
                command_buffer,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                    .build(),
            )
        }
        .unwrap();

        let image_barrier = vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::empty())
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(3)
                    .build(),
            )
            .build();

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier],
            );

            device.end_command_buffer(command_buffer).unwrap();
        }

        let command_buffers = [command_buffer];

        let submit_infos = [vk::SubmitInfo::builder()
            .command_buffers(&command_buffers)
            .build()];

        unsafe {
            device
                .queue_submit(graphics_queue, &submit_infos, vk::Fence::null())
                .expect("Failed to execute queue submit.");

            device.queue_wait_idle(graphics_queue).unwrap();
            device.free_command_buffers(command_pool, &[command_buffer]);
        }
    }

    let scene_buffers = SceneBuffers::new(
        &scene,
        &device,
        device_memory_properties,
        &acceleration_structure,
        command_pool,
        graphics_queue,
    );

    let (descriptor_set_layout, graphics_pipeline, pipeline_layout, shader_groups_len) = {
        let descriptor_set_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::builder()
                    .bindings(&[
                        // Scene global data
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                            .stage_flags(
                                vk::ShaderStageFlags::RAYGEN_KHR | vk::ShaderStageFlags::MISS_KHR,
                            )
                            .binding(0)
                            .build(),
                        // TLAS
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(2)
                            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(1)
                            .build(),
                        // output image
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(2)
                            .build(),
                        // lights
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(3)
                            .build(),
                        // area lights
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(4)
                            .build(),
                        // emit objects AABB
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(5)
                            .build(),
                        // materials
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(6)
                            .build(),
                        // textures
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(
                                vk::ShaderStageFlags::RAYGEN_KHR
                                    | vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                            )
                            .binding(7)
                            .build(),
                        // images
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(scene_buffers.images.len() as u32)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
                            .binding(8)
                            .build(),
                        // index data
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                            .binding(9)
                            .build(),
                        // indices
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(
                                vk::ShaderStageFlags::CLOSEST_HIT_KHR
                                    | vk::ShaderStageFlags::RAYGEN_KHR,
                            )
                            .binding(10)
                            .build(),
                        // vertices
                        vk::DescriptorSetLayoutBinding::builder()
                            .descriptor_count(1)
                            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                            .stage_flags(
                                vk::ShaderStageFlags::CLOSEST_HIT_KHR
                                    | vk::ShaderStageFlags::RAYGEN_KHR,
                            )
                            .binding(11)
                            .build(),
                    ])
                    .build(),
                None,
            )
        }
        .unwrap();

        let push_constant_range = vk::PushConstantRange::builder()
            .offset(0)
            .size(4)
            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
            .build();

        const SHADER: &[u8] = include_bytes!(env!("rene_shader.spv"));

        let shader_module = unsafe { create_shader_module(&device, SHADER).unwrap() };

        let layouts = [descriptor_set_layout];
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .push_constant_ranges(&[push_constant_range])
            .build();

        let pipeline_layout =
            unsafe { device.create_pipeline_layout(&layout_create_info, None) }.unwrap();

        let shader_groups = vec![
            // group0 = [ raygen ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(0)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR)
                .build(),
            // group1 = [ miss ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(1)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR)
                .build(),
            // group2 = [ miss ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(5)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR)
                .build(),
            // group3 = [ triangle ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(4)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR)
                .build(),
            // group4 = [ sphere ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::PROCEDURAL_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(3)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(2)
                .build(),
            // group5 = [ triangle ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(6)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR)
                .build(),
            // group6 = [ sphere ]
            vk::RayTracingShaderGroupCreateInfoKHR::builder()
                .ty(vk::RayTracingShaderGroupTypeKHR::PROCEDURAL_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(7)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(2)
                .build(),
        ];

        let shader_stages = vec![
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::RAYGEN_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"main_ray_generation\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::MISS_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"main_miss\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::INTERSECTION_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"sphere_intersection\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"sphere_closest_hit\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"triangle_closest_hit\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::MISS_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"main_miss_pdf\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"triangle_closest_hit_pdf\0").unwrap())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                .module(shader_module)
                .name(std::ffi::CStr::from_bytes_with_nul(b"sphere_closest_hit_pdf\0").unwrap())
                .build(),
        ];

        let pipeline = unsafe {
            rt_pipeline.create_ray_tracing_pipelines(
                vk::DeferredOperationKHR::null(),
                vk::PipelineCache::null(),
                &[vk::RayTracingPipelineCreateInfoKHR::builder()
                    .stages(&shader_stages)
                    .groups(&shader_groups)
                    .max_pipeline_ray_recursion_depth(0)
                    .layout(pipeline_layout)
                    .build()],
                None,
            )
        }
        .unwrap()[0];

        unsafe {
            device.destroy_shader_module(shader_module, None);
        }

        (
            descriptor_set_layout,
            pipeline,
            pipeline_layout,
            shader_groups.len(),
        )
    };

    let descriptor_sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
            descriptor_count: 2,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_IMAGE,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: scene_buffers.images.len() as u32,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 1,
        },
    ];

    let descriptor_pool_info = vk::DescriptorPoolCreateInfo::builder()
        .pool_sizes(&descriptor_sizes)
        .max_sets(1);

    let descriptor_pool =
        unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) }.unwrap();

    let descriptor_counts = [1];

    let mut count_allocate_info = vk::DescriptorSetVariableDescriptorCountAllocateInfo::builder()
        .descriptor_counts(&descriptor_counts)
        .build();

    let descriptor_sets = unsafe {
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&[descriptor_set_layout])
                .push_next(&mut count_allocate_info)
                .build(),
        )
    }
    .unwrap();

    let descriptor_set = descriptor_sets[0];

    let uniform_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.uniform.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let uniform_buffers_write = vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(&uniform_buffer_info)
        .build();

    let accel_structs = [scene_buffers.tlas, scene_buffers.tlas_emit_object];
    let mut accel_info = vk::WriteDescriptorSetAccelerationStructureKHR::builder()
        .acceleration_structures(&accel_structs)
        .build();

    let mut accel_write = vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_set)
        .dst_binding(1)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
        .push_next(&mut accel_info)
        .build();

    // This is only set by the builder for images, buffers, or views; need to set explicitly after
    accel_write.descriptor_count = 2;

    let image_info = [vk::DescriptorImageInfo::builder()
        .image_layout(vk::ImageLayout::GENERAL)
        .image_view(image_view)
        .build()];

    let image_write = vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_set)
        .dst_binding(2)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
        .image_info(&image_info)
        .build();

    let light_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.lights.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let light_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(3)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&light_buffer_info)
            .build()
    };

    let area_light_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.area_lights.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let area_light_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(4)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&area_light_buffer_info)
            .build()
    };

    let emit_object_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.emit_objects.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let emit_object_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(5)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&emit_object_buffer_info)
            .build()
    };

    let material_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.materials.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let material_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(6)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&material_buffer_info)
            .build()
    };

    let texture_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.textures.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let texture_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(7)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&texture_buffer_info)
            .build()
    };

    let images_info: Vec<_> = scene_buffers
        .images
        .iter()
        .map(|i| {
            vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(i.image_view)
                .sampler(i.sampler)
                .build()
        })
        .collect();

    let images_write = vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_set)
        .dst_binding(8)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&images_info)
        .build();

    let index_data_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.index_data.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let index_data_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(9)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&index_data_buffer_info)
            .build()
    };

    let indices_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.indices.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let indices_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(10)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&indices_buffer_info)
            .build()
    };

    let vertices_buffer_info = [vk::DescriptorBufferInfo::builder()
        .buffer(scene_buffers.vertices.buffer)
        .range(vk::WHOLE_SIZE)
        .build()];

    let vertices_write = {
        vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(11)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&vertices_buffer_info)
            .build()
    };

    unsafe {
        device.update_descriptor_sets(
            &[
                uniform_buffers_write,
                accel_write,
                image_write,
                light_write,
                area_light_write,
                emit_object_write,
                material_write,
                texture_write,
                images_write,
                index_data_write,
                indices_write,
                vertices_write,
            ],
            &[],
        );
    }

    let shader_binding_table_buffer = {
        let incoming_table_data = unsafe {
            rt_pipeline.get_ray_tracing_shader_group_handles(
                graphics_pipeline,
                0,
                shader_groups_len as u32,
                shader_groups_len * rt_pipeline_properties.shader_group_handle_size as usize,
            )
        }
        .unwrap();

        let handle_size_aligned = aligned_size(
            rt_pipeline_properties.shader_group_handle_size,
            rt_pipeline_properties.shader_group_base_alignment,
        );

        let table_size = shader_groups_len * handle_size_aligned as usize;
        let mut table_data = vec![0u8; table_size];

        for i in 0..shader_groups_len {
            table_data[i * handle_size_aligned as usize
                ..i * handle_size_aligned as usize
                    + rt_pipeline_properties.shader_group_handle_size as usize]
                .copy_from_slice(
                    &incoming_table_data[i * rt_pipeline_properties.shader_group_handle_size
                        as usize
                        ..i * rt_pipeline_properties.shader_group_handle_size as usize
                            + rt_pipeline_properties.shader_group_handle_size as usize],
                );
        }

        let mut shader_binding_table_buffer = BufferResource::new(
            table_size as u64,
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        shader_binding_table_buffer.store(&table_data, &device);

        shader_binding_table_buffer
    };

    {
        let handle_size_aligned = aligned_size(
            rt_pipeline_properties.shader_group_handle_size,
            rt_pipeline_properties.shader_group_base_alignment,
        ) as u64;

        // |[ raygen shader ]|[ miss shader ]|[ miss shader (PDF) ]|[ hit shader (triangle) ]|[ hit shader (sphere) ]|[ hit shader (triangle) (PDF) ]|[ hit shader (sphere) (PDF) ]|
        // |                 |               |                     |                         |                       |                               |                             |
        // | 0               | 1             | 2                   | 3                       | 3                     | 4                             | 5                           |

        let sbt_address =
            unsafe { get_buffer_device_address(&device, shader_binding_table_buffer.buffer) };

        let sbt_raygen_region = vk::StridedDeviceAddressRegionKHR::builder()
            .device_address(sbt_address + 0)
            .size(handle_size_aligned)
            .stride(handle_size_aligned)
            .build();

        let sbt_miss_region = vk::StridedDeviceAddressRegionKHR::builder()
            .device_address(sbt_address + 1 * handle_size_aligned)
            .size(2 * handle_size_aligned)
            .stride(handle_size_aligned)
            .build();

        let sbt_hit_region = vk::StridedDeviceAddressRegionKHR::builder()
            .device_address(sbt_address + 3 * handle_size_aligned)
            .size(4 * handle_size_aligned)
            .stride(handle_size_aligned)
            .build();

        let sbt_call_region = vk::StridedDeviceAddressRegionKHR::default();

        let command_buffer = {
            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }
                .expect("Failed to allocate Command Buffers!")[0]
        };

        {
            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE)
                .build();

            unsafe { device.begin_command_buffer(command_buffer, &command_buffer_begin_info) }
                .expect("Failed to begin recording Command Buffer at beginning!");
        }
        unsafe {
            let range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(3)
                .build();

            device.cmd_clear_color_image(
                command_buffer,
                image,
                vk::ImageLayout::GENERAL,
                &vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
                &[range],
            );

            let image_barrier = vk::ImageMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(3)
                        .build(),
                )
                .build();

            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier],
            );

            device.end_command_buffer(command_buffer).unwrap();
        }

        let command_buffers = [command_buffer];

        let submit_infos = [vk::SubmitInfo::builder()
            .command_buffers(&command_buffers)
            .build()];

        unsafe {
            device
                .queue_submit(graphics_queue, &submit_infos, vk::Fence::null())
                .expect("Failed to execute queue submit.");

            device.queue_wait_idle(graphics_queue).unwrap();
            device.free_command_buffers(command_pool, &[command_buffer]);
        }

        let image_barrier2 = vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(3)
                    .build(),
            )
            .build();

        let mut rng = StdRng::from_entropy();
        let mut sampled = 0;

        let command_buffer = {
            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }
                .expect("Failed to allocate Command Buffers!")[0]
        };

        while sampled < N_SAMPLES {
            let samples = std::cmp::min(N_SAMPLES - sampled, N_SAMPLES_ITER);
            sampled += samples;

            {
                let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE)
                    .build();

                unsafe { device.begin_command_buffer(command_buffer, &command_buffer_begin_info) }
                    .expect("Failed to begin recording Command Buffer at beginning!");
            }

            unsafe {
                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::RAY_TRACING_KHR,
                    graphics_pipeline,
                );
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::RAY_TRACING_KHR,
                    pipeline_layout,
                    0,
                    &[descriptor_set],
                    &[],
                );
            }
            for _ in 0..samples {
                unsafe {
                    device.cmd_pipeline_barrier(
                        command_buffer,
                        vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
                        vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[image_barrier2],
                    );

                    device.cmd_push_constants(
                        command_buffer,
                        pipeline_layout,
                        vk::ShaderStageFlags::RAYGEN_KHR,
                        0,
                        &rng.next_u32().to_le_bytes(),
                    );

                    rt_pipeline.cmd_trace_rays(
                        command_buffer,
                        &sbt_raygen_region,
                        &sbt_miss_region,
                        &sbt_hit_region,
                        &sbt_call_region,
                        scene.film.xresolution,
                        scene.film.yresolution,
                        1,
                    );
                }
            }
            unsafe {
                device.end_command_buffer(command_buffer).unwrap();

                let command_buffers = [command_buffer];

                let submit_infos = [vk::SubmitInfo::builder()
                    .command_buffers(&command_buffers)
                    .build()];

                device
                    .queue_submit(graphics_queue, &submit_infos, vk::Fence::null())
                    .expect("Failed to execute queue submit.");

                device.queue_wait_idle(graphics_queue).unwrap();
            }
            eprint!("\rSamples: {} / {} ", sampled, N_SAMPLES);
        }
        unsafe {
            device.free_command_buffers(command_pool, &[command_buffer]);
        }
        eprint!("\nDone");
    }

    // transfer to host

    let dst_image = {
        let dst_image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(COLOR_FORMAT)
            .extent(
                vk::Extent3D::builder()
                    .width(scene.film.xresolution)
                    .height(scene.film.yresolution)
                    .depth(1)
                    .build(),
            )
            .mip_levels(1)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::LINEAR)
            .usage(vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .build();

        unsafe { device.create_image(&dst_image_create_info, None) }.unwrap()
    };

    let dst_device_memory = {
        let dst_mem_reqs = unsafe { device.get_image_memory_requirements(dst_image) };
        let dst_mem_alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(dst_mem_reqs.size)
            .memory_type_index(get_memory_type_index(
                device_memory_properties,
                dst_mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            ));

        unsafe { device.allocate_memory(&dst_mem_alloc_info, None) }.unwrap()
    };
    unsafe { device.bind_image_memory(dst_image, dst_device_memory, 0) }.unwrap();

    let copy_cmd = {
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1)
            .build();

        unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap()[0]
    };

    let mut data = (0..3).map(|layer| {
        {
            let cmd_begin_info = vk::CommandBufferBeginInfo::builder().build();

            unsafe { device.begin_command_buffer(copy_cmd, &cmd_begin_info) }.unwrap();
        }

        {
            let image_barrier = vk::ImageMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(dst_image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build();

            unsafe {
                device.cmd_pipeline_barrier(
                    copy_cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[image_barrier],
                );
            }
        }

        {
            let copy_region = vk::ImageCopy::builder()
                .src_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_array_layer(layer)
                        .layer_count(1)
                        .build(),
                )
                .dst_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1)
                        .build(),
                )
                .extent(
                    vk::Extent3D::builder()
                        .width(scene.film.xresolution)
                        .height(scene.film.yresolution)
                        .depth(1)
                        .build(),
                )
                .build();

            unsafe {
                device.cmd_copy_image(
                    copy_cmd,
                    image,
                    vk::ImageLayout::GENERAL,
                    dst_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy_region],
                );
            }
        }

        {
            let image_barrier = vk::ImageMemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(dst_image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .build();

            unsafe {
                device.cmd_pipeline_barrier(
                    copy_cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[image_barrier],
                );
            }
        }

        {
            let submit_infos = [vk::SubmitInfo {
                s_type: vk::StructureType::SUBMIT_INFO,
                p_next: ptr::null(),
                wait_semaphore_count: 0,
                p_wait_semaphores: null(),
                p_wait_dst_stage_mask: null(),
                command_buffer_count: 1,
                p_command_buffers: &copy_cmd,
                signal_semaphore_count: 0,
                p_signal_semaphores: null(),
            }];

            unsafe {
                device.end_command_buffer(copy_cmd).unwrap();

                device
                    .queue_submit(graphics_queue, &submit_infos, vk::Fence::null())
                    .expect("Failed to execute queue submit.");

                device.queue_wait_idle(graphics_queue).unwrap();
            }
        }

        let subresource_layout = {
            let subresource = vk::ImageSubresource::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .build();

            unsafe { device.get_image_subresource_layout(dst_image, subresource) }
        };

        let data: *const u8 = unsafe {
            device
                .map_memory(
                    dst_device_memory,
                    0,
                    vk::WHOLE_SIZE,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap() as _
        };

        let data = unsafe { data.offset(subresource_layout.offset as isize) };

        let data_linear = to_linear(
            data,
            &subresource_layout,
            scene.film.xresolution as usize,
            scene.film.yresolution as usize,
        );
        unsafe { device.unmap_memory(dst_device_memory) };
        data_linear
    });

    let data_image_linear = data.next().unwrap();
    let data_normal_linear = data.next().unwrap();
    let data_albedo_linear = data.next().unwrap();

    let mut data_image_linear = f32_4_to_3(&data_image_linear);
    let mut data_normal_linear = f32_4_to_3(&data_normal_linear);
    let mut data_albedo_linear = f32_4_to_3(&data_albedo_linear);

    average(&mut data_image_linear, N_SAMPLES);
    average(&mut data_normal_linear, N_SAMPLES);
    average(&mut data_albedo_linear, N_SAMPLES);

    #[cfg(feature = "optix-denoiser")]
    if opts.denoiser == Denoiser::Optix {
        data_image_linear = optix_denoise(
            &data_image_linear,
            &data_normal_linear,
            &data_albedo_linear,
            scene.film.xresolution,
            scene.film.yresolution,
        )
        .unwrap();
    }

    #[cfg(feature = "oidn-denoiser")]
    if opts.denoiser == Denoiser::Oidn {
        data_image_linear = oidn_denoise(
            &data_image_linear,
            &data_normal_linear,
            &data_albedo_linear,
            scene.film.xresolution,
            scene.film.yresolution,
        )
        .unwrap();
    }

    let rgb = to_rgb8(&data_image_linear);

    image::save_buffer(
        scene.film.filename,
        &rgb,
        scene.film.xresolution,
        scene.film.yresolution,
        image::ColorType::Rgb8,
    )
    .unwrap();

    if let Some(aov_normal_path) = opts.aov_normal {
        image::save_buffer(
            aov_normal_path,
            &to_aov_normal(&data_normal_linear),
            scene.film.xresolution,
            scene.film.yresolution,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }

    if let Some(aov_albedo_path) = opts.aov_albedo {
        image::save_buffer(
            aov_albedo_path,
            &to_aov(&data_albedo_linear),
            scene.film.xresolution,
            scene.film.yresolution,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }

    unsafe {
        device.free_memory(dst_device_memory, None);
        device.destroy_image(dst_image, None);
    }

    // clean up

    unsafe {
        device.destroy_command_pool(command_pool, None);
    }

    unsafe {
        device.destroy_descriptor_pool(descriptor_pool, None);
        shader_binding_table_buffer.destroy(&device);
        device.destroy_pipeline(graphics_pipeline, None);
        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
    }

    unsafe {
        device.destroy_pipeline_layout(pipeline_layout, None);
    }

    unsafe {
        scene_buffers.destroy(&device, &acceleration_structure);

        device.destroy_image_view(image_view, None);
        device.destroy_image(image, None);
        device.free_memory(device_memory, None);
    }

    unsafe {
        device.destroy_device(None);
    }

    unsafe {
        instance.destroy_instance(None);
    }
}

fn to_linear(
    mut data: *const u8,
    layout: &vk::SubresourceLayout,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut result = vec![0; 4 * 4 * width * height];

    for h in 0..height {
        let row = unsafe { std::slice::from_raw_parts(data, 4 * 4 * width) };
        result[4 * 4 * width * h..4 * 4 * width * (h + 1)].copy_from_slice(row);

        data = unsafe { data.offset(layout.row_pitch as isize) };
    }

    result
}

fn f32_4_to_3(data: &[u8]) -> Vec<u8> {
    let data_f32: &[f32] = bytemuck::cast_slice(data);

    data_f32
        .chunks(4)
        .flat_map(|v| bytemuck::cast_slice(v).iter().take(3 * 4).copied())
        .collect()
}

fn average(data_linear: &mut [u8], denom: u32) {
    let data_f32: &mut [f32] = bytemuck::cast_slice_mut(data_linear);

    for v in data_f32 {
        *v /= denom as f32;
    }
}

// from pbrt-v3
// gamma 2.2
fn gamma_correct(value: f32) -> f32 {
    if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn to_rgb8(data_linear: &[u8]) -> Vec<u8> {
    let data_f32: &[f32] = bytemuck::cast_slice(data_linear);

    data_f32
        .iter()
        .map(|&value| (255.0 * gamma_correct(value)).round().clamp(0.0, 255.0) as u8)
        .collect()
}

fn to_aov(data_linear: &[u8]) -> Vec<u8> {
    let data_f32: &[f32] = bytemuck::cast_slice(data_linear);

    data_f32
        .iter()
        .map(|&value| (256.0 * value.clamp(0.0, 0.999)) as u8)
        .collect()
}

fn to_aov_normal(data_linear: &[u8]) -> Vec<u8> {
    let data_f32: &[f32] = bytemuck::cast_slice(data_linear);

    data_f32
        .iter()
        .map(|&value| (256.0 * (value * 0.5 + 0.5).clamp(0.0, 0.999)) as u8)
        .collect()
}

#[cfg(feature = "oidn-denoiser")]
fn oidn_denoise(
    linear_image: &[u8],
    linear_normal: &[u8],
    linear_albedo: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, oidn::FilterError> {
    let mut output = vec![0u8; linear_image.len()];

    let device = oidn::Device::new();
    let mut filter = oidn::RayTracing::new(&device);
    filter
        .image_dimensions(width as usize, height as usize)
        .albedo_normal(
            bytemuck::cast_slice(linear_albedo),
            bytemuck::cast_slice(linear_normal),
        );

    filter.filter(
        bytemuck::cast_slice(linear_image),
        &mut bytemuck::cast_slice_mut(&mut output),
    )?;

    Ok(output)
}

#[cfg(feature = "optix-denoiser")]
fn optix_denoise(
    linear_image: &[u8],
    linear_normal: &[u8],
    linear_albedo: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use cust::memory::DeviceBuffer;
    use cust::prelude::{Stream, StreamFlags};
    use cust::util::SliceExt;
    use optix::context::OptixContext;
    use optix::denoiser::DenoiserOptions;
    use optix::denoiser::{Denoiser, DenoiserModelKind, DenoiserParams, Image, ImageFormat};
    // set up CUDA and OptiX then make the needed structs/contexts.
    let cuda_ctx = cust::quick_init()?;
    optix::init()?;
    let optix_ctx = OptixContext::new(&cuda_ctx)?;

    let stream = Stream::new(StreamFlags::NON_BLOCKING, None)?;

    let mut denoiser_option = DenoiserOptions::default();
    denoiser_option.guide_normal = true;
    denoiser_option.guide_albedo = true;
    // set up the denoiser, choosing Ldr as our model because our colors are in
    // the 0.0 - 1.0 range.
    let mut denoiser = Denoiser::new(&optix_ctx, DenoiserModelKind::Ldr, denoiser_option)?;

    // setup the optix state for our required image dimensions. this allocates the required
    // state and scratch memory for further invocations.
    denoiser.setup_state(&stream, width, height, false)?;

    // allocate the buffer for the noisy image and copy the data to the GPU.
    let in_buf_image = linear_image.as_dbuf()?;
    let in_buf_normal = linear_normal.as_dbuf()?;
    let in_buf_albedo = linear_albedo.as_dbuf()?;

    // Currently zeroed is unsafe, but in the future we will probably expose a safe way to do it
    // using bytemuck
    let mut out_buf = unsafe { DeviceBuffer::<[f32; 3]>::zeroed((width * height) as usize)? };

    // make an image to tell OptiX about how our image buffer is represented
    let input_image = Image::new(&in_buf_image, ImageFormat::Float3, width, height);
    let input_normal = Image::new(&in_buf_normal, ImageFormat::Float3, width, height);
    let input_albedo = Image::new(&in_buf_albedo, ImageFormat::Float3, width, height);

    // Invoke the denoiser on the image. OptiX will queue up the work on the
    // CUDA stream.
    denoiser.invoke(
        &stream,
        optix::denoiser::DenoiserGuideImages {
            albedo: Some(input_albedo),
            normal: Some(input_normal),
            flow: None,
        },
        input_image,
        DenoiserParams::default(),
        &mut out_buf,
    )?;

    // Finally, synchronize the stream to wait until the denoiser is finished doing its work.
    stream.synchronize()?;

    // copy back the data from the gpu.
    let denoised = out_buf.as_host_vec()?;

    Ok(denoised
        .iter()
        .flat_map(|v| {
            bytemuck::cast_slice::<f32, u8>(v.as_slice())
                .iter()
                .copied()
        })
        .collect())
}

fn check_validation_layer_support<'a>(
    entry: &ash::Entry,
    required_validation_layers: impl IntoIterator<Item = &'a CStr>,
) -> VkResult<bool> {
    let supported_layers: HashSet<CString> = entry
        .enumerate_instance_layer_properties()?
        .into_iter()
        .map(|layer_property| unsafe {
            CStr::from_ptr(layer_property.layer_name.as_ptr()).to_owned()
        })
        .collect();

    Ok(required_validation_layers
        .into_iter()
        .all(|l| supported_layers.contains(l)))
}

fn pick_physical_device_and_queue_family_indices(
    instance: &ash::Instance,
    extensions: &[&CStr],
) -> VkResult<Option<(vk::PhysicalDevice, u32)>> {
    Ok(unsafe { instance.enumerate_physical_devices() }?
        .into_iter()
        .find_map(|physical_device| {
            if unsafe { instance.enumerate_device_extension_properties(physical_device) }.map(
                |exts| {
                    let set: HashSet<&CStr> = exts
                        .iter()
                        .map(|ext| unsafe { CStr::from_ptr(&ext.extension_name as *const c_char) })
                        .collect();

                    extensions.iter().all(|ext| set.contains(ext))
                },
            ) != Ok(true)
            {
                return None;
            }

            let graphics_family =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                    .into_iter()
                    .enumerate()
                    .find(|(_, device_properties)| {
                        device_properties.queue_count > 0
                            && device_properties
                                .queue_flags
                                .contains(vk::QueueFlags::GRAPHICS)
                    });

            graphics_family.map(|(i, _)| (physical_device, i as u32))
        }))
}

unsafe fn create_shader_module(device: &ash::Device, code: &[u8]) -> VkResult<vk::ShaderModule> {
    let shader_module_create_info = vk::ShaderModuleCreateInfo {
        s_type: vk::StructureType::SHADER_MODULE_CREATE_INFO,
        p_next: ptr::null(),
        flags: vk::ShaderModuleCreateFlags::empty(),
        code_size: code.len(),
        p_code: code.as_ptr() as *const u32,
    };

    device.create_shader_module(&shader_module_create_info, None)
}

fn get_memory_type_index(
    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    mut type_bits: u32,
    properties: vk::MemoryPropertyFlags,
) -> u32 {
    for i in 0..device_memory_properties.memory_type_count {
        if (type_bits & 1) == 1 {
            if (device_memory_properties.memory_types[i as usize].property_flags & properties)
                == properties
            {
                return i;
            }
        }
        type_bits >>= 1;
    }
    0
}

pub unsafe extern "system" fn default_vulkan_debug_utils_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    let severity = match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => "[Verbose]",
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => "[Warning]",
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => "[Error]",
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => "[Info]",
        _ => "[Unknown]",
    };
    let types = match message_type {
        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL => "[General]",
        vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE => "[Performance]",
        vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION => "[Validation]",
        _ => "[Unknown]",
    };
    let message = CStr::from_ptr((*p_callback_data).p_message);
    println!("[Debug]{}{}{:?}", severity, types, message);

    vk::FALSE
}

#[derive(Clone)]
struct BufferResource {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
}

impl BufferResource {
    fn new(
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        memory_properties: vk::MemoryPropertyFlags,
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    ) -> Self {
        unsafe {
            let buffer_info = vk::BufferCreateInfo::builder()
                .size(size)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .build();

            let buffer = device.create_buffer(&buffer_info, None).unwrap();

            let memory_req = device.get_buffer_memory_requirements(buffer);

            let memory_index = get_memory_type_index(
                device_memory_properties,
                memory_req.memory_type_bits,
                memory_properties,
            );

            let mut memory_allocate_flags_info = vk::MemoryAllocateFlagsInfo::builder()
                .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
                .build();

            let mut allocate_info_builder = vk::MemoryAllocateInfo::builder();

            if usage.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS) {
                allocate_info_builder =
                    allocate_info_builder.push_next(&mut memory_allocate_flags_info);
            }

            let allocate_info = allocate_info_builder
                .allocation_size(memory_req.size)
                .memory_type_index(memory_index)
                .build();

            let memory = device.allocate_memory(&allocate_info, None).unwrap();

            device.bind_buffer_memory(buffer, memory, 0).unwrap();

            BufferResource {
                buffer,
                memory,
                size,
            }
        }
    }

    fn store<T: Copy>(&mut self, data: &[T], device: &ash::Device) {
        unsafe {
            let size = (std::mem::size_of::<T>() * data.len()) as u64;
            assert!(self.size >= size);
            let mapped_ptr = self.map(size, device);
            let mut mapped_slice = Align::new(mapped_ptr, std::mem::align_of::<T>() as u64, size);
            mapped_slice.copy_from_slice(&data);
            self.unmap(device);
        }
    }

    fn map(&mut self, size: vk::DeviceSize, device: &ash::Device) -> *mut std::ffi::c_void {
        unsafe {
            let data: *mut std::ffi::c_void = device
                .map_memory(self.memory, 0, size, vk::MemoryMapFlags::empty())
                .unwrap();
            data
        }
    }

    fn unmap(&mut self, device: &ash::Device) {
        unsafe {
            device.unmap_memory(self.memory);
        }
    }

    unsafe fn destroy(self, device: &ash::Device) {
        device.destroy_buffer(self.buffer, None);
        device.free_memory(self.memory, None);
    }
}

fn aligned_size(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}

unsafe fn get_buffer_device_address(device: &ash::Device, buffer: vk::Buffer) -> u64 {
    let buffer_device_address_info = vk::BufferDeviceAddressInfo::builder()
        .buffer(buffer)
        .build();

    device.get_buffer_device_address(&buffer_device_address_info)
}
struct Image {
    buffer: BufferResource,
    image: vk::Image,
    image_view: vk::ImageView,
    sampler: vk::Sampler,
}

impl Image {
    fn load(
        img: &DynamicImage,
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) -> Self {
        const COLOR_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;

        let image = {
            let image_create_info = vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(COLOR_FORMAT)
                .extent(
                    vk::Extent3D::builder()
                        .width(img.width())
                        .height(img.height())
                        .depth(1)
                        .build(),
                )
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .build();

            unsafe { device.create_image(&image_create_info, None) }.unwrap()
        };

        let mem_reqs = unsafe { device.get_image_memory_requirements(image) };

        let buffer = BufferResource::new(
            mem_reqs.size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            device,
            device_memory_properties,
        );

        unsafe { device.bind_image_memory(image, buffer.memory, 0) }.unwrap();

        let image_view = {
            let image_view_create_info = vk::ImageViewCreateInfo::builder()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(COLOR_FORMAT)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image)
                .build();

            unsafe { device.create_image_view(&image_view_create_info, None) }.unwrap()
        };

        let rgb = img.as_rgb8().unwrap();
        let mut data: Vec<u8> = Vec::new();

        for p in rgb.pixels() {
            let rgba = [
                p.0[0] as f32 / 255.0,
                p.0[1] as f32 / 255.0,
                p.0[2] as f32 / 255.0,
                1.0,
            ];

            data.extend(bytemuck::cast_slice(rgba.as_slice()));
        }

        let mut staging_buffer = BufferResource::new(
            data.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::DEVICE_LOCAL,
            device,
            device_memory_properties,
        );

        staging_buffer.store(&data, device);

        let command_buffer = {
            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }
                .expect("Failed to allocate Command Buffers!")[0]
        };

        {
            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE)
                .build();

            unsafe { device.begin_command_buffer(command_buffer, &command_buffer_begin_info) }
                .expect("Failed to begin recording Command Buffer at beginning!");
        }

        let dst_image_barrier = vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .build();

        let image_barrier = vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .build();

        let copy_region = vk::BufferImageCopy::builder()
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .image_extent(
                vk::Extent3D::builder()
                    .width(img.width())
                    .height(img.height())
                    .depth(1)
                    .build(),
            )
            .build();

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[dst_image_barrier],
            );

            device.cmd_copy_buffer_to_image(
                command_buffer,
                staging_buffer.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy_region],
            );

            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier],
            );

            device.end_command_buffer(command_buffer).unwrap();

            let command_buffers = [command_buffer];

            let submit_infos = [vk::SubmitInfo::builder()
                .command_buffers(&command_buffers)
                .build()];

            device
                .queue_submit(graphics_queue, &submit_infos, vk::Fence::null())
                .expect("Failed to execute queue submit.");

            device.queue_wait_idle(graphics_queue).unwrap();
            device.free_command_buffers(command_pool, &[command_buffer]);
        }

        let sampler = {
            let sampler_create_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .build();

            unsafe { device.create_sampler(&sampler_create_info, None) }.unwrap()
        };

        unsafe { staging_buffer.destroy(device) };

        Self {
            buffer,
            image,
            image_view,
            sampler,
        }
    }

    unsafe fn destroy(self, device: &ash::Device) {
        self.buffer.destroy(device);
        device.destroy_image_view(self.image_view, None);
        device.destroy_image(self.image, None);
        device.destroy_sampler(self.sampler, None);
    }
}

struct SceneBuffers {
    tlas: AccelerationStructureKHR,
    tlas_emit_object: AccelerationStructureKHR,
    default_blas: AccelerationStructureKHR,
    blases: Vec<AccelerationStructureKHR>,
    uniform: BufferResource,
    materials: BufferResource,
    buffers: Vec<BufferResource>,
    index_data: BufferResource,
    vertices: BufferResource,
    indices: BufferResource,
    textures: BufferResource,
    lights: BufferResource,
    area_lights: BufferResource,
    emit_objects: BufferResource,
    images: Vec<Image>,
}

impl SceneBuffers {
    fn default_blas(
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
        acceleration_structure: &AccelerationStructure,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) -> (AccelerationStructureKHR, BufferResource, BufferResource) {
        let aabb = vk::AabbPositionsKHR::builder()
            .min_x(-1.0)
            .max_x(1.0)
            .min_y(-1.0)
            .max_y(1.0)
            .min_z(-1.0)
            .max_z(1.0)
            .build();

        let mut aabb_buffer = BufferResource::new(
            std::mem::size_of::<vk::AabbPositionsKHR>() as u64,
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT
                | vk::MemoryPropertyFlags::DEVICE_LOCAL,
            device,
            device_memory_properties,
        );

        aabb_buffer.store(&[aabb], &device);

        let geometry = vk::AccelerationStructureGeometryKHR::builder()
            .geometry_type(vk::GeometryTypeKHR::AABBS)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                aabbs: vk::AccelerationStructureGeometryAabbsDataKHR::builder()
                    .data(vk::DeviceOrHostAddressConstKHR {
                        device_address: unsafe {
                            get_buffer_device_address(&device, aabb_buffer.buffer)
                        },
                    })
                    .stride(std::mem::size_of::<vk::AabbPositionsKHR>() as u64)
                    .build(),
            })
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .build();

        let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::builder()
            .first_vertex(0)
            .primitive_count(1)
            .primitive_offset(0)
            .transform_offset(0)
            .build();

        let geometries = [geometry];

        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::builder()
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .geometries(&geometries)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .build();

        let size_info = unsafe {
            acceleration_structure.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[1],
            )
        };

        let bottom_as_buffer = BufferResource::new(
            size_info.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        let as_create_info = vk::AccelerationStructureCreateInfoKHR::builder()
            .ty(build_info.ty)
            .size(size_info.acceleration_structure_size)
            .buffer(bottom_as_buffer.buffer)
            .offset(0)
            .build();

        let bottom_as =
            unsafe { acceleration_structure.create_acceleration_structure(&as_create_info, None) }
                .unwrap();

        build_info.dst_acceleration_structure = bottom_as;

        let scratch_buffer = BufferResource::new(
            size_info.build_scratch_size,
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS | vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        build_info.scratch_data = vk::DeviceOrHostAddressKHR {
            device_address: unsafe { get_buffer_device_address(&device, scratch_buffer.buffer) },
        };

        let build_command_buffer = {
            let allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            let command_buffers =
                unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap();
            command_buffers[0]
        };

        unsafe {
            device
                .begin_command_buffer(
                    build_command_buffer,
                    &vk::CommandBufferBeginInfo::builder()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                        .build(),
                )
                .unwrap();

            let build_infos = [build_info];
            let build_range_infos: &[&[_]] = &[&[build_range_info]];

            acceleration_structure.cmd_build_acceleration_structures(
                build_command_buffer,
                &build_infos,
                build_range_infos,
            );
            device.end_command_buffer(build_command_buffer).unwrap();
            device
                .queue_submit(
                    graphics_queue,
                    &[vk::SubmitInfo::builder()
                        .command_buffers(&[build_command_buffer])
                        .build()],
                    vk::Fence::null(),
                )
                .expect("queue submit failed.");

            device.queue_wait_idle(graphics_queue).unwrap();
            device.free_command_buffers(command_pool, &[build_command_buffer]);
            scratch_buffer.destroy(&device);
        }
        (bottom_as, bottom_as_buffer, aabb_buffer)
    }

    fn triangle_blas(
        index_offset: u32,
        primitive_count: u32,
        vertices: &BufferResource,
        vertex_len: u32,
        indices: &BufferResource,
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
        acceleration_structure: &AccelerationStructure,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) -> (AccelerationStructureKHR, BufferResource) {
        let vertex_stride = std::mem::size_of::<Vertex>();
        let index_stride = std::mem::size_of::<u32>();

        let geometry = vk::AccelerationStructureGeometryKHR::builder()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::builder()
                    .vertex_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: unsafe {
                            get_buffer_device_address(&device, vertices.buffer)
                        },
                    })
                    .max_vertex(vertex_len as u32 - 1)
                    .vertex_stride(vertex_stride as u64)
                    .vertex_format(vk::Format::R32G32B32_SFLOAT)
                    .index_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: unsafe {
                            get_buffer_device_address(&device, indices.buffer)
                        } + (index_stride * index_offset as usize) as u64,
                    })
                    .index_type(vk::IndexType::UINT32)
                    .build(),
            })
            .build();

        let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::builder()
            .first_vertex(0)
            .primitive_count(primitive_count)
            .primitive_offset(0)
            .transform_offset(0)
            .build();

        let geometries = [geometry];

        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::builder()
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .geometries(&geometries)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .build();

        let size_info = unsafe {
            acceleration_structure.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[primitive_count],
            )
        };

        let bottom_as_buffer = BufferResource::new(
            size_info.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        let as_create_info = vk::AccelerationStructureCreateInfoKHR::builder()
            .ty(build_info.ty)
            .size(size_info.acceleration_structure_size)
            .buffer(bottom_as_buffer.buffer)
            .offset(0)
            .build();

        let bottom_as =
            unsafe { acceleration_structure.create_acceleration_structure(&as_create_info, None) }
                .unwrap();

        build_info.dst_acceleration_structure = bottom_as;

        let scratch_buffer = BufferResource::new(
            size_info.build_scratch_size,
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS | vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        build_info.scratch_data = vk::DeviceOrHostAddressKHR {
            device_address: unsafe { get_buffer_device_address(&device, scratch_buffer.buffer) },
        };

        let build_command_buffer = {
            let allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            let command_buffers =
                unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap();
            command_buffers[0]
        };

        unsafe {
            device
                .begin_command_buffer(
                    build_command_buffer,
                    &vk::CommandBufferBeginInfo::builder()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                        .build(),
                )
                .unwrap();

            let build_infos = [build_info];
            let build_range_infos: &[&[_]] = &[&[build_range_info]];

            acceleration_structure.cmd_build_acceleration_structures(
                build_command_buffer,
                &build_infos,
                build_range_infos,
            );
            device.end_command_buffer(build_command_buffer).unwrap();
            device
                .queue_submit(
                    graphics_queue,
                    &[vk::SubmitInfo::builder()
                        .command_buffers(&[build_command_buffer])
                        .build()],
                    vk::Fence::null(),
                )
                .expect("queue submit failed.");

            device.queue_wait_idle(graphics_queue).unwrap();
            device.free_command_buffers(command_pool, &[build_command_buffer]);
            scratch_buffer.destroy(&device);
        }
        (bottom_as, bottom_as_buffer)
    }

    fn build_tlas(
        tlas_instances: &[vk::AccelerationStructureInstanceKHR],
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
        acceleration_structure: &AccelerationStructure,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) -> (vk::AccelerationStructureKHR, BufferResource, BufferResource) {
        let (instance_count, instance_buffer) = {
            let instances = tlas_instances;

            let instance_buffer_size =
                std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() * instances.len();

            let mut instance_buffer = BufferResource::new(
                instance_buffer_size as vk::DeviceSize,
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );

            instance_buffer.store(&instances, &device);

            (instances.len(), instance_buffer)
        };

        let build_range_info = vk::AccelerationStructureBuildRangeInfoKHR::builder()
            .first_vertex(0)
            .primitive_count(instance_count as u32)
            .primitive_offset(0)
            .transform_offset(0)
            .build();

        let build_command_buffer = {
            let allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .build();

            let command_buffers =
                unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap();
            command_buffers[0]
        };

        unsafe {
            device
                .begin_command_buffer(
                    build_command_buffer,
                    &vk::CommandBufferBeginInfo::builder()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                        .build(),
                )
                .unwrap();
            let memory_barrier = vk::MemoryBarrier::builder()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                .build();
            device.cmd_pipeline_barrier(
                build_command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[memory_barrier],
                &[],
                &[],
            );
        }

        let instances = vk::AccelerationStructureGeometryInstancesDataKHR::builder()
            .array_of_pointers(false)
            .data(vk::DeviceOrHostAddressConstKHR {
                device_address: unsafe {
                    get_buffer_device_address(&device, instance_buffer.buffer)
                },
            })
            .build();

        let geometry = vk::AccelerationStructureGeometryKHR::builder()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .geometry(vk::AccelerationStructureGeometryDataKHR { instances })
            .build();

        let geometries = [geometry];

        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::builder()
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .geometries(&geometries)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .build();

        let size_info = unsafe {
            acceleration_structure.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[build_range_info.primitive_count],
            )
        };

        let top_as_buffer = BufferResource::new(
            size_info.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        let as_create_info = vk::AccelerationStructureCreateInfoKHR::builder()
            .ty(build_info.ty)
            .size(size_info.acceleration_structure_size)
            .buffer(top_as_buffer.buffer)
            .offset(0)
            .build();

        let top_as =
            unsafe { acceleration_structure.create_acceleration_structure(&as_create_info, None) }
                .unwrap();

        build_info.dst_acceleration_structure = top_as;

        let scratch_buffer = BufferResource::new(
            size_info.build_scratch_size,
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS | vk::BufferUsageFlags::STORAGE_BUFFER,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device,
            device_memory_properties,
        );

        build_info.scratch_data = vk::DeviceOrHostAddressKHR {
            device_address: unsafe { get_buffer_device_address(&device, scratch_buffer.buffer) },
        };

        unsafe {
            let build_infos = [build_info];
            let build_range_infos: &[&[_]] = &[&[build_range_info]];
            acceleration_structure.cmd_build_acceleration_structures(
                build_command_buffer,
                &build_infos,
                build_range_infos,
            );
            device.end_command_buffer(build_command_buffer).unwrap();
            device
                .queue_submit(
                    graphics_queue,
                    &[vk::SubmitInfo::builder()
                        .command_buffers(&[build_command_buffer])
                        .build()],
                    vk::Fence::null(),
                )
                .expect("queue submit failed.");

            device.queue_wait_idle(graphics_queue).unwrap();
            device.free_command_buffers(command_pool, &[build_command_buffer]);
            scratch_buffer.destroy(&device);
        }

        (top_as, top_as_buffer, instance_buffer)
    }

    fn new(
        scene: &Scene,
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
        acceleration_structure: &AccelerationStructure,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) -> Self {
        let (default_blas, default_blas_buffer, default_aabb_buffer) = Self::default_blas(
            device,
            device_memory_properties,
            acceleration_structure,
            command_pool,
            graphics_queue,
        );

        let default_accel_handle = {
            let as_addr_info = vk::AccelerationStructureDeviceAddressInfoKHR::builder()
                .acceleration_structure(default_blas)
                .build();
            unsafe {
                acceleration_structure.get_acceleration_structure_device_address(&as_addr_info)
            }
        };
        struct BlasArg {
            index_offset: u32,
            primitive_count: u32,
        }

        let mut buffers = Vec::new();
        let mut global_vertices: Vec<Vertex> = Vec::new();
        let mut global_indices: Vec<u32> = Vec::new();

        let blas_args: Vec<BlasArg> = scene
            .blases
            .iter()
            .map(|triangle_mesh| {
                let index_offset_offset = global_vertices.len() as u32;
                let index_offset = global_indices.len() as u32;

                global_vertices.extend(triangle_mesh.vertices.iter().copied());
                global_indices.extend(
                    triangle_mesh
                        .indices
                        .iter()
                        .map(|&i| i + index_offset_offset),
                );

                BlasArg {
                    index_offset,
                    primitive_count: (triangle_mesh.indices.len() / 3) as u32,
                }
            })
            .collect();

        if global_indices.is_empty() {
            global_indices.push(0);
        }

        if global_vertices.is_empty() {
            global_vertices.push(Vertex {
                position: Vec3A::ZERO,
                normal: Vec3A::ZERO,
                uv: Vec2::ZERO,
            });
        }

        let indices = {
            let buffer_size = (global_indices.len() * std::mem::size_of::<u32>()) as vk::DeviceSize;

            let mut index_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            index_buffer.store(&global_indices, &device);

            index_buffer
        };

        let vertices = {
            let buffer_size =
                (global_vertices.len() * std::mem::size_of::<Vertex>()) as vk::DeviceSize;

            let mut vertex_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            vertex_buffer.store(&global_vertices, &device);

            vertex_buffer
        };

        let blases: Vec<_> = blas_args
            .iter()
            .map(|arg| {
                let (blas, bottom_as_buffer) = Self::triangle_blas(
                    arg.index_offset,
                    arg.primitive_count,
                    &vertices,
                    global_vertices.len() as u32,
                    &indices,
                    device,
                    device_memory_properties,
                    acceleration_structure,
                    command_pool,
                    graphics_queue,
                );
                buffers.push(bottom_as_buffer);
                blas
            })
            .collect();

        buffers.push(default_blas_buffer);
        buffers.push(default_aabb_buffer);

        let material_buffer = {
            let buffer_size =
                (scene.materials.len() * std::mem::size_of::<EnumMaterial>()) as vk::DeviceSize;

            let mut material_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            material_buffer.store(&scene.materials, &device);

            material_buffer
        };

        let mut index_data: Vec<IndexData> = Vec::new();
        let tlas_instances: Vec<vk::AccelerationStructureInstanceKHR> = scene
            .tlas
            .iter()
            .enumerate()
            .map(|(index, instance)| {
                let m = instance.matrix;
                index_data.push(IndexData {
                    material_index: instance.material_index as u32,
                    area_light_index: instance.area_light_index as u32,
                    index_offset: instance
                        .blas_index
                        .map(|i| blas_args[i].index_offset)
                        .unwrap_or(0),
                });
                vk::AccelerationStructureInstanceKHR {
                    transform: vk::TransformMatrixKHR {
                        matrix: [
                            m.x_axis.x, m.y_axis.x, m.z_axis.x, m.w_axis.x, m.x_axis.y, m.y_axis.y,
                            m.z_axis.y, m.w_axis.y, m.x_axis.z, m.y_axis.z, m.z_axis.z, m.w_axis.z,
                        ],
                    },
                    instance_custom_index_and_mask: vk::Packed24_8::new(index as u32, 0xff),
                    instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                        instance.shader_offset as u32,
                        vk::GeometryInstanceFlagsKHR::FORCE_OPAQUE.as_raw() as u8,
                    ),
                    acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                        device_handle: instance
                            .blas_index
                            .map(|i| {
                                let as_addr_info =
                                    vk::AccelerationStructureDeviceAddressInfoKHR::builder()
                                        .acceleration_structure(blases[i])
                                        .build();
                                unsafe {
                                    acceleration_structure
                                        .get_acceleration_structure_device_address(&as_addr_info)
                                }
                            })
                            .unwrap_or(default_accel_handle),
                    },
                }
            })
            .collect();

        let mut tlas_instances_emit: Vec<vk::AccelerationStructureInstanceKHR> = tlas_instances
            .iter()
            .filter(|instance| {
                let index = instance.instance_custom_index_and_mask.low_24() as usize;
                !scene.area_lights[index_data[index].area_light_index as usize].is_null()
            })
            .cloned()
            .collect();

        if tlas_instances_emit.is_empty() {
            tlas_instances_emit.push(tlas_instances[0]);
        }

        let (top_as, top_as_buffer, instance_buffer) = Self::build_tlas(
            &tlas_instances,
            device,
            device_memory_properties,
            acceleration_structure,
            command_pool,
            graphics_queue,
        );

        buffers.push(top_as_buffer);
        buffers.push(instance_buffer);

        let (top_as_emit, top_as_buffer, instance_buffer) = Self::build_tlas(
            &tlas_instances_emit,
            device,
            device_memory_properties,
            acceleration_structure,
            command_pool,
            graphics_queue,
        );

        let mut emit_objects: Vec<SurfaceSample> = scene
            .tlas
            .iter()
            .filter(|t| !scene.area_lights[t.area_light_index].is_null())
            .map(|t| match t.shader_offset {
                ShaderOffset::Sphere => SurfaceSample::new_sphere(t.matrix),
                ShaderOffset::Triangle => {
                    let blas = &blas_args[t.blas_index.unwrap() as usize];
                    SurfaceSample::new_triangle(blas.index_offset, blas.primitive_count, t.matrix)
                }
            })
            .collect();

        buffers.push(top_as_buffer);
        buffers.push(instance_buffer);

        let index_data = {
            let buffer_size =
                (index_data.len() * std::mem::size_of::<IndexData>()) as vk::DeviceSize;

            let mut index_data_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            index_data_buffer.store(&index_data, &device);

            index_data_buffer
        };

        let textures = {
            let buffer_size =
                (scene.textures.len() * std::mem::size_of::<EnumTexture>()) as vk::DeviceSize;

            let mut textures_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            textures_buffer.store(&scene.textures, &device);

            textures_buffer
        };

        let mut lights = scene.lights.clone();
        if lights.is_empty() {
            lights.push(EnumLight::new_distant(
                Vec3A::ZERO,
                Vec3A::ZERO,
                Vec3A::ZERO,
            ));
        }

        let lights = {
            let buffer_size = (lights.len() * std::mem::size_of::<EnumLight>()) as vk::DeviceSize;

            let mut lights_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            lights_buffer.store(&lights, &device);

            lights_buffer
        };

        let area_lights = {
            let buffer_size =
                (scene.area_lights.len() * std::mem::size_of::<EnumAreaLight>()) as vk::DeviceSize;

            let mut area_lights_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            area_lights_buffer.store(&scene.area_lights, &device);

            area_lights_buffer
        };

        let mut images: Vec<Image> = scene
            .images
            .iter()
            .map(|img| {
                Image::load(
                    img,
                    device,
                    device_memory_properties,
                    command_pool,
                    graphics_queue,
                )
            })
            .collect();

        if images.is_empty() {
            let dummy_image = DynamicImage::new_rgb8(1, 1);
            images.push(Image::load(
                &dummy_image,
                device,
                device_memory_properties,
                command_pool,
                graphics_queue,
            ))
        }

        let uniform_buffer = {
            let mut uniform = scene.uniform;
            uniform.emit_object_len = emit_objects.len() as u32;
            uniform.emit_primitives = emit_objects.iter().map(|s| s.primitive_count()).sum();

            let buffer_size = std::mem::size_of::<Uniform>() as vk::DeviceSize;

            let mut uniform_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            uniform_buffer.store(&[uniform], &device);

            uniform_buffer
        };

        if emit_objects.is_empty() {
            emit_objects.push(SurfaceSample::new_sphere(Default::default()));
        }

        let emit_objects = {
            let buffer_size =
                (emit_objects.len() * std::mem::size_of::<SurfaceSample>()) as vk::DeviceSize;

            let mut emit_objects_buffer = BufferResource::new(
                buffer_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device,
                device_memory_properties,
            );
            emit_objects_buffer.store(&emit_objects, &device);

            emit_objects_buffer
        };

        Self {
            tlas: top_as,
            tlas_emit_object: top_as_emit,
            default_blas,
            blases,
            uniform: uniform_buffer,
            materials: material_buffer,
            buffers,
            index_data,
            indices,
            vertices,
            textures,
            lights,
            area_lights,
            emit_objects,
            images,
        }
    }

    unsafe fn destroy(self, device: &ash::Device, acceleration_structure: &AccelerationStructure) {
        acceleration_structure.destroy_acceleration_structure(self.tlas, None);
        acceleration_structure.destroy_acceleration_structure(self.tlas_emit_object, None);
        acceleration_structure.destroy_acceleration_structure(self.default_blas, None);
        for blas in self.blases {
            acceleration_structure.destroy_acceleration_structure(blas, None);
        }
        self.materials.destroy(device);
        self.uniform.destroy(device);
        for buffer in self.buffers {
            buffer.destroy(device);
        }
        self.index_data.destroy(device);
        self.indices.destroy(device);
        self.vertices.destroy(device);
        self.textures.destroy(device);
        self.lights.destroy(device);
        self.area_lights.destroy(device);
        self.emit_objects.destroy(device);

        for image in self.images {
            image.destroy(device);
        }
    }
}
