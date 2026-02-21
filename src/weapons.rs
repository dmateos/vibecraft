//! Weapon gameplay systems: gun, bullets, grenades, VFX, and destruction queue.
//! Handles projectile simulation, NPC damage/knockback, block destruction, and
//! bounded explosion/remesh workloads to keep frame pacing stable.
use std::collections::VecDeque;

use bevy::ecs::query::QueryFilter;
use bevy::math::primitives::Cuboid;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;

use crate::block_edit::{self, BlockMutationRequest};
use crate::config::BREAK_REACH;
use crate::generation::PromptInputState;
use crate::interact::BlockInventory;
use crate::net_client::{is_remote_simulation, NetClientState};
use crate::npc::{DeadNpcCells, LoadedNpcs, Npc};
use crate::player::FlyCam;
use crate::world::{get_block_world, Block, VoxelWorld};

const GUN_RANGE: f32 = BREAK_REACH * 2.3;
const BULLET_SPEED: f32 = 74.0;
const BULLET_LIFE: f32 = 1.6;
const GRENADE_SPEED: f32 = 18.5;
const GRENADE_GRAVITY: f32 = -24.0;
const GRENADE_FUSE: f32 = 1.55;
const GRENADE_RADIUS: i32 = 4;
const MUZZLE_FLASH_TIME: f32 = 0.045;
const EXPLOSION_FX_TIME: f32 = 0.34;
const EXPLOSION_EDITS_PER_TICK: usize = 320;
const GUN_DAMAGE: f32 = 34.0;

#[derive(Component)]
pub struct Grenade {
    velocity: Vec3,
    fuse: f32,
}

#[derive(Component)]
pub struct Bullet {
    velocity: Vec3,
    life: f32,
}

#[derive(Component)]
pub struct ViewGun;

#[derive(Component)]
pub struct WeaponVfx;

#[derive(Component)]
pub struct MuzzleFlashFx {
    age: f32,
}

#[derive(Component)]
pub struct ExplosionFx {
    age: f32,
    max_scale: f32,
}

#[derive(Default)]
struct ExplosionJob {
    cells: Vec<IVec3>,
    cursor: usize,
}

#[derive(Resource, Default)]
pub struct ExplosionWorkQueue {
    jobs: VecDeque<ExplosionJob>,
}

#[derive(Resource)]
pub struct WeaponAssets {
    grenade_mesh: Handle<Mesh>,
    grenade_material: Handle<StandardMaterial>,
    bullet_mesh: Handle<Mesh>,
    bullet_material: Handle<StandardMaterial>,
    gun_mesh: Handle<Mesh>,
    gun_body_material: Handle<StandardMaterial>,
    gun_accent_material: Handle<StandardMaterial>,
    flash_mesh: Handle<Mesh>,
    flash_material: Handle<StandardMaterial>,
    explosion_mesh: Handle<Mesh>,
    explosion_material: Handle<StandardMaterial>,
}

pub fn setup_weapons(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let grenade_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::splat(0.22))));
    let grenade_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.20, 0.16),
        perceptual_roughness: 0.92,
        metallic: 0.0,
        ..default()
    });

    let bullet_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::splat(0.09))));
    let bullet_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.92, 0.58),
        emissive: Color::srgb(0.95, 0.80, 0.36).into(),
        perceptual_roughness: 0.35,
        metallic: 0.0,
        ..default()
    });

    let gun_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let gun_body_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.14, 0.14, 0.16),
        perceptual_roughness: 0.7,
        metallic: 0.25,
        ..default()
    });
    let gun_accent_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.58, 0.14, 0.12),
        emissive: Color::srgb(0.18, 0.05, 0.04).into(),
        perceptual_roughness: 0.65,
        metallic: 0.15,
        ..default()
    });
    let flash_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let flash_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.88, 0.52, 0.90),
        emissive: Color::srgb(1.0, 0.72, 0.34).into(),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let explosion_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let explosion_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.50, 0.18, 0.34),
        emissive: Color::srgb(0.92, 0.44, 0.15).into(),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.insert_resource(WeaponAssets {
        grenade_mesh,
        grenade_material,
        bullet_mesh,
        bullet_material,
        gun_mesh,
        gun_body_material,
        gun_accent_material,
        flash_mesh,
        flash_material,
        explosion_mesh,
        explosion_material,
    });
    commands.insert_resource(ExplosionWorkQueue::default());
}

