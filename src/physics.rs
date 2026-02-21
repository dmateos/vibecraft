//! Shared voxel collision helpers used by player/NPC simulation.
//! Centralizes AABB vs. block queries so gameplay systems avoid duplicating
//! floor/ceiling/wall checks with slightly different edge behavior.
use std::collections::HashMap;

use bevy::prelude::*;

use crate::world::{get_block_world, Block, Chunk};

#[derive(Debug, Clone, Copy)]
pub struct CollisionAabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl CollisionAabb {
    #[inline]
    pub fn from_feet(feet: Vec3, radius: f32, height: f32) -> Self {
        Self {
            min: Vec3::new(feet.x - radius, feet.y, feet.z - radius),
            max: Vec3::new(feet.x + radius, feet.y + height, feet.z + radius),
        }
    }

    #[inline]
    pub fn from_eye(eye: Vec3, radius: f32, eye_height: f32, height: f32) -> Self {
        Self {
            min: Vec3::new(eye.x - radius, eye.y - eye_height, eye.z - radius),
            max: Vec3::new(
                eye.x + radius,
                eye.y + (height - eye_height),
                eye.z + radius,
            ),
        }
    }
}

#[inline]
pub fn is_solid_for_body(block: Block) -> bool {
    block != Block::Air
}

#[inline]
pub fn is_solid_for_npc(block: Block) -> bool {
    block != Block::Air && block != Block::Leaves
}

pub fn query_collision(
    chunks: &HashMap<IVec2, Chunk>,
    aabb: CollisionAabb,
    is_solid: fn(Block) -> bool,
) -> bool {
    let min_x = aabb.min.x.floor() as i32;
    let max_x = aabb.max.x.floor() as i32;
    let min_y = aabb.min.y.floor() as i32;
    let max_y = aabb.max.y.floor() as i32;
    let min_z = aabb.min.z.floor() as i32;
    let max_z = aabb.max.z.floor() as i32;

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
pub fn has_support(
    chunks: &HashMap<IVec2, Chunk>,
    x: i32,
    y: i32,
    z: i32,
    is_solid: fn(Block) -> bool,
) -> bool {
    is_solid(get_block_world(chunks, x, y, z)) || is_solid(get_block_world(chunks, x, y - 1, z))
}
