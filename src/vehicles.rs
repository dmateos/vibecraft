//! Enterable arcade vehicles and landmark-based vehicle yard spawns.
//! Keeps the camera as the player/view source while mounting it onto
//! vehicle entities so existing interaction/streaming systems continue to work.
use std::collections::HashSet;

use bevy::math::primitives::Cuboid;
use bevy::prelude::*;

use crate::config::{SEA_LEVEL, WORLD_HEIGHT};
use crate::generation::PromptInputState;
use crate::physics::{self, CollisionAabb};
use crate::player::{self, FlyCam};
use crate::water::{self, WaterFlowSim};
use crate::world::{Block, LoadedChunks, VoxelWorld, get_block_world};

const VEHICLE_INTERACT_RANGE: f32 = 4.0;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleKind {
    Car,
    Boat,
    Helicopter,
    Plane,
}

impl VehicleKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Car => "Car",
            Self::Boat => "Boat",
            Self::Helicopter => "Helicopter",
            Self::Plane => "Plane",
        }
    }

    fn seat_offset(self) -> Vec3 {
        match self {
            Self::Car => Vec3::new(0.0, 1.35, 0.0),
            Self::Boat => Vec3::new(0.0, 1.1, 0.1),
            Self::Helicopter => Vec3::new(0.0, 1.85, 0.0),
            Self::Plane => Vec3::new(0.0, 1.45, -0.1),
        }
    }

    fn hull_extents(self) -> Vec3 {
        match self {
            Self::Car => Vec3::new(1.1, 0.9, 2.0),
            Self::Boat => Vec3::new(1.3, 0.7, 2.2),
            Self::Helicopter => Vec3::new(1.6, 1.4, 2.2),
            Self::Plane => Vec3::new(2.0, 0.8, 3.0),
        }
    }
}

#[derive(Component)]
pub struct Vehicle {
    pub kind: VehicleKind,
}

#[derive(Component, Default)]
pub struct VehicleMotion {
    pub velocity: Vec3,
    pub throttle: f32,
    pub yaw: f32,
}

#[derive(Component, Default)]
pub struct VehicleOccupant {
    pub driver: Option<Entity>,
}

#[derive(Component)]
struct VehicleYardMarker {
    _marker: IVec3,
}

#[derive(Resource, Default)]
pub struct VehicleRiderState {
    pub mounted_vehicle: Option<Entity>,
}

impl VehicleRiderState {
    pub fn is_mounted(&self) -> bool {
        self.mounted_vehicle.is_some()
    }
}

#[derive(Resource, Default)]
pub struct VehicleYardSpawnState {
    scanned_chunks: HashSet<IVec2>,
    spawned_markers: HashSet<IVec3>,
    world_seed_seen: Option<u32>,
    focused_player_on_first_yard: bool,
}

#[derive(Resource)]
pub struct VehicleAssets {
    cube_mesh: Handle<Mesh>,
    car_mat: Handle<StandardMaterial>,
    boat_mat: Handle<StandardMaterial>,
    heli_mat: Handle<StandardMaterial>,
    plane_mat: Handle<StandardMaterial>,
    accent_mat: Handle<StandardMaterial>,
}

pub fn setup_vehicle_assets(
    mut commands: Commands,
    existing: Option<Res<VehicleAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if existing.is_some() {
        return;
    }
    let cube_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let car_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.86, 0.22, 0.18),
        ..default()
    });
    let boat_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.33, 0.22, 0.14),
        ..default()
    });
    let heli_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.17, 0.55, 0.22),
        ..default()
    });
    let plane_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.34, 0.82),
        ..default()
    });
    let accent_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.93, 0.87, 0.25),
        emissive: Color::srgb(0.15, 0.13, 0.02).into(),
        ..default()
    });
    commands.insert_resource(VehicleAssets {
        cube_mesh,
        car_mat,
        boat_mat,
        heli_mat,
        plane_mat,
        accent_mat,
    });
}

pub fn sync_vehicle_seed_reset(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    mut spawn_state: ResMut<VehicleYardSpawnState>,
    vehicle_q: Query<Entity, With<Vehicle>>,
    mut rider: ResMut<VehicleRiderState>,
) {
    if spawn_state.world_seed_seen == Some(world.seed) {
        return;
    }
    spawn_state.world_seed_seen = Some(world.seed);
    spawn_state.scanned_chunks.clear();
    spawn_state.spawned_markers.clear();
    rider.mounted_vehicle = None;
    spawn_state.focused_player_on_first_yard = false;
    for e in &vehicle_q {
        commands.entity(e).despawn_recursive();
    }
}

