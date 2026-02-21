//! Shared perception helpers (line-of-sight and FOV checks).
//! Keeps visibility behavior consistent across NPC systems and debug tools.
use std::collections::HashMap;

use bevy::prelude::*;

use crate::world::{get_block_world, Block, Chunk};

pub fn line_of_sight_clear(
    from: Vec3,
    to: Vec3,
    chunks: &HashMap<IVec2, Chunk>,
    is_opaque: fn(Block) -> bool,
) -> bool {
    let delta = to - from;
    let dist = delta.length();
    if dist <= 0.001 {
        return true;
    }
    let dir = delta / dist;
    let mut t = 0.35;
    while t < dist - 0.2 {
        let p = from + dir * t;
        let b = get_block_world(
            chunks,
            p.x.floor() as i32,
            p.y.floor() as i32,
            p.z.floor() as i32,
        );
        if is_opaque(b) {
            return false;
        }
        t += 0.45;
    }
    true
}

pub fn can_see_target(
    actor_pos: Vec3,
    heading: f32,
    eye_height: f32,
    target_pos: Vec3,
    target_eye_height: f32,
    max_dist: f32,
    fov_dot: f32,
    chunks: &HashMap<IVec2, Chunk>,
    is_opaque: fn(Block) -> bool,
) -> bool {
    let eye = actor_pos + Vec3::new(0.0, eye_height, 0.0);
    let target = target_pos + Vec3::new(0.0, target_eye_height, 0.0);
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
    line_of_sight_clear(eye, target, chunks, is_opaque)
}
