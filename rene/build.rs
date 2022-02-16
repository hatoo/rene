use std::error::Error;

use spirv_builder::{Capability, MetadataPrintout, SpirvBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    std::env::set_var("NO_SPIRV_OPT", "1");
    SpirvBuilder::new("../rene-shader", "spirv-unknown-spv1.3")
        .capability(Capability::RayTracingKHR)
        .capability(Capability::RuntimeDescriptorArray)
        .relax_logical_pointer(true)
        .extension("SPV_KHR_ray_tracing")
        .extension("SPV_EXT_descriptor_indexing")
        .release(true)
        .print_metadata(MetadataPrintout::Full)
        .build()?;

    Ok(())
}
