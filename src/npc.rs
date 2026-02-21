//! NPC spawning, behavior simulation, combat response, and debug drawing.
//! Owns friendly/hostile decision loops, movement constraints, and player
//! interaction hooks while supporting authority split for networked mode.
use std::collections::{HashMap, HashSet};

use bevy::math::primitives::Cuboid;
use bevy::prelude::*;

use crate::config::{CHUNK_SIZE, SEA_LEVEL, WORLD_HEIGHT};
use crate::generation::PromptInputState;
use crate::net_client::NetClientState;
use crate::player::FlyCam;
use crate::ui::DebugOverlayState;
use crate::world::{chunk_distance_sq, div_floor, get_block_world, Block, Chunk, VoxelWorld};

const NPC_HEIGHT: f32 = 1.72;
const NPC_RADIUS: f32 = 0.28;
const NPC_STEP_HEIGHT: f32 = 1.05;
const NPC_GRAVITY: f32 = -22.0;
const NPC_MAX_FALL: f32 = -30.0;
const NPC_SPAWN_RADIUS_CHUNKS: i32 = 6;
const NPC_DESPAWN_RADIUS_CHUNKS: i32 = 10;
const NPC_MAX_COUNT: usize = 14;
const NPC_MAX_SPAWNS_PER_TICK: usize = 2;
const NPC_CELL_SIZE: i32 = 18;
const FRIENDLY_INTERACT_RANGE: f32 = 4.8;
const HOSTILE_AGGRO_RANGE: f32 = 18.0;
const HOSTILE_ATTACK_RANGE: f32 = 1.45;
const WATER_AVOID_LEVEL: i32 = SEA_LEVEL;
const HOSTILE_VISION_RANGE: f32 = 32.0;
const FRIENDLY_VISION_RANGE: f32 = 18.0;
const HOSTILE_VISION_DOT: f32 = -0.15;
const FRIENDLY_VISION_DOT: f32 = -0.40;
const HOSTILE_HEARING_RANGE: f32 = 52.0;
const NPC_SIGHT_MEMORY: f32 = 3.0;
const NPC_INVESTIGATE_MEMORY: f32 = 4.5;
const VILLAGE_SETTLEMENT_RADIUS: f32 = 40.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpcKind {
    Friendly,
    Hostile,
}

#[derive(Component)]
pub struct Npc {
    pub kind: NpcKind,
    pub cell: IVec2,
    pub health: f32,
    pub dead: bool,
    pub heading: f32,
    pub speed: f32,
    pub turn_timer: f32,
    pub vertical_velocity: f32,
    pub rng: u32,
    pub follow_player: bool,
    pub attack_cooldown: f32,
    pub chat_cooldown: f32,
    pub last_seen_player: Vec3,
    pub last_seen_timer: f32,
    pub investigate_target: Vec3,
    pub investigate_timer: f32,
    pub last_pos: Vec3,
    pub stuck_timer: f32,
    pub knockback_velocity: Vec3,
    pub hurt_stun: f32,
    pub home_center: Vec2,
    pub home_radius: f32,
}

#[derive(Component)]
pub struct NpcRig {
    pub left_leg: Entity,
    pub right_leg: Entity,
    pub left_arm: Entity,
    pub right_arm: Entity,
    pub quadruped: bool,
}

#[derive(Resource, Default)]
pub struct LoadedNpcs {
    entries: HashMap<IVec2, Entity>,
}

#[derive(Resource, Default)]
pub struct DeadNpcCells {
    killed: HashSet<IVec2>,
}

impl DeadNpcCells {
    pub fn clear(&mut self) {
        self.killed.clear();
    }

    pub fn mark_killed(&mut self, cell: IVec2) {
        self.killed.insert(cell);
    }

    pub fn is_killed(&self, cell: IVec2) -> bool {
        self.killed.contains(&cell)
    }
}

impl LoadedNpcs {
    pub fn clear_and_despawn(&mut self, commands: &mut Commands) {
        let entities: Vec<Entity> = self.entries.values().copied().collect();
        self.entries.clear();
        for entity in entities {
            commands.entity(entity).despawn_recursive();
        }
    }

}

