use bevy::prelude::*;

use crate::generation::PromptInputState;
use crate::net_client::{NetClientState, is_remote_simulation};
use crate::perception;
use crate::physics;
use crate::player::FlyCam;
use crate::world::VoxelWorld;

use super::movement::{
    choose_walk_heading, clamp_to_home, collides_npc, enters_water, has_support_npc, next_rand,
    random_friendly_line, try_step_up_npc, wrap_angle,
};
use super::{
    FRIENDLY_INTERACT_RANGE, FRIENDLY_VISION_DOT, FRIENDLY_VISION_RANGE, HOSTILE_AGGRO_RANGE,
    HOSTILE_ATTACK_RANGE, HOSTILE_HEARING_RANGE, HOSTILE_VISION_DOT, HOSTILE_VISION_RANGE,
    NPC_GRAVITY, NPC_INVESTIGATE_MEMORY, NPC_MAX_FALL, NPC_SIGHT_MEMORY,
};
use super::{Npc, NpcKind, NpcRig, NpcStimulus, NpcUiState, PlayerVitals};

pub fn capture_player_noise(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    net: Option<Res<NetClientState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut stim: ResMut<NpcStimulus>,
) {
    if is_remote_simulation(net.as_deref()) {
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
    if is_remote_simulation(net.as_deref()) {
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
    if is_remote_simulation(net.as_deref()) {
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
            NpcKind::Friendly => perception::can_see_target(
                transform.translation,
                npc.heading,
                1.45,
                player_pos,
                1.45,
                FRIENDLY_VISION_RANGE,
                FRIENDLY_VISION_DOT,
                &world.chunks,
                physics::is_solid_for_npc,
            ),
            NpcKind::Hostile => perception::can_see_target(
                transform.translation,
                npc.heading,
                1.45,
                player_pos,
                1.45,
                HOSTILE_VISION_RANGE,
                HOSTILE_VISION_DOT,
                &world.chunks,
                physics::is_solid_for_npc,
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
            if !has_support_npc(
                &world.chunks,
                ahead.x.floor() as i32,
                ahead.y.floor() as i32,
                ahead.z.floor() as i32,
            ) || enters_water(
                &world.chunks,
                ahead.x.floor() as i32,
                ahead.z.floor() as i32,
            ) || collides_npc(
                &world.chunks,
                transform.translation + Vec3::new(dir.x * move_speed * dt, 0.0, dir.y * move_speed * dt),
            ) {
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

        rig_updates.push((
            rig.left_leg,
            rig.right_leg,
            rig.left_arm,
            rig.right_arm,
            swing,
            rig.quadruped,
        ));
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
