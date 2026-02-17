use std::collections::HashSet;

use bevy::prelude::*;
use bevy::render::view::NoFrustumCulling;
use rayon::prelude::*;

use crate::config::{
    CHUNK_SIZE, MAX_CHUNKS_GENERATED_PER_TICK, MAX_CHUNKS_MESHED_PER_TICK, VIEW_DISTANCE_CHUNKS,
};
use crate::materials::{TerrainMaterial, VoxelMaterial};
use crate::player::FlyCam;
use crate::world::{
    build_chunk_mesh, chunk_distance_sq, div_floor, generate_chunk, ChunkRender, LoadedChunks, StreamTimer,
    TerrainMode, VoxelWorld,
};

pub fn stream_chunks_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<StreamTimer>,
    mut world: ResMut<VoxelWorld>,
    terrain_mode: Res<TerrainMode>,
    mut loaded: ResMut<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<TerrainMaterial>,
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
        if !desired.contains(&pos) {
            if let Some(entry) = loaded.entries.remove(&pos) {
                commands.entity(entry.entity).despawn_recursive();
            }
            world.chunks.remove(&pos);
        }
    }

    let mut to_generate: Vec<IVec2> = desired
        .iter()
        .copied()
        .filter(|pos| !world.chunks.contains_key(pos))
        .collect();
    to_generate.sort_by_key(|pos| chunk_distance_sq(*pos, cam_chunk));
    to_generate.truncate(MAX_CHUNKS_GENERATED_PER_TICK);

    if !to_generate.is_empty() {
        let seed = world.seed;
        let mode = *terrain_mode;
        let generated = to_generate
            .par_iter()
            .map(|pos| generate_chunk(*pos, seed, mode))
            .collect::<Vec<_>>();

        for chunk in generated {
            world.chunks.insert(chunk.pos, chunk);
        }
    }

    let mut to_spawn: Vec<IVec2> = desired
        .iter()
        .copied()
        .filter(|pos| !loaded.entries.contains_key(pos))
        .filter(|pos| world.chunks.contains_key(pos))
        .collect();
    to_spawn.sort_by_key(|pos| chunk_distance_sq(*pos, cam_chunk));
    to_spawn.truncate(MAX_CHUNKS_MESHED_PER_TICK);

    if to_spawn.is_empty() {
        return;
    }

    let chunk_map = &world.chunks;
    let built = to_spawn
        .par_iter()
        .map(|pos| (*pos, build_chunk_mesh(*pos, chunk_map)))
        .collect::<Vec<_>>();

    for (pos, mesh) in built {
        let mesh_handle = meshes.add(mesh);
        let translation = Vec3::new(
            (pos.x * CHUNK_SIZE as i32) as f32,
            0.0,
            (pos.y * CHUNK_SIZE as i32) as f32,
        );

        let entity = commands
            .spawn((
                MaterialMeshBundle::<VoxelMaterial> {
                    mesh: mesh_handle.clone(),
                    material: material.0.clone(),
                    transform: Transform::from_translation(translation),
                    ..default()
                },
                NoFrustumCulling,
            ))
            .id();

        loaded
            .entries
            .insert(pos, ChunkRender { entity, mesh: mesh_handle });
    }
}
