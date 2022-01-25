use spirv_std::glam::Vec4;

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(u32)]
pub enum MicrofacetDistributionType {
    TrowbridgeReitz,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
#[repr(C)]
pub struct EnumMicrofacetDistributionData {
    v0: Vec4,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "spirv"), derive(Debug))]
pub struct EnumMicrofacetDistribution {
    t: MicrofacetDistributionType,
    data: EnumMicrofacetDistributionData,
}
