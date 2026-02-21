//! First-person controller: camera look, movement, collision, and fly mode.
//! Provides ground physics, step-up handling, and cursor capture behavior
//! shared across local and connected gameplay modes.
use std::collections::HashMap;

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;

use crate::config::{
    EYE_HEIGHT, GRAVITY, JUMP_SPEED, PLAYER_HEIGHT, PLAYER_RADIUS, SPRINT_MULTIPLIER, STEP_HEIGHT,
    WALK_SPEED,
};
use crate::generation::PromptInputState;
use crate::world::{get_block_world, Chunk, VoxelWorld};

#[derive(Component)]
pub struct FlyCam {
    pub yaw: f32,
    pub pitch: f32,
    pub sensitivity: f32,
    pub velocity: Vec3,
    pub grounded: bool,
    pub fly_mode: bool,
}

pub fn camera_look(
    mut windows: Query<&mut Window>,
    mut motion: EventReader<MouseMotion>,
    mut q: Query<(&mut Transform, &mut FlyCam)>,
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
) {
    if prompt.active {
        motion.clear();
        return;
    }

    let Ok((mut transform, mut cam)) = q.get_single_mut() else {
        return;
    };

    let Ok(mut window) = windows.get_single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Escape) {
        window.cursor.visible = true;
        window.cursor.grab_mode = CursorGrabMode::None;
    }
    if keys.just_pressed(KeyCode::Tab) && window.cursor.grab_mode != CursorGrabMode::Locked {
        window.cursor.visible = false;
        window.cursor.grab_mode = CursorGrabMode::Locked;
    }

    if window.cursor.grab_mode != CursorGrabMode::Locked {
        motion.clear();
        return;
    }

    let mut delta = Vec2::ZERO;
    for m in motion.read() {
        delta += m.delta;
    }

    if delta == Vec2::ZERO {
        return;
    }

    cam.yaw -= delta.x * cam.sensitivity;
    cam.pitch -= delta.y * cam.sensitivity;
    cam.pitch = cam.pitch.clamp(-1.54, 1.54);

    let yaw_rot = Quat::from_axis_angle(Vec3::Y, cam.yaw);
    let pitch_rot = Quat::from_axis_angle(Vec3::X, cam.pitch);
    transform.rotation = yaw_rot * pitch_rot;
}

pub fn player_move_and_collision(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    world: Res<VoxelWorld>,
    mut q: Query<(&mut Transform, &mut FlyCam)>,
    prompt: Res<PromptInputState>,
) {
    if prompt.active {
        return;
    }

    let Ok((mut transform, mut cam)) = q.get_single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::KeyF) {
        cam.fly_mode = !cam.fly_mode;
        cam.velocity = Vec3::ZERO;
        cam.grounded = false;
    }

    let dt = time.delta_seconds();
    let mut wish = Vec3::ZERO;
    let mut forward = *transform.forward();
    forward.y = 0.0;
    if forward.length_squared() > 0.0 {
        forward = forward.normalize();
    }
    let mut right = *transform.right();
    right.y = 0.0;
    if right.length_squared() > 0.0 {
        right = right.normalize();
    }

    if keys.pressed(KeyCode::KeyW) {
        wish += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        wish -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        wish += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        wish -= right;
    }

    let sprint = if keys.pressed(KeyCode::ControlLeft) {
        SPRINT_MULTIPLIER
    } else {
        1.0
    };
    let speed = WALK_SPEED * sprint;

    if cam.fly_mode {
        if keys.pressed(KeyCode::Space) {
            wish += Vec3::Y;
        }
        if keys.pressed(KeyCode::ShiftLeft) {
            wish -= Vec3::Y;
        }

        if wish.length_squared() > 0.0 {
            transform.translation += wish.normalize() * speed * 1.7 * dt;
        }
        cam.velocity = Vec3::ZERO;
        cam.grounded = false;
        return;
    }

    if wish.length_squared() > 0.0 {
        wish = wish.normalize() * speed;
    }

    cam.velocity.x = wish.x;
    cam.velocity.z = wish.z;
    cam.velocity.y += GRAVITY * dt;

    if cam.grounded && keys.just_pressed(KeyCode::Space) {
        cam.velocity.y = JUMP_SPEED;
        cam.grounded = false;
    }

    let mut new_pos = transform.translation;
    let mut grounded = false;

    let x_step = Vec3::new(cam.velocity.x * dt, 0.0, 0.0);
    if !collides_player(new_pos + x_step, &world.chunks) {
        new_pos += x_step;
    } else if let Some(stepped) = try_step_up(new_pos, x_step, &world.chunks) {
        new_pos = stepped;
    } else {
        cam.velocity.x = 0.0;
    }

    let z_step = Vec3::new(0.0, 0.0, cam.velocity.z * dt);
    if !collides_player(new_pos + z_step, &world.chunks) {
        new_pos += z_step;
    } else if let Some(stepped) = try_step_up(new_pos, z_step, &world.chunks) {
        new_pos = stepped;
    } else {
        cam.velocity.z = 0.0;
    }

    if chunks_loaded_for_player(new_pos, &world.chunks) {
        let y_step = Vec3::new(0.0, cam.velocity.y * dt, 0.0);
        if !collides_player(new_pos + y_step, &world.chunks) {
            new_pos += y_step;
        } else {
            if cam.velocity.y < 0.0 {
                grounded = true;
            }
            cam.velocity.y = 0.0;
        }
    } else {
        // Prevent falling through temporarily-unloaded terrain while streaming catches up.
        cam.velocity.y = 0.0;
        grounded = true;
    }

    cam.grounded = grounded;
    transform.translation = new_pos;
}

