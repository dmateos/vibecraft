use std::collections::{HashMap, HashSet};

use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;
use noise::{NoiseFn, Perlin};

use crate::player::FlyCam;
use crate::weather::WeatherState;
use crate::world::{chunk_distance_sq, div_floor, VoxelWorld};

const CLOUD_CELL_SIZE: f32 = 4.0;
const CLOUD_CHUNK_CELLS: i32 = 24;
const CLOUD_VIEW_DISTANCE: i32 = 8;
const CLOUD_LAYER_Y: f32 = 238.0;
const CLOUD_THICKNESS: f32 = 2.2;
const MAX_CLOUD_CHUNKS_PER_TICK: usize = 8;
const CLOUD_DRIFT_SPEED: Vec2 = Vec2::new(1.8, 0.6);

#[derive(Resource)]
pub struct CloudMaterial(pub Handle<StandardMaterial>);

#[derive(Clone)]
struct CloudRender {
    entity: Entity,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct CloudChunk {
    pos: IVec2,
}

#[derive(Resource, Default)]
pub struct LoadedClouds {
    entries: HashMap<IVec2, CloudRender>,
}

impl LoadedClouds {
    pub fn clear_and_despawn(&mut self, commands: &mut Commands) {
        let items: Vec<CloudRender> = self.entries.values().cloned().collect();
        self.entries.clear();
        for item in items {
            commands.entity(item.entity).despawn_recursive();
        }
    }
}

#[derive(Resource)]
pub struct CloudStreamTimer(pub Timer);

pub fn setup_clouds(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.97, 0.98, 1.0, 0.84),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.02,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(CloudMaterial(material));
}

