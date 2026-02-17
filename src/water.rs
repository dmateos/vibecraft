use std::collections::{HashMap, HashSet};

use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;

use crate::config::{CHUNK_SIZE, SEA_LEVEL, VIEW_DISTANCE_CHUNKS};
use crate::player::FlyCam;
use crate::world::{chunk_distance_sq, div_floor};

const MAX_WATER_CHUNKS_PER_TICK: usize = 24;

#[derive(Resource)]
pub struct WaterMaterial(pub Handle<StandardMaterial>);

#[derive(Resource)]
pub struct WaterMesh(pub Handle<Mesh>);

#[derive(Clone)]
struct WaterRender {
    entity: Entity,
}

#[derive(Resource, Default)]
pub struct LoadedWater {
    entries: HashMap<IVec2, WaterRender>,
}

#[derive(Resource)]
pub struct WaterStreamTimer(pub Timer);

pub fn setup_water(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.10, 0.36, 0.64, 0.72),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.08,
        metallic: 0.02,
        reflectance: 0.35,
        cull_mode: None,
        ..default()
    });

    let mesh = meshes.add(build_water_mesh());
    commands.insert_resource(WaterMaterial(material));
    commands.insert_resource(WaterMesh(mesh));
}

pub fn stream_water_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<WaterStreamTimer>,
    mut loaded: ResMut<LoadedWater>,
    water_mesh: Res<WaterMesh>,
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
        let translation = Vec3::new(
            (pos.x * CHUNK_SIZE as i32) as f32,
            SEA_LEVEL as f32 + 0.08,
            (pos.y * CHUNK_SIZE as i32) as f32,
        );

        let entity = commands
            .spawn((
                PbrBundle {
                    mesh: water_mesh.0.clone(),
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

fn build_water_mesh() -> Mesh {
    let size = CHUNK_SIZE as f32;
    let positions = vec![[0.0, 0.0, 0.0], [size, 0.0, 0.0], [size, 0.0, size], [0.0, 0.0, size]];
    let normals = vec![[0.0, 1.0, 0.0]; 4];
    let indices = vec![0_u32, 1, 2, 0, 2, 3];

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh
}
