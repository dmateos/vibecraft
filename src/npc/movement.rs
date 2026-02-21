use std::collections::HashMap;

use bevy::prelude::*;

use crate::config::{SEA_LEVEL, WORLD_HEIGHT};
use crate::physics::{self, CollisionAabb};
use crate::world::{Chunk, get_block_world};

use super::{NPC_HEIGHT, NPC_RADIUS, NPC_STEP_HEIGHT, VILLAGE_SETTLEMENT_RADIUS, WATER_AVOID_LEVEL};

pub(super) fn find_spawn_ground(chunks: &HashMap<IVec2, Chunk>, x: i32, z: i32) -> Option<i32> {
    for y in (2..(WORLD_HEIGHT as i32 - 3)).rev() {
        let ground = get_block_world(chunks, x, y, z);
        if !physics::is_solid_for_npc(ground) {
            continue;
        }
        if y <= SEA_LEVEL {
            return None;
        }
        let a1 = get_block_world(chunks, x, y + 1, z);
        let a2 = get_block_world(chunks, x, y + 2, z);
        if !physics::is_solid_for_npc(a1) && !physics::is_solid_for_npc(a2) {
            return Some(y + 1);
        }
    }
    None
}

#[inline]
pub(super) fn has_support_npc(chunks: &HashMap<IVec2, Chunk>, x: i32, y: i32, z: i32) -> bool {
    physics::has_support(chunks, x, y, z, physics::is_solid_for_npc)
}

pub(super) fn enters_water(chunks: &HashMap<IVec2, Chunk>, x: i32, z: i32) -> bool {
    match find_spawn_ground(chunks, x, z) {
        Some(ground_y) => ground_y <= WATER_AVOID_LEVEL + 1,
        None => true,
    }
}

pub(super) fn clamp_to_home(pos: Vec3, center: Vec2, radius: f32) -> Vec3 {
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

pub(super) fn collides_npc(chunks: &HashMap<IVec2, Chunk>, feet: Vec3) -> bool {
    let body = CollisionAabb::from_feet(feet, NPC_RADIUS, NPC_HEIGHT);
    physics::query_collision(chunks, body, physics::is_solid_for_npc)
}

pub(super) fn try_step_up_npc(
    current: Vec3,
    horizontal_delta: Vec3,
    chunks: &HashMap<IVec2, Chunk>,
) -> Option<Vec3> {
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

pub(super) fn choose_walk_heading(
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
        let mut score = wrap_angle(h - desired_heading).abs() * 1.8
            + wrap_angle(h - current_heading).abs() * 0.45;
        if enters_water(chunks, probe.x.floor() as i32, probe.z.floor() as i32) {
            score += 7.0;
        }
        if collides_npc(chunks, probe) && try_step_up_npc(pos, probe - pos, chunks).is_none() {
            score += 8.0;
        }
        if !has_support_npc(
            chunks,
            probe.x.floor() as i32,
            (probe.y - 0.2).floor() as i32,
            probe.z.floor() as i32,
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

#[inline]
pub(super) fn random_friendly_line(seed: &mut u32) -> &'static str {
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
pub(super) fn wrap_angle(a: f32) -> f32 {
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
pub(super) fn next_rand(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    (x as f32) / (u32::MAX as f32)
}

pub(super) fn settlement_anchor_for_position(seed: u32, x: i32, z: i32) -> Option<(Vec2, f32)> {
    let p = Vec2::new(x as f32, z as f32);
    const VILLAGE_CELL: i32 = 80;
    let gx = crate::world::div_floor(x, VILLAGE_CELL);
    let gz = crate::world::div_floor(z, VILLAGE_CELL);
    for cz in (gz - 2)..=(gz + 2) {
        for cx in (gx - 2)..=(gx + 2) {
            let h = hash3(cx, cz, seed ^ 0x51AA_92F1);
            let guaranteed_origin = cx == 0 && cz == 0;
            if !guaranteed_origin && (h & 0xFF) < 232 {
                continue;
            }

            let vx = cx * VILLAGE_CELL
                + ((((h >> 8) as i32).rem_euclid(VILLAGE_CELL)) - (VILLAGE_CELL / 2));
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
pub(super) fn should_spawn_cell(cell: IVec2, seed: u32) -> bool {
    let h = hash3(cell.x, cell.y, seed ^ 0x736E_7063);
    (h & 0xFF) >= 214
}

#[inline]
pub(super) fn hash3(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed ^ (x as u32).wrapping_mul(374_761_393) ^ (z as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}