pub fn spawn_vehicle_yards_from_landmarks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    assets: Option<Res<VehicleAssets>>,
    mut state: ResMut<VehicleYardSpawnState>,
    mut cam_q: Query<&mut Transform, With<FlyCam>>,
) {
    let Some(assets) = assets else {
        return;
    };

    let new_chunks: Vec<IVec2> = loaded
        .entries
        .keys()
        .copied()
        .filter(|c| !state.scanned_chunks.contains(c))
        .collect();
    if new_chunks.is_empty() {
        return;
    }

    for chunk_pos in new_chunks {
        state.scanned_chunks.insert(chunk_pos);
        let base_x = chunk_pos.x * crate::config::CHUNK_SIZE as i32;
        let base_z = chunk_pos.y * crate::config::CHUNK_SIZE as i32;
        for lz in 0..crate::config::CHUNK_SIZE as i32 {
            for lx in 0..crate::config::CHUNK_SIZE as i32 {
                let wx = base_x + lx;
                let wz = base_z + lz;
                let Some(marker_base) = detect_vehicle_yard_marker(&world.chunks, wx, wz) else {
                    continue;
                };
                if !state.spawned_markers.insert(marker_base) {
                    continue;
                }
                spawn_yard_vehicles(&mut commands, &assets, &world.chunks, marker_base);
                if !state.focused_player_on_first_yard {
                    if let Ok(mut cam) = cam_q.get_single_mut() {
                        let yard_center = Vec3::new(
                            marker_base.x as f32 + 0.5,
                            marker_base.y as f32 + 0.5,
                            marker_base.z as f32 + 0.5,
                        );
                        cam.translation = yard_center + Vec3::new(0.0, 4.5, -12.0);
                        cam.look_at(yard_center + Vec3::Y * 1.5, Vec3::Y);
                        state.focused_player_on_first_yard = true;
                    }
                }
            }
        }
    }
}

pub fn handle_vehicle_mount_input(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    mut rider: ResMut<VehicleRiderState>,
    world: Res<VoxelWorld>,
    mut cam_q: Query<(Entity, &mut Transform), With<FlyCam>>,
    mut vehicles: Query<(Entity, &Transform, &Vehicle, &mut VehicleOccupant), Without<FlyCam>>,
) {
    if prompt.active || !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Ok((cam_entity, mut cam_transform)) = cam_q.get_single_mut() else {
        return;
    };

    if let Some(mounted) = rider.mounted_vehicle {
        if let Ok((_veh_e, veh_t, veh, mut occ)) = vehicles.get_mut(mounted) {
            let right = veh_t.rotation * Vec3::X;
            let candidates = [
                veh_t.translation + right * 2.2 + Vec3::Y * 0.1,
                veh_t.translation - right * 2.2 + Vec3::Y * 0.1,
                veh_t.translation + veh_t.rotation * Vec3::Z * 2.6 + Vec3::Y * 0.1,
            ];
            for feet in candidates {
                let eye = feet + Vec3::Y * crate::config::EYE_HEIGHT;
                if !player::collides_player(eye, &world.chunks) {
                    cam_transform.translation = eye;
                    occ.driver = None;
                    rider.mounted_vehicle = None;
                    info!("dismounted {}", veh.kind.label());
                    return;
                }
            }
            warn!("no safe dismount spot near vehicle");
        } else {
            rider.mounted_vehicle = None;
        }
        return;
    }

    let cam_pos = cam_transform.translation;
    let mut best: Option<(Entity, f32)> = None;
    for (e, vt, veh, occ) in &mut vehicles {
        if veh.kind != VehicleKind::Helicopter {
            continue;
        }
        if occ.driver.is_some() {
            continue;
        }
        let d2 = vt.translation.distance_squared(cam_pos);
        if d2 > VEHICLE_INTERACT_RANGE * VEHICLE_INTERACT_RANGE {
            continue;
        }
        if best.map(|(_, bd2)| d2 < bd2).unwrap_or(true) {
            best = Some((e, d2));
        }
    }

    let Some((target, _)) = best else {
        return;
    };

    if let Ok((_e, _vt, veh, mut occ)) = vehicles.get_mut(target) {
        occ.driver = Some(cam_entity);
        rider.mounted_vehicle = Some(target);
        info!("mounted {} (G to exit)", veh.kind.label());
    }
}

