#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use bevy::pbr::{Material, MaterialPlugin, NotShadowCaster};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{AsBindGroup, PrimitiveTopology, ShaderRef, ShaderType};

use crate::config::{CHUNK_SIZE, SEA_LEVEL, VIEW_DISTANCE_CHUNKS};
use crate::player::FlyCam;
use crate::world::{chunk_distance_sq, div_floor, get_block_world, Block, Chunk, VoxelWorld};

const MAX_WATER_CHUNKS_PER_TICK: usize = 24;

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct WaterMaterialParams {
    pub shallow_color: Vec4,
    pub deep_color: Vec4,
    pub wave: Vec4,
    pub foam: Vec4,
    pub weather: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterSurfaceMaterial {
    #[uniform(0)]
    pub params: WaterMaterialParams,
}

impl Material for WaterSurfaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/water_material.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

#[derive(Resource)]
pub struct WaterMaterial(pub Handle<WaterSurfaceMaterial>);

#[derive(Clone)]
struct WaterRender {
    entity: Entity,
}

#[derive(Resource, Default)]
pub struct LoadedWater {
    entries: HashMap<IVec2, WaterRender>,
}

impl LoadedWater {
    pub fn clear_and_despawn(&mut self, commands: &mut Commands) {
        let items: Vec<WaterRender> = self.entries.values().cloned().collect();
        self.entries.clear();
        for item in items {
            commands.entity(item.entity).despawn_recursive();
        }
    }
}

#[derive(Resource)]
pub struct WaterStreamTimer(pub Timer);

pub fn water_material_plugin() -> MaterialPlugin<WaterSurfaceMaterial> {
    MaterialPlugin::<WaterSurfaceMaterial>::default()
}

pub fn setup_water(
    mut commands: Commands,
    mut materials: ResMut<Assets<WaterSurfaceMaterial>>,
) {
    let material = materials.add(WaterSurfaceMaterial {
        params: WaterMaterialParams {
            shallow_color: Vec4::new(0.20, 0.62, 0.92, 0.90),
            deep_color: Vec4::new(0.02, 0.10, 0.24, 0.95),
            wave: Vec4::new(0.085, 2.9, 1.25, 0.5),
            foam: Vec4::new(0.30, 0.0, 0.0, 0.0),
            weather: Vec4::ZERO,
        },
    });

    commands.insert_resource(WaterMaterial(material));
}

pub fn stream_water_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<WaterStreamTimer>,
    mut loaded: ResMut<LoadedWater>,
    world: Res<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    water_material: Res<WaterMaterial>,
    cam_q: Query<&Transform, With<FlyCam>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let cam_chunk = IVec2::new(
        div_floor(cam.translation.x.floor() as i32, CHUNK_SIZE as i32),
        div_floor(cam.translation.z.floor() as i32, CHUNK_SIZE as i32),
    );

    let mut desired = HashSet::new();
    for dz in -VIEW_DISTANCE_CHUNKS..=VIEW_DISTANCE_CHUNKS {
        for dx in -VIEW_DISTANCE_CHUNKS..=VIEW_DISTANCE_CHUNKS {
            if dx * dx + dz * dz > VIEW_DISTANCE_CHUNKS * VIEW_DISTANCE_CHUNKS {
                continue;
            }
            desired.insert(IVec2::new(cam_chunk.x + dx, cam_chunk.y + dz));
        }
    }

    let loaded_positions: Vec<IVec2> = loaded.entries.keys().copied().collect();
    for pos in loaded_positions {
        if !desired.contains(&pos)
            && let Some(entry) = loaded.entries.remove(&pos)
        {
            commands.entity(entry.entity).despawn_recursive();
        }
    }

    let mut to_spawn: Vec<IVec2> = desired
        .iter()
        .copied()
        .filter(|pos| !loaded.entries.contains_key(pos))
        .collect();
    to_spawn.sort_by_key(|pos| chunk_distance_sq(*pos, cam_chunk));
    to_spawn.truncate(MAX_WATER_CHUNKS_PER_TICK);

    for pos in to_spawn {
        let mesh_handle = meshes.add(build_water_mesh_for_chunk(pos, &world.chunks));
        let translation = Vec3::new(
            (pos.x * CHUNK_SIZE as i32) as f32,
            SEA_LEVEL as f32 + 0.08,
            (pos.y * CHUNK_SIZE as i32) as f32,
        );

        let entity = commands
            .spawn((
                MaterialMeshBundle::<WaterSurfaceMaterial> {
                    mesh: mesh_handle,
                    material: water_material.0.clone(),
                    transform: Transform::from_translation(translation),
                    ..default()
                },
                NotShadowCaster,
            ))
            .id();

        loaded.entries.insert(pos, WaterRender { entity });
    }
}

fn build_water_mesh_for_chunk(
    pos: IVec2,
    chunks: &std::collections::HashMap<IVec2, Chunk>,
) -> Mesh {
    const DIVS: usize = 12;
    let size = CHUNK_SIZE as f32;
    let step = size / DIVS as f32;
    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;

    let mut positions = Vec::with_capacity((DIVS + 1) * (DIVS + 1));
    let mut normals = Vec::with_capacity((DIVS + 1) * (DIVS + 1));
    let mut colors = Vec::with_capacity((DIVS + 1) * (DIVS + 1));
    let mut indices = Vec::with_capacity(DIVS * DIVS * 6);

    for gz in 0..=DIVS {
        for gx in 0..=DIVS {
            let lx = gx as f32 * step;
            let lz = gz as f32 * step;
            positions.push([lx, 0.0, lz]);
            normals.push([0.0, 1.0, 0.0]);

            let wx = base_x + lx.round() as i32;
            let wz = base_z + lz.round() as i32;
            let surface = find_surface_y(chunks, wx, wz);
            let depth = ((SEA_LEVEL - surface) as f32 / 24.0).clamp(0.0, 1.0);
            colors.push([depth, 0.0, 0.0, 1.0]);
        }
    }

    for z in 0..DIVS {
        for x in 0..DIVS {
            let i0 = (z * (DIVS + 1) + x) as u32;
            let i1 = i0 + 1;
            let i2 = i0 + (DIVS + 1) as u32;
            let i3 = i2 + 1;
            indices.extend_from_slice(&[i0, i3, i1, i0, i2, i3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn find_surface_y(chunks: &std::collections::HashMap<IVec2, Chunk>, x: i32, z: i32) -> i32 {
    for y in (0..=SEA_LEVEL + 24).rev() {
        let b = get_block_world(chunks, x, y, z);
        if b != Block::Air && b != Block::Leaves {
            return y;
        }
    }
    0
}
