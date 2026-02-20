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
            shallow_color: Vec4::new(0.10, 0.58, 0.88, 0.90),
            deep_color: Vec4::new(0.01, 0.12, 0.36, 0.96),
            wave: Vec4::new(0.080, 2.7, 1.15, 0.5),
            foam: Vec4::new(0.26, 0.0, 0.0, 0.0),
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
        .filter(|pos| world.chunks.contains_key(pos))
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
    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;

    let mut positions = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 4);
    let mut normals = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 4);
    let mut colors = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 4);
    let mut indices = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 6);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = base_x + x as i32;
            let wz = base_z + z as i32;
            let surface = find_surface_y(chunks, wx, wz);
            let surface_smooth = sample_surface_avg(chunks, wx, wz);
            // Draw water where smoothed terrain lies below sea level.
            if surface_smooth >= SEA_LEVEL as f32 - 0.05 {
                continue;
            }

            let depth = ((SEA_LEVEL as f32 - surface_smooth) / 22.0).clamp(0.0, 1.0);
            let shore_soften = ((SEA_LEVEL as f32 - surface as f32) / 4.0).clamp(0.0, 1.0);
            let depth = depth * 0.82 + shore_soften * 0.18;
            let start = positions.len() as u32;

            positions.extend_from_slice(&[
                [x as f32, 0.0, z as f32],
                [x as f32 + 1.0, 0.0, z as f32],
                [x as f32 + 1.0, 0.0, z as f32 + 1.0],
                [x as f32, 0.0, z as f32 + 1.0],
            ]);
            normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
            colors.extend_from_slice(&[[depth, 0.0, 0.0, 1.0]; 4]);

            indices.extend_from_slice(&[
                start,
                start + 2,
                start + 1,
                start,
                start + 3,
                start + 2,
            ]);
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

fn sample_surface_avg(chunks: &std::collections::HashMap<IVec2, Chunk>, x: i32, z: i32) -> f32 {
    let mut sum = 0.0;
    let mut wsum = 0.0;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let w = if dx == 0 && dz == 0 { 0.32 } else { 0.085 };
            let y = find_surface_y(chunks, x + dx, z + dz) as f32;
            sum += y * w;
            wsum += w;
        }
    }
    if wsum > 0.0 { sum / wsum } else { SEA_LEVEL as f32 }
}