pub fn ensure_view_gun(
    mut commands: Commands,
    assets: Res<WeaponAssets>,
    cam_q: Query<Entity, With<FlyCam>>,
    gun_q: Query<Entity, With<ViewGun>>,
) {
    if !gun_q.is_empty() {
        return;
    }

    let Ok(cam_entity) = cam_q.get_single() else {
        return;
    };

    let gun_root = commands
        .spawn((
            SpatialBundle {
                transform: Transform {
                    translation: Vec3::new(0.34, -0.28, -0.55),
                    rotation: Quat::from_euler(EulerRot::XYZ, -0.18, -0.24, -0.03),
                    ..default()
                },
                ..default()
            },
            ViewGun,
            NotShadowCaster,
        ))
        .id();

    let body = commands
        .spawn((
            PbrBundle {
                mesh: assets.gun_mesh.clone(),
                material: assets.gun_body_material.clone(),
                transform: Transform {
                    translation: Vec3::new(0.0, 0.0, 0.0),
                    scale: Vec3::new(0.22, 0.14, 0.74),
                    ..default()
                },
                ..default()
            },
            NotShadowCaster,
        ))
        .id();

    let barrel = commands
        .spawn((
            PbrBundle {
                mesh: assets.gun_mesh.clone(),
                material: assets.gun_accent_material.clone(),
                transform: Transform {
                    translation: Vec3::new(0.0, 0.01, -0.44),
                    scale: Vec3::new(0.09, 0.09, 0.30),
                    ..default()
                },
                ..default()
            },
            NotShadowCaster,
        ))
        .id();

    let grip = commands
        .spawn((
            PbrBundle {
                mesh: assets.gun_mesh.clone(),
                material: assets.gun_body_material.clone(),
                transform: Transform {
                    translation: Vec3::new(0.0, -0.11, 0.10),
                    rotation: Quat::from_euler(EulerRot::XYZ, -0.22, 0.0, 0.0),
                    scale: Vec3::new(0.11, 0.20, 0.18),
                    ..default()
                },
                ..default()
            },
            NotShadowCaster,
        ))
        .id();

    commands
        .entity(gun_root)
        .add_child(body)
        .add_child(barrel)
        .add_child(grip);
    commands.entity(cam_entity).add_child(gun_root);
}

pub fn fire_gun_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    net: Option<Res<NetClientState>>,
    mut cam_q: Query<(&mut Transform, &mut FlyCam)>,
    view_gun_q: Query<Entity, With<ViewGun>>,
    mut commands: Commands,
    assets: Res<WeaponAssets>,
    prompt: Res<PromptInputState>,
) {
    if prompt.active || !keys.just_pressed(KeyCode::KeyE) {
        return;
    }

    let Ok((mut cam_transform, mut cam_ctrl)) = cam_q.get_single_mut() else {
        return;
    };

    let online = net
        .as_ref()
        .map(|n| n.cfg.enabled && n.connected)
        .unwrap_or(false);
    if !online {
        let forward = *cam_transform.forward();
        let spawn = cam_transform.translation + forward * 0.85 + Vec3::new(0.0, -0.10, 0.0);
        let velocity = forward * BULLET_SPEED;

        commands.spawn((
            PbrBundle {
                mesh: assets.bullet_mesh.clone(),
                material: assets.bullet_material.clone(),
                transform: Transform::from_translation(spawn),
                ..default()
            },
            Bullet {
                velocity,
                life: BULLET_LIFE,
            },
            NotShadowCaster,
        ));
    }

    // Recoil kick.
    cam_ctrl.pitch = (cam_ctrl.pitch + 0.030).clamp(-1.54, 1.54);
    cam_ctrl.yaw += 0.0045;
    let yaw_rot = Quat::from_axis_angle(Vec3::Y, cam_ctrl.yaw);
    let pitch_rot = Quat::from_axis_angle(Vec3::X, cam_ctrl.pitch);
    cam_transform.rotation = yaw_rot * pitch_rot;

    // Short muzzle flash attached to the view-gun.
    if let Ok(gun_entity) = view_gun_q.get_single() {
        let flash = commands
            .spawn((
                PbrBundle {
                    mesh: assets.flash_mesh.clone(),
                    material: assets.flash_material.clone(),
                    transform: Transform {
                        translation: Vec3::new(0.0, 0.01, -0.62),
                        scale: Vec3::new(0.10, 0.10, 0.18),
                        ..default()
                    },
                    ..default()
                },
                MuzzleFlashFx { age: 0.0 },
                WeaponVfx,
                NotShadowCaster,
            ))
            .id();
        commands.entity(gun_entity).add_child(flash);
    }
}