pub fn tick_vehicles_active(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    rider: Res<VehicleRiderState>,
    world: Res<VoxelWorld>,
    _water_sim: Res<WaterFlowSim>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut vehicles: Query<
        (Entity, &mut Transform, &Vehicle, &mut VehicleMotion, &VehicleOccupant),
        Without<FlyCam>,
    >,
) {
    let dt = time.delta_seconds();
    if dt <= 0.0 {
        return;
    }
    let cam = cam_q.get_single().ok();

    for (entity, mut transform, vehicle, mut motion, occ) in &mut vehicles {
        let driven_by_local = rider.mounted_vehicle == Some(entity) && occ.driver.is_some();
        if vehicle.kind != VehicleKind::Helicopter {
            continue;
        }

        if driven_by_local {
            tick_helicopter(&keys, dt, &world, cam, &mut transform, &mut motion);
        } else {
            motion.velocity *= (1.0 - 1.8 * dt).clamp(0.0, 1.0);
            settle_vehicle_on_ground(&world, &mut transform, vehicle.kind);
        }
    }
}

pub fn sync_camera_to_mounted_vehicle(
    rider: Res<VehicleRiderState>,
    mut cam_q: Query<(&mut Transform, &FlyCam), With<FlyCam>>,
    vehicle_q: Query<(&Transform, &Vehicle), Without<FlyCam>>,
) {
    let Some(vehicle_entity) = rider.mounted_vehicle else {
        return;
    };
    let Ok((mut cam, cam_ctrl)) = cam_q.get_single_mut() else {
        return;
    };
    let Ok((veh_t, veh)) = vehicle_q.get(vehicle_entity) else {
        return;
    };
    cam.translation = veh_t.translation + veh_t.rotation * veh.kind.seat_offset();
    // Follow vehicle yaw/roll while keeping local look pitch.
    cam.rotation = veh_t.rotation * Quat::from_axis_angle(Vec3::X, cam_ctrl.pitch);
}

