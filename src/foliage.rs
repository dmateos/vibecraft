use std::collections::{HashMap, HashSet};

use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::config::{CHUNK_SIZE, VIEW_DISTANCE_CHUNKS};
use crate::player::FlyCam;
use crate::weather::WeatherState;
use crate::world::{chunk_distance_sq, div_floor, Chunk, VoxelWorld};

const FOLIAGE_VIEW_DISTANCE: i32 = VIEW_DISTANCE_CHUNKS - 4;
const MAX_FOLIAGE_CHUNKS_PER_TICK: usize = 20;
const MAX_FOLIAGE_DESPAWNS_PER_TICK: usize = 80;

#[derive(Resource)]
pub struct FoliageMaterial(pub Handle<StandardMaterial>);

#[derive(Component)]
pub struct FoliageChunk {
    base: Vec3,
    phase: f32,
    amp: f32,
    freq: f32,
}

#[derive(Clone)]
struct FoliageRender {
    entity: Entity,
}

#[derive(Resource, Default)]
pub struct LoadedFoliage {
    entries: HashMap<IVec2, FoliageRender>,
}

impl LoadedFoliage {
    pub fn clear_and_despawn(&mut self, commands: &mut Commands) {
        let items: Vec<FoliageRender> = self.entries.values().cloned().collect();
        self.entries.clear();
        for item in items {
            commands.entity(item.entity).despawn_recursive();
        }
    }
}

#[derive(Resource)]
pub struct FoliageStreamTimer(pub Timer);

pub fn setup_foliage(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        fog_enabled: true,
        ..default()
    });
    commands.insert_resource(FoliageMaterial(mat));
}

pub fn stream_foliage_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<FoliageStreamTimer>,
    world: Res<VoxelWorld>,
    mut loaded: ResMut<LoadedFoliage>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<FoliageMaterial>,
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
    for dz in -FOLIAGE_VIEW_DISTANCE..=FOLIAGE_VIEW_DISTANCE {
        for dx in -FOLIAGE_VIEW_DISTANCE..=FOLIAGE_VIEW_DISTANCE {
            if dx * dx + dz * dz > FOLIAGE_VIEW_DISTANCE * FOLIAGE_VIEW_DISTANCE {
                continue;
            }
            desired.insert(IVec2::new(cam_chunk.x + dx, cam_chunk.y + dz));
        }
    }

    let mut to_remove = Vec::new();
    for pos in loaded.entries.keys().copied() {
        if !desired.contains(&pos) {
            to_remove.push(pos);
        }
    }
    to_remove.sort_by_key(|p| -chunk_distance_sq(*p, cam_chunk));
    to_remove.truncate(MAX_FOLIAGE_DESPAWNS_PER_TICK);
    for pos in to_remove {
        if let Some(entry) = loaded.entries.remove(&pos) {
            commands.entity(entry.entity).despawn_recursive();
        }
    }

    let mut to_spawn: Vec<IVec2> = desired
        .iter()
        .copied()
        .filter(|p| !loaded.entries.contains_key(p))
        .filter(|p| world.chunks.contains_key(p))
        .collect();
    to_spawn.sort_by_key(|p| chunk_distance_sq(*p, cam_chunk));
    to_spawn.truncate(MAX_FOLIAGE_CHUNKS_PER_TICK);

    for pos in to_spawn {
        let Some((mesh, amp, freq, phase)) = build_foliage_mesh_for_chunk(pos, &world.chunks, world.seed)
        else {
            continue;
        };

        let entity = commands
            .spawn((
                PbrBundle {
                    mesh: meshes.add(mesh),
                    material: material.0.clone(),
                    transform: Transform::from_translation(Vec3::new(
                        (pos.x * CHUNK_SIZE as i32) as f32,
                        0.0,
                        (pos.y * CHUNK_SIZE as i32) as f32,
                    )),
                    ..default()
                },
                FoliageChunk {
                    base: Vec3::new(
                        (pos.x * CHUNK_SIZE as i32) as f32,
                        0.0,
                        (pos.y * CHUNK_SIZE as i32) as f32,
                    ),
                    phase,
                    amp,
                    freq,
                },
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();

        loaded.entries.insert(pos, FoliageRender { entity });
    }
}

pub fn animate_foliage(time: Res<Time>, weather: Res<WeatherState>, mut q: Query<(&FoliageChunk, &mut Transform)>) {
    let t = time.elapsed_seconds_wrapped();
    let wind = 1.0 + weather.rain_factor() * 0.85;

    for (fol, mut transform) in &mut q {
        let s = (t * fol.freq * wind + fol.phase).sin();
        transform.translation = fol.base;
        transform.rotation = Quat::from_rotation_y(s * fol.amp * 0.10);
    }
}

fn build_foliage_mesh_for_chunk(
    _pos: IVec2,
    _chunks: &HashMap<IVec2, Chunk>,
    _seed: u32,
) -> Option<(Mesh, f32, f32, f32)> {
    None
}
