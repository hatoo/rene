# Rene

WIP Vulkan Raytracing renderer entirely written by Rust.

# Requirements

- Vulkan Raytracing ready GPU and Driver
- [LunarG Vulkan SDK](https://www.lunarg.com/vulkan-sdk/)
# Run

```
cargo run -- sample_scenes/current.pbrt
```

`out.png` will be produced.

## OptiX Denoiser

Rene has built-in OptiX Denoiser if you enable "optix-denoiser" feature.

You can denoise images by "--optix-denoiser" flag.

# Examples

## Cornell box

With Optix Denoiser

```
cargo run --features=optix-denoiser  -- --optix-denoiser  .\sample_scenes\cornell-box\scene.pbrt  
```

![Cornell box](images/cornell-box.png)