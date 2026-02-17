#![allow(dead_code)]

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
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct VoxelMaterial {
    #[uniform(0)]
    pub params: VoxelMaterialParams,
}

impl Material for VoxelMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/voxel_material.wgsl".into()
    }
}
