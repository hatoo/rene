#[cfg(target_arch = "spirv")]
use core::arch::asm;

#[cfg(not(target_arch = "spirv"))]
pub fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

#[spirv_std_macros::gpu_only]
#[cfg(target_arch = "spirv")]
#[inline]
pub fn u32_to_f32(value: u32) -> f32 {
    let mut fvalue: f32;

    unsafe {
        asm! {
            "%float = OpTypeFloat 32",
            "{result} = OpConvertUToF %float {value}",
            value = in(reg) value,
            result = out(reg) fvalue
        }
    }
    fvalue
}

#[cfg(not(target_arch = "spirv"))]
pub fn f32_to_u32(value: f32) -> u32 {
    value as u32
}

#[spirv_std_macros::gpu_only]
#[cfg(target_arch = "spirv")]
#[inline]
pub fn f32_to_u32(value: f32) -> u32 {
    let mut uvalue: u32;

    unsafe {
        asm! {
            "%uint = OpTypeInt 32 0",
            "{result} = OpConvertFToU %uint {value}",
            value = in(reg) value,
            result = out(reg) uvalue
        }
    }
    uvalue
}