#[derive(Resource)]
pub struct NpcStreamTimer(pub Timer);

#[derive(Resource, Debug, Clone, Copy)]
pub struct NpcStimulus {
    pub loud_pos: Vec3,
    pub ttl: f32,
}

impl Default for NpcStimulus {
    fn default() -> Self {
        Self {
            loud_pos: Vec3::ZERO,
            ttl: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct NpcUiState {
    pub message: String,
    pub ttl: f32,
}

impl Default for NpcUiState {
    fn default() -> Self {
        Self {
            message: "No NPC nearby".to_string(),
            ttl: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerVitals {
    pub health: f32,
    pub max_health: f32,
}

impl Default for PlayerVitals {
    fn default() -> Self {
        Self {
            health: 100.0,
            max_health: 100.0,
        }
    }
}

#[derive(Resource)]
pub struct NpcAssets {
    mesh: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    cloth_a: Handle<StandardMaterial>,
    cloth_b: Handle<StandardMaterial>,
    cloth_c: Handle<StandardMaterial>,
    hostile: Handle<StandardMaterial>,
}

pub fn setup_npcs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let skin = materials.add(StandardMaterial {
        base_color: Color::srgb(0.80, 0.68, 0.55),
        perceptual_roughness: 0.95,
        ..default()
    });
    let cloth_a = materials.add(StandardMaterial {
        base_color: Color::srgb(0.56, 0.66, 0.84),
        perceptual_roughness: 0.95,
        ..default()
    });
    let cloth_b = materials.add(StandardMaterial {
        base_color: Color::srgb(0.64, 0.78, 0.60),
        perceptual_roughness: 0.95,
        ..default()
    });
    let cloth_c = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.68, 0.52),
        perceptual_roughness: 0.95,
        ..default()
    });
    let hostile = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.30, 0.30),
        perceptual_roughness: 0.93,
        ..default()
    });

    commands.insert_resource(NpcAssets {
        mesh,
        skin,
        cloth_a,
        cloth_b,
        cloth_c,
        hostile,
    });
}

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
    if let Some(net) = net
        && net.cfg.enabled
        && net.connected
        && !net.is_authority()
    {
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
            if chunk_distance_sq(cc, cam_chunk)
                <= NPC_SPAWN_RADIUS_CHUNKS * NPC_SPAWN_RADIUS_CHUNKS
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
        let out_of_range =
            chunk_distance_sq(cc, cam_chunk) > NPC_DESPAWN_RADIUS_CHUNKS * NPC_DESPAWN_RADIUS_CHUNKS;
        let should_remove = if out_of_range {
            true
        } else if !desired_cells.contains(&cell) {
            match loaded.entries.get(&cell).and_then(|e| npc_state_q.get(*e).ok()) {
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
        let jitter_z = ((((seed >> 8) & 0xFF) as i32) % (NPC_CELL_SIZE - 4)) - (NPC_CELL_SIZE / 2 - 2);
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
                    home_center: home.map(|h| h.0).unwrap_or(Vec2::new(wx as f32 + 0.5, wz as f32 + 0.5)),
                    home_radius: home.map(|h| h.1).unwrap_or(24.0),
                },
            ))
            .id();

        let (head, torso, left_leg, right_leg, left_arm, right_arm, quadruped) = if kind == NpcKind::Hostile {
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

pub fn capture_player_noise(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    net: Option<Res<NetClientState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut stim: ResMut<NpcStimulus>,
) {
    if let Some(net) = net
        && net.cfg.enabled
        && net.connected
        && !net.is_authority()
    {
        return;
    }
    stim.ttl = (stim.ttl - time.delta_seconds()).max(0.0);
    if prompt.active {
        return;
    }

    let mut loud = false;
    if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::KeyE) {
        loud = true;
    }
    if !loud {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };
    stim.loud_pos = cam.translation;
    stim.ttl = 1.4;
}