pub fn tick_bullets(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Bullet), Without<Npc>>,
    world: Res<VoxelWorld>,
    _loaded_npcs: ResMut<LoadedNpcs>,
    mut inv: ResMut<BlockInventory>,
    mut dead_cells: ResMut<DeadNpcCells>,
    mut block_edits: EventWriter<BlockMutationRequest>,
    mut npc_q: ParamSet<(
        Query<(Entity, &Transform, &Npc), Without<Bullet>>,
        Query<(&mut Npc, &mut Transform), Without<Bullet>>,
    )>,
) {
    let dt = time.delta_seconds();

    for (entity, mut transform, mut bullet) in &mut q {
        bullet.life -= dt;
        if bullet.life <= 0.0 {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        let start = transform.translation;
        let end = start + bullet.velocity * dt;
        let npc_hit = raycast_npc(start, end, &npc_q.p0());
        let block_hit = raycast_block(start, end, &world.chunks);

        let npc_dist = npc_hit.map(|(_, d)| d).unwrap_or(f32::INFINITY);
        let block_dist = block_hit.map(|(_, d)| d).unwrap_or(f32::INFINITY);

        if npc_dist < block_dist {
            if let Some((npc_entity, _)) = npc_hit {
                if let Ok((mut npc, mut npc_transform)) = npc_q.p1().get_mut(npc_entity) {
                    let bullet_dir = bullet.velocity.normalize_or_zero();
                    npc.knockback_velocity += bullet_dir * 6.8 + Vec3::Y * 1.8;
                    npc.hurt_stun = npc.hurt_stun.max(0.22);
                    npc_transform.translation += bullet_dir * 0.20 + Vec3::Y * 0.04;
                    npc.health -= GUN_DAMAGE;
                    if npc.health <= 0.0 && !npc.dead {
                        dead_cells.mark_killed(npc.cell);
                        npc.dead = true;
                        npc.health = 0.0;
                        npc.follow_player = false;
                        npc.attack_cooldown = 9999.0;
                        npc.chat_cooldown = 9999.0;
                        npc.knockback_velocity += bullet_dir * 4.4 + Vec3::Y * 1.2;
                        npc_transform.translation += bullet_dir * 0.46;
                        npc_transform.translation.y += 0.10;
                        let yaw = -npc.heading + std::f32::consts::FRAC_PI_2;
                        npc_transform.rotation = Quat::from_euler(EulerRot::XYZ, 0.0, yaw, 1.20);
                    }
                }
                commands.entity(entity).despawn_recursive();
                continue;
            }
        }

        if let Some((hit, _)) = block_hit {
            let broken = get_block_world(&world.chunks, hit.x, hit.y, hit.z);
            if broken != Block::Air {
                inv.add(broken, 1);
                block_edit::enqueue_block_mutation(
                    &mut block_edits,
                    hit.x,
                    hit.y,
                    hit.z,
                    Block::Air,
                    true,
                );
            }
            commands.entity(entity).despawn_recursive();
            continue;
        }

        transform.translation = end;
    }
}

pub fn throw_grenade_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    net: Option<Res<NetClientState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut commands: Commands,
    assets: Res<WeaponAssets>,
    prompt: Res<PromptInputState>,
) {
    if is_remote_simulation(net.as_deref()) {
        // In network mode, server is authoritative for grenade explosions.
        return;
    }
    if prompt.active || !keys.just_pressed(KeyCode::KeyQ) {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let forward = *cam.forward();
    let spawn = cam.translation + forward * 0.8 + Vec3::Y * -0.10;
    let velocity = forward * GRENADE_SPEED + Vec3::Y * 3.4;

    commands.spawn((
        PbrBundle {
            mesh: assets.grenade_mesh.clone(),
            material: assets.grenade_material.clone(),
            transform: Transform::from_translation(spawn),
            ..default()
        },
        Grenade {
            velocity,
            fuse: GRENADE_FUSE,
        },
    ));
}

pub fn tick_grenades(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Grenade)>,
    assets: Res<WeaponAssets>,
    mut work: ResMut<ExplosionWorkQueue>,
    world: Res<VoxelWorld>,
) {
    let dt = time.delta_seconds();

    for (entity, mut transform, mut grenade) in &mut q {
        grenade.fuse -= dt;

        grenade.velocity.y += GRENADE_GRAVITY * dt;
        let next = transform.translation + grenade.velocity * dt;

        let cell = IVec3::new(
            next.x.floor() as i32,
            next.y.floor() as i32,
            next.z.floor() as i32,
        );
        let hit_solid = get_block_world(&world.chunks, cell.x, cell.y, cell.z) != Block::Air;

        if hit_solid {
            grenade.fuse = grenade.fuse.min(0.06);
            grenade.velocity *= 0.25;
        } else {
            transform.translation = next;
        }

        if grenade.fuse > 0.0 {
            continue;
        }

        let center = IVec3::new(
            transform.translation.x.floor() as i32,
            transform.translation.y.floor() as i32,
            transform.translation.z.floor() as i32,
        );
        enqueue_explosion(&mut work, center, GRENADE_RADIUS);
        spawn_explosion_fx(
            &mut commands,
            &assets,
            transform.translation,
            GRENADE_RADIUS as f32,
        );

        commands.entity(entity).despawn_recursive();
    }
}

