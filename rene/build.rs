use std::error::Error;

use spirv_builder::{Capability, MetadataPrintout, SpirvBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    let result = SpirvBuilder::new("../rene-shader", "spirv-unknown-spv1.3")
        .capability(Capability::RayTracingKHR)
        .capability(Capability::RuntimeDescriptorArray)
        .relax_logical_pointer(true)
        .extension("SPV_KHR_ray_tracing")
        .extension("SPV_EXT_descriptor_indexing")
        .multimodule(true)
        .release(true)
        .print_metadata(MetadataPrintout::DependencyOnly)
        .build()?;

    for (name, path) in result.module.unwrap_multi() {
        println!("cargo:rustc-env={}={}", name, path.to_str().unwrap());
    }

    Ok(())
}