fn tick_car(
    keys: &ButtonInput<KeyCode>,
    dt: f32,
    world: &VoxelWorld,
    transform: &mut Transform,
    motion: &mut VehicleMotion,
) {
    let steer = (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
    let accel = (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32;
    let brake = keys.pressed(KeyCode::Space);

    let yaw_rate = 1.7 + motion.velocity.length().min(12.0) * 0.05;
    motion.yaw -= steer * yaw_rate * dt;
    let rot = Quat::from_rotation_y(motion.yaw);
    let forward = rot * -Vec3::Z;

    let drive_accel = 26.0 * accel;
    motion.velocity += forward * drive_accel * dt;
    motion.velocity.y = 0.0;
    let drag = if brake { 7.5 } else { 2.8 };
    motion.velocity *= (1.0 - drag * dt).clamp(0.0, 1.0);
    let max_speed = if brake { 7.0 } else { 22.0 };
    let speed = motion.velocity.length();
    if speed > max_speed {
        motion.velocity = motion.velocity.normalize() * max_speed;
    }

    let next = transform.translation + motion.velocity * dt;
    if !collides_vehicle(world, next, VehicleKind::Car) {
        transform.translation.x = next.x;
        transform.translation.z = next.z;
    } else {
        motion.velocity *= 0.2;
    }
    settle_vehicle_on_ground(world, transform, VehicleKind::Car);
    transform.rotation = rot;
}

fn tick_boat(
    keys: &ButtonInput<KeyCode>,
    dt: f32,
    world: &VoxelWorld,
    water_sim: &WaterFlowSim,
    transform: &mut Transform,
    motion: &mut VehicleMotion,
) {
    let steer = (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
    let accel = (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32;
    motion.yaw -= steer * 1.2 * dt;
    let rot = Quat::from_rotation_y(motion.yaw);
    let forward = rot * -Vec3::Z;
    motion.velocity += forward * accel * 11.0 * dt;
    motion.velocity.y = 0.0;
    motion.velocity *= (1.0 - 2.1 * dt).clamp(0.0, 1.0);
    let speed = motion.velocity.length();
    if speed > 14.0 {
        motion.velocity = motion.velocity.normalize() * 14.0;
    }
    let next = transform.translation + motion.velocity * dt;
    if !collides_vehicle(world, next, VehicleKind::Boat) {
        transform.translation.x = next.x;
        transform.translation.z = next.z;
    } else {
        motion.velocity *= 0.3;
    }
    if let Some(h) =
        water::sample_water_surface_height(world, water_sim, transform.translation.x, transform.translation.z)
    {
        transform.translation.y = h + 0.7 + (time_sine_hint(transform.translation) * 0.05);
    } else {
        settle_vehicle_on_ground(world, transform, VehicleKind::Boat);
    }
    transform.rotation = rot;
}

fn tick_helicopter(
    keys: &ButtonInput<KeyCode>,
    dt: f32,
    world: &VoxelWorld,
    cam: Option<&Transform>,
    transform: &mut Transform,
    motion: &mut VehicleMotion,
) {
    let steer = (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
    let thrust_fb = (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32;
    let lift = (keys.pressed(KeyCode::Space) as i32 - keys.pressed(KeyCode::ShiftLeft) as i32) as f32;

    motion.yaw -= steer * 1.25 * dt;
    let yaw_rot = Quat::from_rotation_y(motion.yaw);
    let mut forward = yaw_rot * -Vec3::Z;
    if let Some(cam) = cam {
        let mut c = *cam.forward();
        c.y = 0.0;
        if c.length_squared() > 0.01 {
            forward = c.normalize();
        }
    }
    // Helicopter tuning: neutral stick should sink slowly, not drop hard.
    // Space/Shift act as collective up/down around a near-hover baseline.
    let forward_accel = 30.0;
    let collective_accel = 14.0;
    let neutral_lift = 7.6;
    let gravity_pull = 8.2;
    motion.velocity += forward * thrust_fb * forward_accel * dt;
    motion.velocity.y += (neutral_lift + lift * collective_accel - gravity_pull) * dt;
    motion.velocity *= Vec3::new(0.975, 0.992, 0.975);
    if motion.velocity.length() > 32.0 {
        motion.velocity = motion.velocity.normalize() * 32.0;
    }
    let next = transform.translation + motion.velocity * dt;
    if !collides_vehicle(world, next, VehicleKind::Helicopter) {
        transform.translation = next;
    } else {
        motion.velocity *= 0.15;
    }
    if transform.translation.y < SEA_LEVEL as f32 + 1.5 {
        transform.translation.y = SEA_LEVEL as f32 + 1.5;
        motion.velocity.y = motion.velocity.y.max(0.0);
    }
    transform.rotation = yaw_rot;
}

fn tick_plane(
    keys: &ButtonInput<KeyCode>,
    dt: f32,
    world: &VoxelWorld,
    cam: Option<&Transform>,
    transform: &mut Transform,
    motion: &mut VehicleMotion,
) {
    let throttle_axis =
        (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32;
    motion.throttle = (motion.throttle + throttle_axis * dt * 0.7).clamp(0.25, 1.0);
    let steer = (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
    motion.yaw -= steer * 0.8 * dt;

    let yaw_rot = Quat::from_rotation_y(motion.yaw);
    let mut flight_dir = yaw_rot * -Vec3::Z;
    if let Some(cam) = cam {
        let c = *cam.forward();
        if c.length_squared() > 0.01 {
            flight_dir = c.normalize();
            motion.yaw = (-flight_dir.x).atan2(-flight_dir.z);
        }
    }
    let target_speed = 18.0 + motion.throttle * 28.0;
    let current_speed = motion.velocity.length().max(0.1);
    let speed = current_speed + (target_speed - current_speed) * (1.8 * dt).clamp(0.0, 1.0);
    motion.velocity = flight_dir * speed;
    motion.velocity.y += 4.0 * (motion.throttle - 0.45) * dt;
    motion.velocity.y -= 2.2 * dt; // gravity-ish

    let next = transform.translation + motion.velocity * dt;
    if !collides_vehicle(world, next, VehicleKind::Plane) {
        transform.translation = next;
    } else {
        motion.velocity *= 0.1;
    }
    transform.translation.y = transform.translation.y.max(SEA_LEVEL as f32 + 6.0);
    transform.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, motion.velocity.normalize_or_zero());
}

fn time_sine_hint(pos: Vec3) -> f32 {
    (pos.x * 0.21 + pos.z * 0.17).sin()
}

fn settle_vehicle_on_ground(world: &VoxelWorld, transform: &mut Transform, kind: VehicleKind) {
    let x = transform.translation.x.floor() as i32;
    let z = transform.translation.z.floor() as i32;
    let mut top = 0i32;
    for y in (0..WORLD_HEIGHT as i32).rev() {
        let b = get_block_world(&world.chunks, x, y, z);
        if b != Block::Air && b != Block::Leaves {
            top = y;
            break;
        }
    }
    let y = match kind {
        VehicleKind::Car => top as f32 + 1.25,
        VehicleKind::Boat => top as f32 + 1.1,
        VehicleKind::Helicopter => transform
            .translation
            .y
            .max(top as f32 + 1.0 + kind.hull_extents().y + 0.35),
        VehicleKind::Plane => transform.translation.y.max(top as f32 + 3.0),
    };
    transform.translation.y = y;
}

fn collides_vehicle(world: &VoxelWorld, center: Vec3, kind: VehicleKind) -> bool {
    let e = kind.hull_extents();
    let aabb = CollisionAabb {
        min: center - e,
        max: center + e,
    };
    physics::query_collision(&world.chunks, aabb, physics::is_solid_for_npc)
}

fn detect_vehicle_yard_marker(
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    wx: i32,
    wz: i32,
) -> Option<IVec3> {
    let mut top_y = None;
    for y in (0..WORLD_HEIGHT as i32).rev() {
        let b = get_block_world(chunks, wx, y, wz);
        if b != Block::Air {
            top_y = Some(y);
            break;
        }
    }
    let y = top_y?;
    if get_block_world(chunks, wx, y, wz) != Block::Cyan {
        return None;
    }
    if get_block_world(chunks, wx, y - 1, wz) != Block::Purple {
        return None;
    }
    if get_block_world(chunks, wx, y - 2, wz) != Block::Cyan {
        return None;
    }
    Some(IVec3::new(wx, y - 2, wz))
}

fn spawn_yard_vehicles(
    commands: &mut Commands,
    assets: &VehicleAssets,
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    marker: IVec3,
) {
    let center = Vec3::new(marker.x as f32 + 0.5, marker.y as f32 + 0.5, marker.z as f32 + 0.5);
    let layouts = [(VehicleKind::Helicopter, Vec3::new(0.0, 0.0, 0.0), 0.0f32)];

    for (kind, offset, yaw) in layouts {
        let pos = center + offset;
        let mut transform = Transform::from_translation(pos);
        transform.rotation = Quat::from_rotation_y(yaw);
        let mut motion = VehicleMotion::default();
        motion.yaw = yaw;
        let x = pos.x.floor() as i32;
        let z = pos.z.floor() as i32;
        let mut top = marker.y;
        for y in (0..WORLD_HEIGHT as i32).rev() {
            let b = get_block_world(chunks, x, y, z);
            if b != Block::Air && b != Block::Leaves {
                top = y;
                break;
            }
        }
        transform.translation.y = top as f32 + match kind {
            VehicleKind::Car => 1.25,
            VehicleKind::Boat => 1.0,
            VehicleKind::Helicopter => 1.0 + kind.hull_extents().y + 0.35,
            VehicleKind::Plane => 1.3,
        };
        spawn_vehicle(commands, assets, kind, transform, marker, motion);
    }
}

fn spawn_vehicle(
    commands: &mut Commands,
    assets: &VehicleAssets,
    kind: VehicleKind,
    transform: Transform,
    marker: IVec3,
    motion: VehicleMotion,
) {
    let root = commands
        .spawn((
            PbrBundle {
                mesh: assets.cube_mesh.clone(),
                material: match kind {
                    VehicleKind::Car => assets.car_mat.clone(),
                    VehicleKind::Boat => assets.boat_mat.clone(),
                    VehicleKind::Helicopter => assets.heli_mat.clone(),
                    VehicleKind::Plane => assets.plane_mat.clone(),
                },
                transform,
                ..default()
            },
            Vehicle { kind },
            motion,
            VehicleOccupant::default(),
            VehicleYardMarker { _marker: marker },
        ))
        .id();

    let (body_scale, body_offset) = match kind {
        VehicleKind::Car => (Vec3::new(1.6, 0.8, 3.0), Vec3::new(0.0, 0.0, 0.0)),
        VehicleKind::Boat => (Vec3::new(1.8, 0.5, 3.2), Vec3::new(0.0, -0.1, 0.0)),
        VehicleKind::Helicopter => (Vec3::new(1.3, 0.9, 2.2), Vec3::new(0.0, 0.2, 0.0)),
        VehicleKind::Plane => (Vec3::new(1.2, 0.5, 3.8), Vec3::new(0.0, 0.0, 0.0)),
    };
    let body = commands
        .spawn(PbrBundle {
            mesh: assets.cube_mesh.clone(),
            material: match kind {
                VehicleKind::Car => assets.car_mat.clone(),
                VehicleKind::Boat => assets.boat_mat.clone(),
                VehicleKind::Helicopter => assets.heli_mat.clone(),
                VehicleKind::Plane => assets.plane_mat.clone(),
            },
            transform: Transform {
                translation: body_offset,
                scale: body_scale,
                ..default()
            },
            ..default()
        })
        .id();
    commands.entity(root).add_child(body);

    let accent = commands
        .spawn(PbrBundle {
            mesh: assets.cube_mesh.clone(),
            material: assets.accent_mat.clone(),
            transform: Transform {
                translation: match kind {
                    VehicleKind::Car => Vec3::new(0.0, 0.65, -0.15),
                    VehicleKind::Boat => Vec3::new(0.0, 0.45, 0.0),
                    VehicleKind::Helicopter => Vec3::new(0.0, 1.15, 0.0),
                    VehicleKind::Plane => Vec3::new(0.0, 0.45, 0.65),
                },
                scale: match kind {
                    VehicleKind::Car => Vec3::new(1.0, 0.45, 1.2),
                    VehicleKind::Boat => Vec3::new(0.8, 0.35, 1.2),
                    VehicleKind::Helicopter => Vec3::new(0.9, 0.12, 4.8),
                    VehicleKind::Plane => Vec3::new(4.8, 0.12, 0.8),
                },
                ..default()
            },
            ..default()
        })
        .id();
    commands.entity(root).add_child(accent);

    if kind == VehicleKind::Helicopter {
        let tail = commands
            .spawn(PbrBundle {
                mesh: assets.cube_mesh.clone(),
                material: assets.heli_mat.clone(),
                transform: Transform {
                    translation: Vec3::new(0.0, 0.22, 2.4),
                    scale: Vec3::new(0.22, 0.22, 2.8),
                    ..default()
                },
                ..default()
            })
            .id();
        let skids = [
            Vec3::new(-0.75, -0.75, 0.2),
            Vec3::new(0.75, -0.75, 0.2),
        ]
        .map(|p| {
            commands
                .spawn(PbrBundle {
                    mesh: assets.cube_mesh.clone(),
                    material: assets.accent_mat.clone(),
                    transform: Transform {
                        translation: p,
                        scale: Vec3::new(0.10, 0.10, 2.3),
                        ..default()
                    },
                    ..default()
                })
                .id()
        });
        let mast = commands
            .spawn(PbrBundle {
                mesh: assets.cube_mesh.clone(),
                material: assets.accent_mat.clone(),
                transform: Transform {
                    translation: Vec3::new(0.0, 1.15, 0.0),
                    scale: Vec3::new(0.10, 0.65, 0.10),
                    ..default()
                },
                ..default()
            })
            .id();
        let rotor = commands
            .spawn(PbrBundle {
                mesh: assets.cube_mesh.clone(),
                material: assets.accent_mat.clone(),
                transform: Transform {
                    translation: Vec3::new(0.0, 1.55, 0.0),
                    scale: Vec3::new(4.8, 0.06, 0.14),
                    ..default()
                },
                ..default()
            })
            .id();
        commands
            .entity(root)
            .add_child(tail)
            .add_child(skids[0])
            .add_child(skids[1])
            .add_child(mast)
            .add_child(rotor);
    }
}