pub fn process_explosion_jobs(
    mut work: ResMut<ExplosionWorkQueue>,
    mut block_edits: EventWriter<BlockMutationRequest>,
) {
    let mut budget = EXPLOSION_EDITS_PER_TICK;

    while budget > 0 {
        let Some(mut job) = work.jobs.pop_front() else {
            break;
        };

        while budget > 0 && job.cursor < job.cells.len() {
            let cell = job.cells[job.cursor];
            job.cursor += 1;
            budget -= 1;

            block_edit::enqueue_block_mutation(
                &mut block_edits,
                cell.x,
                cell.y,
                cell.z,
                Block::Air,
                true,
            );
        }

        if job.cursor < job.cells.len() {
            work.jobs.push_front(job);
            break;
        }
    }
}

pub fn clear_explosion_work_queue(work: &mut ExplosionWorkQueue) {
    work.jobs.clear();
}

pub fn tick_weapon_vfx(
    time: Res<Time>,
    mut commands: Commands,
    mut flash_q: Query<(Entity, &mut MuzzleFlashFx)>,
    mut boom_q: Query<(Entity, &mut ExplosionFx, &mut Transform)>,
) {
    let dt = time.delta_seconds();

    for (entity, mut fx) in &mut flash_q {
        fx.age += dt;
        if fx.age >= MUZZLE_FLASH_TIME {
            commands.entity(entity).despawn_recursive();
        }
    }

    for (entity, mut fx, mut transform) in &mut boom_q {
        fx.age += dt;
        let t = (fx.age / EXPLOSION_FX_TIME).clamp(0.0, 1.0);
        let s = 0.4 + fx.max_scale * (t * (2.0 - t));
        transform.scale = Vec3::splat(s);
        if fx.age >= EXPLOSION_FX_TIME {
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn enqueue_explosion(work: &mut ExplosionWorkQueue, center: IVec3, radius: i32) {
    let r2 = radius * radius;
    let mut cells = Vec::new();

    for z in center.z - radius..=center.z + radius {
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                let dx = x - center.x;
                let dy = y - center.y;
                let dz = z - center.z;
                if dx * dx + dy * dy + dz * dz > r2 {
                    continue;
                }
                cells.push(IVec3::new(x, y, z));
            }
        }
    }

    work.jobs.push_back(ExplosionJob { cells, cursor: 0 });
}

fn spawn_explosion_fx(commands: &mut Commands, assets: &WeaponAssets, at: Vec3, radius: f32) {
    commands.spawn((
        PbrBundle {
            mesh: assets.explosion_mesh.clone(),
            material: assets.explosion_material.clone(),
            transform: Transform {
                translation: at + Vec3::Y * 0.2,
                scale: Vec3::splat(0.5),
                ..default()
            },
            ..default()
        },
        ExplosionFx {
            age: 0.0,
            max_scale: radius * 1.7,
        },
        WeaponVfx,
        NotShadowCaster,
    ));
}

fn raycast_block(
    start: Vec3,
    end: Vec3,
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
) -> Option<(IVec3, f32)> {
    let delta = end - start;
    let dist = delta.length().max(0.0001);
    let dir = delta / dist;
    let step = 0.05;
    let mut t = 0.0;
    let max_dist = dist.min(GUN_RANGE);
    let mut last = IVec3::new(i32::MIN, i32::MIN, i32::MIN);

    while t <= max_dist {
        let p = start + dir * t;
        let cell = IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if cell == last {
            t += step;
            continue;
        }
        last = cell;

        if get_block_world(chunks, cell.x, cell.y, cell.z) != Block::Air {
            return Some((cell, t));
        }
        t += step;
    }

    None
}

fn raycast_npc<F: QueryFilter>(
    start: Vec3,
    end: Vec3,
    npcs: &Query<(Entity, &Transform, &Npc), F>,
) -> Option<(Entity, f32)> {
    let mut best: Option<(Entity, f32)> = None;
    let seg = end - start;
    let seg_len_sq = seg.length_squared().max(0.0001);
    let seg_len = seg_len_sq.sqrt();

    for (entity, transform, npc) in npcs.iter() {
        if npc.health <= 0.0 {
            continue;
        }

        let feet = transform.translation;
        let center = feet + Vec3::new(0.0, 0.95, 0.0);
        let t = ((center - start).dot(seg) / seg_len_sq).clamp(0.0, 1.0);
        let p = start + seg * t;

        if p.y < feet.y - 0.12 || p.y > feet.y + 1.95 {
            continue;
        }
        let horiz = (p.xz() - feet.xz()).length();
        if horiz > 0.58 {
            continue;
        }

        let hit_dist = seg_len * t;
        if best.map(|(_, d)| hit_dist < d).unwrap_or(true) {
            best = Some((entity, hit_dist));
        }
    }

    best
}
