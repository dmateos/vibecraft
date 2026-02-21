use std::collections::HashSet;

use bevy::prelude::*;

use crate::config::CHUNK_SIZE;
use crate::net_client::{NetClientState, is_remote_simulation};
use crate::player::FlyCam;
use crate::world::{VoxelWorld, chunk_distance_sq, div_floor};

use super::components::NpcAssets;
use super::movement::{find_spawn_ground, hash3, settlement_anchor_for_position, should_spawn_cell};
use super::{
    NPC_CELL_SIZE, NPC_DESPAWN_RADIUS_CHUNKS, NPC_MAX_COUNT, NPC_MAX_SPAWNS_PER_TICK,
    NPC_SPAWN_RADIUS_CHUNKS,
};
use super::{DeadNpcCells, LoadedNpcs, Npc, NpcKind, NpcRig, NpcStreamTimer};

pub fn stream_npcs_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<NpcStreamTimer>,
    mut loaded: ResMut<LoadedNpcs>,
    dead_cells: Res<DeadNpcCells>,
    npc_state_q: Query<&Npc>,
    net: Option<Res<NetClientState>>,
    world: Res<VoxelWorld>,
    assets: Res<NpcAssets>,
    cam_q: Query<&Transform, With<FlyCam>>,
) {
    if is_remote_simulation(net.as_deref()) {
        loaded.clear_and_despawn(&mut commands);
        return;
    }
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

    let mut desired_cells = HashSet::new();
    let cam_cell = IVec2::new(
        div_floor(cam.translation.x.floor() as i32, NPC_CELL_SIZE),
        div_floor(cam.translation.z.floor() as i32, NPC_CELL_SIZE),
    );
    let cell_radius = ((NPC_SPAWN_RADIUS_CHUNKS * CHUNK_SIZE as i32) / NPC_CELL_SIZE) + 2;

    for dz in -cell_radius..=cell_radius {
        for dx in -cell_radius..=cell_radius {
            let cell = IVec2::new(cam_cell.x + dx, cam_cell.y + dz);
            if !should_spawn_cell(cell, world.seed) {
                continue;
            }
            let wx = cell.x * NPC_CELL_SIZE + NPC_CELL_SIZE / 2;
            let wz = cell.y * NPC_CELL_SIZE + NPC_CELL_SIZE / 2;
            let cc = IVec2::new(
                div_floor(wx, CHUNK_SIZE as i32),
                div_floor(wz, CHUNK_SIZE as i32),
            );
            if chunk_distance_sq(cc, cam_chunk) <= NPC_SPAWN_RADIUS_CHUNKS * NPC_SPAWN_RADIUS_CHUNKS
            {
                desired_cells.insert(cell);
            }
        }
    }

    let loaded_cells: Vec<IVec2> = loaded.entries.keys().copied().collect();
    for cell in loaded_cells {
        let wx = cell.x * NPC_CELL_SIZE + NPC_CELL_SIZE / 2;
        let wz = cell.y * NPC_CELL_SIZE + NPC_CELL_SIZE / 2;
        let cc = IVec2::new(
            div_floor(wx, CHUNK_SIZE as i32),
            div_floor(wz, CHUNK_SIZE as i32),
        );
        let out_of_range = chunk_distance_sq(cc, cam_chunk)
            > NPC_DESPAWN_RADIUS_CHUNKS * NPC_DESPAWN_RADIUS_CHUNKS;
        let should_remove = if out_of_range {
            true
        } else if !desired_cells.contains(&cell) {
            match loaded
                .entries
                .get(&cell)
                .and_then(|e| npc_state_q.get(*e).ok())
            {
                Some(npc) if npc.dead => false,
                _ => true,
            }
        } else {
            false
        };
        if should_remove && let Some(entity) = loaded.entries.remove(&cell) {
            commands.entity(entity).despawn_recursive();
        }
    }

    if loaded.entries.len() >= NPC_MAX_COUNT {
        return;
    }

    let mut to_spawn: Vec<IVec2> = desired_cells
        .into_iter()
        .filter(|cell| !loaded.entries.contains_key(cell))
        .filter(|cell| !dead_cells.is_killed(*cell))
        .collect();
    to_spawn.sort_by_key(|cell| {
        let wx = cell.x * NPC_CELL_SIZE + NPC_CELL_SIZE / 2;
        let wz = cell.y * NPC_CELL_SIZE + NPC_CELL_SIZE / 2;
        let cc = IVec2::new(
            div_floor(wx, CHUNK_SIZE as i32),
            div_floor(wz, CHUNK_SIZE as i32),
        );
        chunk_distance_sq(cc, cam_chunk)
    });

    let budget = NPC_MAX_COUNT
        .saturating_sub(loaded.entries.len())
        .min(NPC_MAX_SPAWNS_PER_TICK);
    to_spawn.truncate(budget);

    for cell in to_spawn {
        let seed = hash3(cell.x, cell.y, world.seed ^ 0xB6F1_94C3);
        let jitter_x = (((seed & 0xFF) as i32) % (NPC_CELL_SIZE - 4)) - (NPC_CELL_SIZE / 2 - 2);
        let jitter_z =
            ((((seed >> 8) & 0xFF) as i32) % (NPC_CELL_SIZE - 4)) - (NPC_CELL_SIZE / 2 - 2);
        let wx = cell.x * NPC_CELL_SIZE + NPC_CELL_SIZE / 2 + jitter_x;
        let wz = cell.y * NPC_CELL_SIZE + NPC_CELL_SIZE / 2 + jitter_z;

        let Some(ground_y) = find_spawn_ground(&world.chunks, wx, wz) else {
            continue;
        };

        let home = settlement_anchor_for_position(world.seed, wx, wz);
        let heading = ((seed >> 16) as f32 / u16::MAX as f32) * std::f32::consts::TAU;
        let spawn_roll = ((seed >> 24) & 0xFF) as u8;
        let kind = if home.is_some() {
            NpcKind::Friendly
        } else if spawn_roll < 28 {
            NpcKind::Hostile
        } else {
            continue;
        };
        let speed = match kind {
            NpcKind::Friendly => 0.62 + ((seed >> 20) as f32 / 255.0) * 0.48,
            NpcKind::Hostile => 0.98 + ((seed >> 20) as f32 / 255.0) * 0.58,
        };

        let torso_material = match kind {
            NpcKind::Hostile => assets.hostile.clone(),
            NpcKind::Friendly => match (seed as usize) % 3 {
                0 => assets.cloth_a.clone(),
                1 => assets.cloth_b.clone(),
                _ => assets.cloth_c.clone(),
            },
        };

        let root = commands
            .spawn((
                SpatialBundle {
                    transform: Transform::from_translation(Vec3::new(
                        wx as f32 + 0.5,
                        ground_y as f32,
                        wz as f32 + 0.5,
                    )),
                    ..default()
                },
                Npc {
                    kind,
                    cell,
                    health: if kind == NpcKind::Hostile { 72.0 } else { 48.0 },
                    dead: false,
                    heading,
                    speed,
                    turn_timer: 0.8 + ((seed >> 10) as f32 / 255.0) * 2.2,
                    vertical_velocity: 0.0,
                    rng: seed ^ 0xD17A_2B4F,
                    follow_player: false,
                    attack_cooldown: 0.0,
                    chat_cooldown: 1.5 + ((seed >> 2) as f32 / 255.0) * 3.0,
                    last_seen_player: Vec3::ZERO,
                    last_seen_timer: 0.0,
                    investigate_target: Vec3::ZERO,
                    investigate_timer: 0.0,
                    last_pos: Vec3::new(wx as f32 + 0.5, ground_y as f32, wz as f32 + 0.5),
                    stuck_timer: 0.0,
                    knockback_velocity: Vec3::ZERO,
                    hurt_stun: 0.0,
                    home_center: home
                        .map(|h| h.0)
                        .unwrap_or(Vec2::new(wx as f32 + 0.5, wz as f32 + 0.5)),
                    home_radius: home.map(|h| h.1).unwrap_or(24.0),
                },
            ))
            .id();

        let (head, torso, left_leg, right_leg, left_arm, right_arm, quadruped) =
            if kind == NpcKind::Hostile {
                let head = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.hostile.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.0, 0.98, -0.42),
                            scale: Vec3::new(0.42, 0.32, 0.52),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let torso = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.hostile.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.0, 0.78, 0.0),
                            scale: Vec3::new(0.82, 0.44, 1.10),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let left_leg = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(-0.30, 0.30, 0.34),
                            scale: Vec3::new(0.16, 0.60, 0.16),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let right_leg = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.30, 0.30, 0.34),
                            scale: Vec3::new(0.16, 0.60, 0.16),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let left_arm = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(-0.30, 0.30, -0.28),
                            scale: Vec3::new(0.16, 0.60, 0.16),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let right_arm = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.30, 0.30, -0.28),
                            scale: Vec3::new(0.16, 0.60, 0.16),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                (head, torso, left_leg, right_leg, left_arm, right_arm, true)
            } else {
                let head = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.0, 1.48, 0.0),
                            scale: Vec3::new(0.36, 0.36, 0.36),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let torso = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: torso_material,
                        transform: Transform {
                            translation: Vec3::new(0.0, 0.94, 0.0),
                            scale: Vec3::new(0.56, 0.72, 0.30),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let left_leg = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(-0.16, 0.34, 0.0),
                            scale: Vec3::new(0.16, 0.68, 0.18),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let right_leg = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.16, 0.34, 0.0),
                            scale: Vec3::new(0.16, 0.68, 0.18),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let left_arm = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(-0.36, 1.02, 0.0),
                            scale: Vec3::new(0.14, 0.52, 0.16),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                let right_arm = commands
                    .spawn(PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.skin.clone(),
                        transform: Transform {
                            translation: Vec3::new(0.36, 1.02, 0.0),
                            scale: Vec3::new(0.14, 0.52, 0.16),
                            ..default()
                        },
                        ..default()
                    })
                    .id();
                (head, torso, left_leg, right_leg, left_arm, right_arm, false)
            };

        commands
            .entity(root)
            .add_child(head)
            .add_child(torso)
            .add_child(left_leg)
            .add_child(right_leg)
            .add_child(left_arm)
            .add_child(right_arm)
            .insert(NpcRig {
                left_leg,
                right_leg,
                left_arm,
                right_arm,
                quadruped,
            });

        loaded.entries.insert(cell, root);
    }
}