pub fn npc_interactions(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    net: Option<Res<NetClientState>>,
    mut npcs: Query<(&Transform, &mut Npc)>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut ui: ResMut<NpcUiState>,
) {
    if let Some(net) = net
        && net.cfg.enabled
        && net.connected
        && !net.is_authority()
    {
        return;
    }
    if prompt.active || !keys.just_pressed(KeyCode::KeyE) {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let eye = cam.translation;
    let forward = cam.forward();

    let mut found = false;
    let mut best_dist = f32::INFINITY;
    for (transform, mut npc) in &mut npcs {
        if npc.kind != NpcKind::Friendly {
            continue;
        }
        let delta = transform.translation - eye;
        let dist = delta.length();
        if dist > FRIENDLY_INTERACT_RANGE {
            continue;
        }
        let dot = forward.dot(delta.normalize_or_zero());
        if dot < 0.35 {
            continue;
        }
        if dist >= best_dist {
            continue;
        }
        found = true;
        best_dist = dist;
        npc.follow_player = !npc.follow_player;
        ui.message = if npc.follow_player {
            "Friendly: I will follow you".to_string()
        } else {
            "Friendly: I will stay here".to_string()
        };
        ui.ttl = 3.2;
    }

    if !found {
        ui.message = "No friendly NPC in range (look at one and press E)".to_string();
        ui.ttl = 2.8;
    }
}

pub fn tick_npcs(
    time: Res<Time>,
    world: Res<VoxelWorld>,
    net: Option<Res<NetClientState>>,
    cam_q: Query<&Transform, (With<FlyCam>, Without<Npc>)>,
    mut ui: ResMut<NpcUiState>,
    mut vitals: ResMut<PlayerVitals>,
    stim: Res<NpcStimulus>,
    mut qset: ParamSet<(
        Query<(&mut Transform, &mut Npc, &NpcRig), Without<FlyCam>>,
        Query<&mut Transform, (Without<Npc>, Without<FlyCam>)>,
    )>,
) {
    if let Some(net) = net
        && net.cfg.enabled
        && net.connected
        && !net.is_authority()
    {
        return;
    }
    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let dt = time.delta_seconds();
    let player_pos = cam.translation;
    let mut rig_updates: Vec<(Entity, Entity, Entity, Entity, f32, bool)> = Vec::new();

    if ui.ttl > 0.0 {
        ui.ttl = (ui.ttl - dt).max(0.0);
        if ui.ttl <= 0.0 {
            ui.message = "No NPC nearby".to_string();
        }
    }

    for (mut transform, mut npc, rig) in &mut qset.p0() {
        npc.attack_cooldown = (npc.attack_cooldown - dt).max(0.0);
        npc.chat_cooldown = (npc.chat_cooldown - dt).max(0.0);
        npc.last_seen_timer = (npc.last_seen_timer - dt).max(0.0);
        npc.investigate_timer = (npc.investigate_timer - dt).max(0.0);
        npc.hurt_stun = (npc.hurt_stun - dt).max(0.0);

        if npc.dead {
            let kb = npc.knockback_velocity * dt;
            let next = transform.translation + kb;
            if !collides_npc(&world.chunks, next) {
                transform.translation = next;
            }
            npc.knockback_velocity *= 0.82_f32.powf(dt * 60.0);
            npc.knockback_velocity.y += NPC_GRAVITY * dt * 0.25;
            continue;
        }

        let to_player = player_pos - transform.translation;
        let player_dist = to_player.length();
        let can_see_player = match npc.kind {
            NpcKind::Friendly => can_see_target(
                transform.translation,
                npc.heading,
                player_pos,
                FRIENDLY_VISION_RANGE,
                FRIENDLY_VISION_DOT,
                &world.chunks,
            ),
            NpcKind::Hostile => can_see_target(
                transform.translation,
                npc.heading,
                player_pos,
                HOSTILE_VISION_RANGE,
                HOSTILE_VISION_DOT,
                &world.chunks,
            ),
        };
        if can_see_player {
            npc.last_seen_player = player_pos;
            npc.last_seen_timer = NPC_SIGHT_MEMORY;
        }
        if stim.ttl > 0.0 && transform.translation.distance(stim.loud_pos) <= HOSTILE_HEARING_RANGE {
            npc.investigate_target = stim.loud_pos;
            npc.investigate_timer = NPC_INVESTIGATE_MEMORY;
        }

        let mut desired_dir = Vec2::new(npc.heading.cos(), npc.heading.sin());
        match npc.kind {
            NpcKind::Friendly => {
                let to_home = npc.home_center - transform.translation.xz();
                let dist_home = to_home.length();
                if dist_home > npc.home_radius + 4.0 {
                    desired_dir = to_home.normalize_or_zero();
                    npc.follow_player = false;
                } else if dist_home > npc.home_radius - 1.5 && !npc.follow_player {
                    desired_dir = to_home.normalize_or_zero();
                }

                if npc.follow_player {
                    if player_dist > 2.4 {
                        desired_dir = to_player.xz().normalize_or_zero();
                    } else if player_dist < 1.6 {
                        desired_dir = -to_player.xz().normalize_or_zero();
                    } else {
                        desired_dir = Vec2::ZERO;
                    }
                } else {
                    npc.turn_timer -= dt;
                    if npc.turn_timer <= 0.0 {
                        let r = next_rand(&mut npc.rng);
                        npc.heading += (r - 0.5) * 1.6;
                        npc.turn_timer = 1.0 + next_rand(&mut npc.rng) * 2.8;
                    }
                }

                if !npc.follow_player && player_dist < 6.0 && npc.chat_cooldown <= 0.0 {
                    if next_rand(&mut npc.rng) > 0.65 {
                        ui.message = random_friendly_line(&mut npc.rng).to_string();
                        ui.ttl = 2.6;
                    }
                    npc.chat_cooldown = 7.0 + next_rand(&mut npc.rng) * 10.0;
                }
            }
            NpcKind::Hostile => {
                if can_see_player || player_dist <= HOSTILE_AGGRO_RANGE {
                    desired_dir = to_player.xz().normalize_or_zero();
                } else if npc.last_seen_timer > 0.0 {
                    desired_dir = (npc.last_seen_player - transform.translation).xz().normalize_or_zero();
                } else if npc.investigate_timer > 0.0 {
                    desired_dir = (npc.investigate_target - transform.translation).xz().normalize_or_zero();
                } else {
                    npc.turn_timer -= dt;
                    if npc.turn_timer <= 0.0 {
                        npc.heading += (next_rand(&mut npc.rng) - 0.5) * 2.1;
                        npc.turn_timer = 0.8 + next_rand(&mut npc.rng) * 1.8;
                    }
                }

                if player_dist <= HOSTILE_ATTACK_RANGE && npc.attack_cooldown <= 0.0 {
                    vitals.health = (vitals.health - 8.0).max(0.0);
                    npc.attack_cooldown = 1.25;
                    ui.message = format!("Hostile hit you! HP {:.0}", vitals.health);
                    ui.ttl = 1.8;
                }
            }
        }

        let moving_intent = desired_dir.length_squared() > 0.001;
        let mut turn_mag = 0.0f32;
        if moving_intent {
            let base_heading = desired_dir.y.atan2(desired_dir.x);
            let target_heading = choose_walk_heading(
                npc.heading,
                base_heading,
                transform.translation,
                npc.speed,
                dt,
                &world.chunks,
            );
            let delta = wrap_angle(target_heading - npc.heading);
            turn_mag = delta.abs();
            npc.heading += delta.clamp(-2.2 * dt, 2.2 * dt);
        }

        let dir = Vec2::new(npc.heading.cos(), npc.heading.sin());
        let mut move_speed = if moving_intent { npc.speed } else { 0.0 };
        if npc.hurt_stun > 0.0 {
            move_speed *= 0.45;
        }
        let turn_slow = (1.0 - (turn_mag / std::f32::consts::PI) * 0.55).clamp(0.45, 1.0);
        move_speed *= turn_slow;
        let ahead = transform.translation + Vec3::new(dir.x * 0.9, -0.05, dir.y * 0.9);
        if move_speed > 0.001 {
            if !has_support(&world.chunks, ahead.x.floor() as i32, ahead.z.floor() as i32, ahead.y.floor() as i32)
                || enters_water(&world.chunks, ahead.x.floor() as i32, ahead.z.floor() as i32)
                || collides_npc(
                    &world.chunks,
                    transform.translation + Vec3::new(dir.x * move_speed * dt, 0.0, dir.y * move_speed * dt),
                )
            {
                npc.heading += (next_rand(&mut npc.rng) - 0.5) * 2.6;
            }
        }

        let dir = Vec2::new(npc.heading.cos(), npc.heading.sin());
        let mut pos = transform.translation;
        if npc.knockback_velocity.length_squared() > 0.0001 {
            let kb = npc.knockback_velocity * dt;
            if !collides_npc(&world.chunks, pos + kb) {
                pos += kb;
            }
            npc.knockback_velocity *= 0.80_f32.powf(dt * 60.0);
        }

        let x_step = Vec3::new(dir.x * move_speed * dt, 0.0, 0.0);
        if !collides_npc(&world.chunks, pos + x_step)
            && !enters_water(
                &world.chunks,
                (pos.x + x_step.x).floor() as i32,
                pos.z.floor() as i32,
            )
        {
            pos += x_step;
        } else if let Some(stepped) = try_step_up_npc(pos, x_step, &world.chunks) {
            pos = stepped;
        }

        let z_step = Vec3::new(0.0, 0.0, dir.y * move_speed * dt);
        if !collides_npc(&world.chunks, pos + z_step)
            && !enters_water(
                &world.chunks,
                pos.x.floor() as i32,
                (pos.z + z_step.z).floor() as i32,
            )
        {
            pos += z_step;
        } else if let Some(stepped) = try_step_up_npc(pos, z_step, &world.chunks) {
            pos = stepped;
        }

        npc.vertical_velocity = (npc.vertical_velocity + NPC_GRAVITY * dt).max(NPC_MAX_FALL);
        let y_step = Vec3::new(0.0, npc.vertical_velocity * dt, 0.0);
        if !collides_npc(&world.chunks, pos + y_step) {
            pos += y_step;
        } else if npc.vertical_velocity < 0.0 {
            npc.vertical_velocity = 0.0;
        }

        let horiz_speed = Vec2::new(pos.x - transform.translation.x, pos.z - transform.translation.z)
            .length()
            / dt.max(0.0001);
        let moved = Vec2::new(pos.x - npc.last_pos.x, pos.z - npc.last_pos.z).length();
        if moved < 0.018 {
            npc.stuck_timer += dt;
        } else {
            npc.stuck_timer = 0.0;
            npc.last_pos = pos;
        }
        if npc.stuck_timer > 0.9 {
            npc.heading += (next_rand(&mut npc.rng) - 0.5) * 3.4;
            npc.stuck_timer = 0.0;
        }

        transform.translation = pos;
        if npc.kind == NpcKind::Friendly {
            let clamped = clamp_to_home(pos, npc.home_center, npc.home_radius + 1.0);
            if clamped != pos {
                transform.translation = clamped;
            }
        }
        let yaw = -npc.heading
            + std::f32::consts::FRAC_PI_2
            + if rig.quadruped { std::f32::consts::PI } else { 0.0 };
        transform.rotation = Quat::from_rotation_y(yaw);

        let stride = (horiz_speed / 2.2).clamp(0.0, 1.0);
        let phase = (time.elapsed_seconds() * (6.0 + stride * 5.0)) + (npc.rng as f32 * 0.0001);
        let swing = phase.sin() * 0.20 * stride;

        rig_updates.push((rig.left_leg, rig.right_leg, rig.left_arm, rig.right_arm, swing, rig.quadruped));
    }

    for (left_leg, right_leg, left_arm, right_arm, swing, quadruped) in rig_updates {
        let leg_swing = if quadruped { swing * 1.25 } else { swing };
        let arm_swing = if quadruped { swing * 1.10 } else { swing * 0.9 };
        if let Ok(mut left) = qset.p1().get_mut(left_leg) {
            left.rotation = Quat::from_rotation_x(leg_swing);
        }
        if let Ok(mut right) = qset.p1().get_mut(right_leg) {
            right.rotation = Quat::from_rotation_x(-leg_swing);
        }
        if let Ok(mut left) = qset.p1().get_mut(left_arm) {
            left.rotation = Quat::from_rotation_x(-arm_swing);
        }
        if let Ok(mut right) = qset.p1().get_mut(right_arm) {
            right.rotation = Quat::from_rotation_x(arm_swing);
        }
    }
}

pub fn draw_npc_debug_gizmos(
    overlay: Res<DebugOverlayState>,
    world: Res<VoxelWorld>,
    cam_q: Query<&Transform, With<FlyCam>>,
    npc_q: Query<(&Transform, &Npc)>,
    mut gizmos: Gizmos,
) {
    if !overlay.visible {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };
    let cam_pos = cam.translation;

    let mut nearest: Option<(f32, Vec3, &Npc)> = None;
    for (t, npc) in &npc_q {
        let d = cam_pos.distance(t.translation);
        if nearest.map(|n| d < n.0).unwrap_or(true) {
            nearest = Some((d, t.translation, npc));
        }
    }
    let Some((_dist, npc_pos, npc)) = nearest else {
        return;
    };

    let body_center = npc_pos + Vec3::new(0.0, NPC_HEIGHT * 0.5, 0.0);
    let eye = npc_pos + Vec3::new(0.0, 1.45, 0.0);
    let heading = Vec3::new(npc.heading.cos(), 0.0, npc.heading.sin()).normalize_or_zero();
    let heading_color = if npc.stuck_timer > 0.2 {
        Color::srgb(1.0, 0.55, 0.18)
    } else {
        Color::srgb(0.16, 0.92, 0.36)
    };

    gizmos.cuboid(
        Transform::from_translation(body_center).with_scale(Vec3::new(0.72, NPC_HEIGHT, 0.72)),
        Color::srgba(1.0, 1.0, 1.0, 0.38),
    );
    gizmos.line(eye, eye + heading * 3.2, heading_color);

    let target = match npc.kind {
        NpcKind::Friendly => {
            if npc.follow_player {
                Some(cam_pos)
            } else {
                None
            }
        }
        NpcKind::Hostile => {
            if npc.last_seen_timer > 0.05 {
                Some(npc.last_seen_player)
            } else if npc.investigate_timer > 0.05 {
                Some(npc.investigate_target)
            } else {
                None
            }
        }
    };

    if let Some(target_pos) = target {
        let marker = target_pos + Vec3::new(0.0, 0.6, 0.0);
        gizmos.line(eye, marker, Color::srgb(0.24, 0.62, 1.0));
        gizmos.cuboid(
            Transform::from_translation(marker).with_scale(Vec3::splat(0.35)),
            Color::srgba(0.24, 0.62, 1.0, 0.85),
        );
    }

    if npc.kind == NpcKind::Hostile {
        let sees = can_see_target(
            npc_pos,
            npc.heading,
            cam_pos,
            HOSTILE_VISION_RANGE,
            HOSTILE_VISION_DOT,
            &world.chunks,
        );
        let los_color = if sees {
            Color::srgb(0.95, 0.10, 0.10)
        } else if line_of_sight_clear(eye, cam_pos + Vec3::new(0.0, 1.3, 0.0), &world.chunks) {
            Color::srgb(0.94, 0.84, 0.18)
        } else {
            Color::srgb(0.45, 0.45, 0.45)
        };
        gizmos.line(eye, cam_pos + Vec3::new(0.0, 1.3, 0.0), los_color);
    }
}

fn find_spawn_ground(chunks: &HashMap<IVec2, Chunk>, x: i32, z: i32) -> Option<i32> {
    for y in (2..(WORLD_HEIGHT as i32 - 3)).rev() {
        let ground = get_block_world(chunks, x, y, z);
        if !is_solid(ground) {
            continue;
        }
        if y <= SEA_LEVEL {
            return None;
        }
        let a1 = get_block_world(chunks, x, y + 1, z);
        let a2 = get_block_world(chunks, x, y + 2, z);
        if !is_solid(a1) && !is_solid(a2) {
            return Some(y + 1);
        }
    }
    None
}

#[inline]
fn has_support(chunks: &HashMap<IVec2, Chunk>, x: i32, z: i32, y: i32) -> bool {
    is_solid(get_block_world(chunks, x, y, z)) || is_solid(get_block_world(chunks, x, y - 1, z))
}

fn enters_water(chunks: &HashMap<IVec2, Chunk>, x: i32, z: i32) -> bool {
    match find_spawn_ground(chunks, x, z) {
        Some(ground_y) => ground_y <= WATER_AVOID_LEVEL + 1,
        None => true,
    }
}

fn clamp_to_home(pos: Vec3, center: Vec2, radius: f32) -> Vec3 {
    let mut p = pos;
    let d = p.xz() - center;
    let len = d.length();
    if len > radius && len > 0.001 {
        let on_edge = center + d / len * radius;
        p.x = on_edge.x;
        p.z = on_edge.y;
    }
    p
}

fn collides_npc(chunks: &HashMap<IVec2, Chunk>, feet: Vec3) -> bool {
    let min = Vec3::new(feet.x - NPC_RADIUS, feet.y, feet.z - NPC_RADIUS);
    let max = Vec3::new(feet.x + NPC_RADIUS, feet.y + NPC_HEIGHT, feet.z + NPC_RADIUS);

    let min_x = min.x.floor() as i32;
    let max_x = max.x.floor() as i32;
    let min_y = min.y.floor() as i32;
    let max_y = max.y.floor() as i32;
    let min_z = min.z.floor() as i32;
    let max_z = max.z.floor() as i32;

    for y in min_y..=max_y {
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if is_solid(get_block_world(chunks, x, y, z)) {
                    return true;
                }
            }
        }
    }

    false
}

