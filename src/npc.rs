use std::collections::{HashMap, HashSet};

use bevy::math::primitives::Cuboid;
use bevy::prelude::*;

use crate::config::{CHUNK_SIZE, SEA_LEVEL, WORLD_HEIGHT};
use crate::generation::PromptInputState;
use crate::player::FlyCam;
use crate::world::{chunk_distance_sq, div_floor, get_block_world, Block, Chunk, VoxelWorld};

const NPC_HEIGHT: f32 = 1.72;
const NPC_RADIUS: f32 = 0.28;
const NPC_GRAVITY: f32 = -22.0;
const NPC_MAX_FALL: f32 = -30.0;
const NPC_SPAWN_RADIUS_CHUNKS: i32 = 7;
const NPC_DESPAWN_RADIUS_CHUNKS: i32 = 10;
const NPC_MAX_COUNT: usize = 28;
const NPC_MAX_SPAWNS_PER_TICK: usize = 4;
const NPC_CELL_SIZE: i32 = 18;
const FRIENDLY_INTERACT_RANGE: f32 = 4.8;
const HOSTILE_AGGRO_RANGE: f32 = 18.0;
const HOSTILE_ATTACK_RANGE: f32 = 1.45;
const WATER_AVOID_LEVEL: i32 = SEA_LEVEL;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpcKind {
    Friendly,
    Hostile,
}

#[derive(Component)]
pub struct Npc {
    pub kind: NpcKind,
    pub heading: f32,
    pub speed: f32,
    pub turn_timer: f32,
    pub vertical_velocity: f32,
    pub rng: u32,
    pub follow_player: bool,
    pub attack_cooldown: f32,
    pub chat_cooldown: f32,
}

#[derive(Component)]
pub struct NpcRig {
    pub left_leg: Entity,
    pub right_leg: Entity,
}

#[derive(Resource, Default)]
pub struct LoadedNpcs {
    entries: HashMap<IVec2, Entity>,
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
    world: Res<VoxelWorld>,
    assets: Res<NpcAssets>,
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
        if (out_of_range || !desired_cells.contains(&cell))
            && let Some(entity) = loaded.entries.remove(&cell)
        {
            commands.entity(entity).despawn_recursive();
        }
    }

    if loaded.entries.len() >= NPC_MAX_COUNT {
        return;
    }

    let mut to_spawn: Vec<IVec2> = desired_cells
        .into_iter()
        .filter(|cell| !loaded.entries.contains_key(cell))
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

        let heading = ((seed >> 16) as f32 / u16::MAX as f32) * std::f32::consts::TAU;
        let kind = if ((seed >> 24) & 0xFF) > 215 {
            NpcKind::Hostile
        } else {
            NpcKind::Friendly
        };
        let speed = match kind {
            NpcKind::Friendly => 0.85 + ((seed >> 20) as f32 / 255.0) * 0.65,
            NpcKind::Hostile => 1.35 + ((seed >> 20) as f32 / 255.0) * 0.65,
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
                    heading,
                    speed,
                    turn_timer: 0.8 + ((seed >> 10) as f32 / 255.0) * 2.2,
                    vertical_velocity: 0.0,
                    rng: seed ^ 0xD17A_2B4F,
                    follow_player: false,
                    attack_cooldown: 0.0,
                    chat_cooldown: 1.5 + ((seed >> 2) as f32 / 255.0) * 3.0,
                },
            ))
            .id();

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

        commands
            .entity(root)
            .add_child(head)
            .add_child(torso)
            .add_child(left_leg)
            .add_child(right_leg)
            .insert(NpcRig {
                left_leg,
                right_leg,
            });

        loaded.entries.insert(cell, root);
    }
}