fn chunks_loaded_for_player(eye_pos: Vec3, chunks: &HashMap<IVec2, Chunk>) -> bool {
    let min_x = (eye_pos.x - PLAYER_RADIUS).floor() as i32;
    let max_x = (eye_pos.x + PLAYER_RADIUS).floor() as i32;
    let min_z = (eye_pos.z - PLAYER_RADIUS).floor() as i32;
    let max_z = (eye_pos.z + PLAYER_RADIUS).floor() as i32;

    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let cx = crate::world::div_floor(x, crate::config::CHUNK_SIZE as i32);
            let cz = crate::world::div_floor(z, crate::config::CHUNK_SIZE as i32);
            if !chunks.contains_key(&IVec2::new(cx, cz)) {
                return false;
            }
        }
    }
    true
}

fn try_step_up(current: Vec3, horizontal_delta: Vec3, chunks: &HashMap<IVec2, Chunk>) -> Option<Vec3> {
    let raised = current + Vec3::Y * STEP_HEIGHT;
    if collides_player(raised, chunks) {
        return None;
    }

    let moved = raised + horizontal_delta;
    if collides_player(moved, chunks) {
        return None;
    }

    let mut snapped = moved;
    let drop_step = 0.1;
    let mut dropped = 0.0;
    while dropped < STEP_HEIGHT + 0.1 {
        let next = snapped - Vec3::Y * drop_step;
        if collides_player(next, chunks) {
            break;
        }
        snapped = next;
        dropped += drop_step;
    }

    Some(snapped)
}

pub fn collides_player(eye_pos: Vec3, chunks: &HashMap<IVec2, Chunk>) -> bool {
    let min = Vec3::new(
        eye_pos.x - PLAYER_RADIUS,
        eye_pos.y - EYE_HEIGHT,
        eye_pos.z - PLAYER_RADIUS,
    );
    let max = Vec3::new(
        eye_pos.x + PLAYER_RADIUS,
        eye_pos.y + (PLAYER_HEIGHT - EYE_HEIGHT),
        eye_pos.z + PLAYER_RADIUS,
    );

    let min_x = min.x.floor() as i32;
    let max_x = max.x.floor() as i32;
    let min_y = min.y.floor() as i32;
    let max_y = max.y.floor() as i32;
    let min_z = min.z.floor() as i32;
    let max_z = max.z.floor() as i32;

    for y in min_y..=max_y {
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if get_block_world(chunks, x, y, z) != crate::world::Block::Air {
                    return true;
                }
            }
        }
    }

    false
}