fn try_step_up_npc(current: Vec3, horizontal_delta: Vec3, chunks: &HashMap<IVec2, Chunk>) -> Option<Vec3> {
    for step_h in [NPC_STEP_HEIGHT, NPC_STEP_HEIGHT + 0.38] {
        let raised = current + Vec3::Y * step_h;
        if collides_npc(chunks, raised) {
            continue;
        }

        let moved = raised + horizontal_delta;
        if collides_npc(chunks, moved) {
            continue;
        }
        if enters_water(chunks, moved.x.floor() as i32, moved.z.floor() as i32) {
            continue;
        }

        let mut snapped = moved;
        let drop_step = 0.10;
        let mut dropped = 0.0;
        while dropped < step_h + 0.20 {
            let next = snapped - Vec3::Y * drop_step;
            if collides_npc(chunks, next) {
                break;
            }
            snapped = next;
            dropped += drop_step;
        }
        return Some(snapped);
    }
    None
}

fn choose_walk_heading(
    current_heading: f32,
    desired_heading: f32,
    pos: Vec3,
    speed: f32,
    dt: f32,
    chunks: &HashMap<IVec2, Chunk>,
) -> f32 {
    let probe_dist = (speed * dt * 2.0 + 0.95).clamp(0.9, 1.8);
    let mut best = desired_heading;
    let mut best_score = f32::INFINITY;

    for off in [0.0, 0.28, -0.28, 0.55, -0.55, 0.95, -0.95, 1.35, -1.35] {
        let h = desired_heading + off;
        let dir = Vec2::new(h.cos(), h.sin());
        let probe = pos + Vec3::new(dir.x * probe_dist, 0.0, dir.y * probe_dist);
        let mut score = wrap_angle(h - desired_heading).abs() * 1.8 + wrap_angle(h - current_heading).abs() * 0.45;
        if enters_water(chunks, probe.x.floor() as i32, probe.z.floor() as i32) {
            score += 7.0;
        }
        if collides_npc(chunks, probe) && try_step_up_npc(pos, probe - pos, chunks).is_none() {
            score += 8.0;
        }
        if !has_support(
            chunks,
            probe.x.floor() as i32,
            probe.z.floor() as i32,
            (probe.y - 0.2).floor() as i32,
        ) {
            score += 5.0;
        }
        if score < best_score {
            best_score = score;
            best = h;
        }
    }
    best
}