pub fn npc_interactions(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    mut npcs: Query<(&Transform, &mut Npc)>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut ui: ResMut<NpcUiState>,
) {
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
    cam_q: Query<&Transform, (With<FlyCam>, Without<Npc>)>,
    mut ui: ResMut<NpcUiState>,
    mut vitals: ResMut<PlayerVitals>,
    mut qset: ParamSet<(
        Query<(&mut Transform, &mut Npc, &NpcRig), Without<FlyCam>>,
        Query<&mut Transform, (Without<Npc>, Without<FlyCam>)>,
    )>,
) {
    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let dt = time.delta_seconds();
    let player_pos = cam.translation;
    let mut leg_updates: Vec<(Entity, Entity, f32)> = Vec::new();

    if ui.ttl > 0.0 {
        ui.ttl = (ui.ttl - dt).max(0.0);
        if ui.ttl <= 0.0 {
            ui.message = "No NPC nearby".to_string();
        }
    }

    for (mut transform, mut npc, rig) in &mut qset.p0() {
        npc.attack_cooldown = (npc.attack_cooldown - dt).max(0.0);
        npc.chat_cooldown = (npc.chat_cooldown - dt).max(0.0);

        let to_player = player_pos - transform.translation;
        let player_dist = to_player.length();

        let mut desired_dir = Vec2::new(npc.heading.cos(), npc.heading.sin());
        match npc.kind {
            NpcKind::Friendly => {
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
                if player_dist <= HOSTILE_AGGRO_RANGE {
                    desired_dir = to_player.xz().normalize_or_zero();
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

        if desired_dir.length_squared() > 0.001 {
            let target_heading = desired_dir.y.atan2(desired_dir.x);
            let delta = wrap_angle(target_heading - npc.heading);
            npc.heading += delta.clamp(-2.2 * dt, 2.2 * dt);
        }

        let dir = Vec2::new(npc.heading.cos(), npc.heading.sin());
        let ahead = transform.translation + Vec3::new(dir.x * 0.9, -0.05, dir.y * 0.9);
        if !has_support(&world.chunks, ahead.x.floor() as i32, ahead.z.floor() as i32, ahead.y.floor() as i32)
            || enters_water(&world.chunks, ahead.x.floor() as i32, ahead.z.floor() as i32)
            || collides_npc(
                &world.chunks,
                transform.translation + Vec3::new(dir.x * npc.speed * dt, 0.0, dir.y * npc.speed * dt),
            )
        {
            npc.heading += (next_rand(&mut npc.rng) - 0.5) * 2.6;
        }

        let dir = Vec2::new(npc.heading.cos(), npc.heading.sin());
        let mut pos = transform.translation;

        let x_step = Vec3::new(dir.x * npc.speed * dt, 0.0, 0.0);
        if !collides_npc(&world.chunks, pos + x_step)
            && !enters_water(
                &world.chunks,
                (pos.x + x_step.x).floor() as i32,
                pos.z.floor() as i32,
            )
        {
            pos += x_step;
        }

        let z_step = Vec3::new(0.0, 0.0, dir.y * npc.speed * dt);
        if !collides_npc(&world.chunks, pos + z_step)
            && !enters_water(
                &world.chunks,
                pos.x.floor() as i32,
                (pos.z + z_step.z).floor() as i32,
            )
        {
            pos += z_step;
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

        transform.translation = pos;
        transform.rotation = Quat::from_rotation_y(-npc.heading + std::f32::consts::FRAC_PI_2);

        let stride = (horiz_speed / 2.2).clamp(0.0, 1.0);
        let phase = (time.elapsed_seconds() * (6.0 + stride * 5.0)) + (npc.rng as f32 * 0.0001);
        let swing = phase.sin() * 0.20 * stride;

        leg_updates.push((rig.left_leg, rig.right_leg, swing));
    }

    for (left_leg, right_leg, swing) in leg_updates {
        if let Ok(mut left) = qset.p1().get_mut(left_leg) {
            left.rotation = Quat::from_rotation_x(swing);
        }
        if let Ok(mut right) = qset.p1().get_mut(right_leg) {
            right.rotation = Quat::from_rotation_x(-swing);
        }
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

#[inline]
fn is_solid(block: Block) -> bool {
    block != Block::Air && block != Block::Leaves
}

#[inline]
fn should_spawn_cell(cell: IVec2, seed: u32) -> bool {
    let h = hash3(cell.x, cell.y, seed ^ 0x736E_7063);
    (h & 0xFF) >= 182
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