pub fn stream_clouds_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<CloudStreamTimer>,
    world: Res<VoxelWorld>,
    mut loaded: ResMut<LoadedClouds>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<CloudMaterial>,
    cam_q: Query<&Transform, With<FlyCam>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let chunk_world_size = (CLOUD_CHUNK_CELLS as f32 * CLOUD_CELL_SIZE) as i32;
    let cam_chunk = IVec2::new(
        div_floor(cam.translation.x.floor() as i32, chunk_world_size),
        div_floor(cam.translation.z.floor() as i32, chunk_world_size),
    );

    let mut desired = HashSet::new();
    for dz in -CLOUD_VIEW_DISTANCE..=CLOUD_VIEW_DISTANCE {
        for dx in -CLOUD_VIEW_DISTANCE..=CLOUD_VIEW_DISTANCE {
            if dx * dx + dz * dz > CLOUD_VIEW_DISTANCE * CLOUD_VIEW_DISTANCE {
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
    to_spawn.truncate(MAX_CLOUD_CHUNKS_PER_TICK);

    if to_spawn.is_empty() {
        return;
    }

    for pos in to_spawn {
        let mesh = build_cloud_chunk_mesh(pos, world.seed);
        let mesh_handle = meshes.add(mesh);
        let translation = Vec3::new(
            pos.x as f32 * CLOUD_CHUNK_CELLS as f32 * CLOUD_CELL_SIZE,
            CLOUD_LAYER_Y,
            pos.y as f32 * CLOUD_CHUNK_CELLS as f32 * CLOUD_CELL_SIZE,
        );

        let entity = commands
            .spawn((
                PbrBundle {
                    mesh: mesh_handle,
                    material: material.0.clone(),
                    transform: Transform::from_translation(translation),
                    ..default()
                },
                NotShadowCaster,
                NotShadowReceiver,
                CloudChunk { pos },
            ))
            .id();

        loaded.entries.insert(pos, CloudRender { entity });
    }
}

pub fn animate_clouds(
    time: Res<Time>,
    weather: Res<WeatherState>,
    mut q: Query<(&CloudChunk, &mut Transform)>,
) {
    let t = time.elapsed_seconds_wrapped();
    let wind = 1.0 + weather.rain_factor() * 0.7;
    let drift = CLOUD_DRIFT_SPEED * t * wind;
    let chunk_size = CLOUD_CHUNK_CELLS as f32 * CLOUD_CELL_SIZE;

    for (chunk, mut transform) in &mut q {
        transform.translation = Vec3::new(
            chunk.pos.x as f32 * chunk_size + drift.x,
            CLOUD_LAYER_Y
                + (t * 0.27 * wind + (chunk.pos.x ^ chunk.pos.y) as f32 * 0.11).sin()
                    * (0.55 + weather.rain_factor() * 0.18),
            chunk.pos.y as f32 * chunk_size + drift.y,
        );
    }
}

fn build_cloud_chunk_mesh(pos: IVec2, seed: u32) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();

    let cell_origin_x = pos.x * CLOUD_CHUNK_CELLS;
    let cell_origin_z = pos.y * CLOUD_CHUNK_CELLS;
    let perlin_main = Perlin::new(seed ^ 0xA1E5_77C3);
    let perlin_detail = Perlin::new(seed ^ 0x5A32_D191);
    let perlin_mask = Perlin::new(seed ^ 0x7F4A_03BC);

    for z in 0..CLOUD_CHUNK_CELLS {
        for x in 0..CLOUD_CHUNK_CELLS {
            let wx = cell_origin_x + x;
            let wz = cell_origin_z + z;
            if !cloud_cell_filled(wx, wz, &perlin_main, &perlin_detail, &perlin_mask) {
                continue;
            }

            let lx = x as f32 * CLOUD_CELL_SIZE;
            let lz = z as f32 * CLOUD_CELL_SIZE;

            add_face(
                &mut positions,
                &mut normals,
                &mut indices,
                [
                    [lx, CLOUD_THICKNESS, lz + CLOUD_CELL_SIZE],
                    [lx + CLOUD_CELL_SIZE, CLOUD_THICKNESS, lz + CLOUD_CELL_SIZE],
                    [lx + CLOUD_CELL_SIZE, CLOUD_THICKNESS, lz],
                    [lx, CLOUD_THICKNESS, lz],
                ],
                [0.0, 1.0, 0.0],
            );
            add_face(
                &mut positions,
                &mut normals,
                &mut indices,
                [
                    [lx, 0.0, lz],
                    [lx + CLOUD_CELL_SIZE, 0.0, lz],
                    [lx + CLOUD_CELL_SIZE, 0.0, lz + CLOUD_CELL_SIZE],
                    [lx, 0.0, lz + CLOUD_CELL_SIZE],
                ],
                [0.0, 1.0, 0.0],
            );

            if !cloud_cell_filled(wx + 1, wz, &perlin_main, &perlin_detail, &perlin_mask) {
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut indices,
                    [
                        [lx + CLOUD_CELL_SIZE, 0.0, lz],
                        [lx + CLOUD_CELL_SIZE, CLOUD_THICKNESS, lz],
                        [lx + CLOUD_CELL_SIZE, CLOUD_THICKNESS, lz + CLOUD_CELL_SIZE],
                        [lx + CLOUD_CELL_SIZE, 0.0, lz + CLOUD_CELL_SIZE],
                    ],
                    [1.0, 0.0, 0.0],
                );
            }
            if !cloud_cell_filled(wx - 1, wz, &perlin_main, &perlin_detail, &perlin_mask) {
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut indices,
                    [
                        [lx, 0.0, lz + CLOUD_CELL_SIZE],
                        [lx, CLOUD_THICKNESS, lz + CLOUD_CELL_SIZE],
                        [lx, CLOUD_THICKNESS, lz],
                        [lx, 0.0, lz],
                    ],
                    [-1.0, 0.0, 0.0],
                );
            }
            if !cloud_cell_filled(wx, wz + 1, &perlin_main, &perlin_detail, &perlin_mask) {
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut indices,
                    [
                        [lx + CLOUD_CELL_SIZE, 0.0, lz + CLOUD_CELL_SIZE],
                        [lx + CLOUD_CELL_SIZE, CLOUD_THICKNESS, lz + CLOUD_CELL_SIZE],
                        [lx, CLOUD_THICKNESS, lz + CLOUD_CELL_SIZE],
                        [lx, 0.0, lz + CLOUD_CELL_SIZE],
                    ],
                    [0.0, 0.0, 1.0],
                );
            }
            if !cloud_cell_filled(wx, wz - 1, &perlin_main, &perlin_detail, &perlin_mask) {
                add_face(
                    &mut positions,
                    &mut normals,
                    &mut indices,
                    [
                        [lx, 0.0, lz],
                        [lx, CLOUD_THICKNESS, lz],
                        [lx + CLOUD_CELL_SIZE, CLOUD_THICKNESS, lz],
                        [lx + CLOUD_CELL_SIZE, 0.0, lz],
                    ],
                    [0.0, 0.0, -1.0],
                );
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh
}

fn add_face(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    verts: [[f32; 3]; 4],
    normal: [f32; 3],
) {
    let start = positions.len() as u32;
    positions.extend_from_slice(&verts);
    normals.extend_from_slice(&[normal, normal, normal, normal]);
    indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn cloud_cell_filled(
    x: i32,
    z: i32,
    perlin_main: &Perlin,
    perlin_detail: &Perlin,
    perlin_mask: &Perlin,
) -> bool {
    let a = perlin_main.get([x as f64 * 0.052, z as f64 * 0.052]) as f32;
    let b = perlin_detail.get([x as f64 * 0.018, z as f64 * 0.018]) as f32;
    let mask = perlin_mask.get([x as f64 * 0.006 + 913.7, z as f64 * 0.006 - 147.3]) as f32;
    let density = a * 0.66 + b * 0.34;
    density > 0.24 && mask > -0.08
}
