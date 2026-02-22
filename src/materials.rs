//! Terrain material types and shader binding layouts.
//! Defines the GPU parameter interface used by voxel terrain rendering and
//! keeps material/shader coupling explicit for visual tuning work.
#![allow(dead_code)] // Suppress encase/Bevy `ShaderType` derive false-positive `check` warnings.

use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};

#[derive(Resource)]
pub struct TerrainMaterial(pub Handle<VoxelMaterial>);

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct VoxelMaterialParams {
    pub sun_dir_and_strength: Vec4,
    pub fog_color: Vec4,
    pub fog_distances: Vec4,
    pub ao: Vec4,
    pub weather: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct VoxelMaterial {
    #[uniform(0)]
    pub params: VoxelMaterialParams,
    #[texture(1)]
    #[sampler(2)]
    pub atlas: Handle<Image>,
}

impl Material for VoxelMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/voxel_material.wgsl".into()
    }
}