fn can_see_target(
    npc_pos: Vec3,
    heading: f32,
    target_pos: Vec3,
    max_dist: f32,
    fov_dot: f32,
    chunks: &HashMap<IVec2, Chunk>,
) -> bool {
    let eye = npc_pos + Vec3::new(0.0, 1.45, 0.0);
    let target = target_pos + Vec3::new(0.0, 1.45, 0.0);
    let delta = target - eye;
    let dist = delta.length();
    if dist > max_dist || dist < 0.001 {
        return false;
    }
    let forward = Vec3::new(heading.cos(), 0.0, heading.sin()).normalize_or_zero();
    let facing = forward.dot(delta.normalize_or_zero());
    if facing < fov_dot {
        return false;
    }
    line_of_sight_clear(eye, target, chunks)
}

fn line_of_sight_clear(from: Vec3, to: Vec3, chunks: &HashMap<IVec2, Chunk>) -> bool {
    let delta = to - from;
    let dist = delta.length();
    if dist <= 0.001 {
        return true;
    }
    let dir = delta / dist;
    let mut t = 0.35;
    while t < dist - 0.2 {
        let p = from + dir * t;
        let b = get_block_world(chunks, p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if is_solid(b) {
            return false;
        }
        t += 0.45;
    }
    true
}

#[inline]
fn is_solid(block: Block) -> bool {
    block != Block::Air && block != Block::Leaves
}

#[inline]
fn should_spawn_cell(cell: IVec2, seed: u32) -> bool {
    let h = hash3(cell.x, cell.y, seed ^ 0x736E_7063);
    (h & 0xFF) >= 214
}

fn settlement_anchor_for_position(seed: u32, x: i32, z: i32) -> Option<(Vec2, f32)> {
    let p = Vec2::new(x as f32, z as f32);
    const VILLAGE_CELL: i32 = 80;
    let gx = div_floor(x, VILLAGE_CELL);
    let gz = div_floor(z, VILLAGE_CELL);
    for cz in (gz - 2)..=(gz + 2) {
        for cx in (gx - 2)..=(gx + 2) {
            let h = hash3(cx, cz, seed ^ 0x51AA_92F1);
            let guaranteed_origin = cx == 0 && cz == 0;
            if !guaranteed_origin && (h & 0xFF) < 232 {
                continue;
            }

            let vx =
                cx * VILLAGE_CELL + ((((h >> 8) as i32).rem_euclid(VILLAGE_CELL)) - (VILLAGE_CELL / 2));
            let vz = cz * VILLAGE_CELL
                + ((((h >> 16) as i32).rem_euclid(VILLAGE_CELL)) - (VILLAGE_CELL / 2));
            let center = Vec2::new(vx as f32, vz as f32);
            if p.distance(center) <= VILLAGE_SETTLEMENT_RADIUS {
                return Some((center, VILLAGE_SETTLEMENT_RADIUS));
            }
        }
    }

    None
}

#[inline]
fn random_friendly_line(seed: &mut u32) -> &'static str {
    match ((next_rand(seed) * 6.0) as i32).clamp(0, 5) {
        0 => "Friendly: Nice weather today",
        1 => "Friendly: I can follow if you press E",
        2 => "Friendly: Keep an eye on the red ones",
        3 => "Friendly: This biome feels new",
        4 => "Friendly: Need any help building?",
        _ => "Friendly: Watch out after sunset",
    }
}

#[inline]
fn wrap_angle(a: f32) -> f32 {
    let mut x = a;
    while x > std::f32::consts::PI {
        x -= std::f32::consts::TAU;
    }
    while x < -std::f32::consts::PI {
        x += std::f32::consts::TAU;
    }
    x
}

#[inline]
fn next_rand(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    (x as f32) / (u32::MAX as f32)
}

#[inline]
fn hash3(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(374_761_393)
        ^ (z as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}
