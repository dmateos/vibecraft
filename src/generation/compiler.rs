use std::collections::HashSet;

use bevy::prelude::*;

use crate::config::{CHUNK_SIZE, WORLD_HEIGHT};
use crate::generation::schema::{GenerationOp, GenerationRequest};
use crate::world::{div_floor, get_block_world, Block, VoxelWorld};

#[derive(Debug, Clone, Copy)]
pub struct BlockEdit {
    pub pos: IVec3,
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct CompiledPlan {
    pub request_id: String,
    pub source: String,
    pub edits: Vec<BlockEdit>,
    pub touched_chunks: HashSet<IVec2>,
}

pub fn compile_request(req: &GenerationRequest, world: &VoxelWorld) -> Result<CompiledPlan, String> {
    let mut edits = Vec::new();
    let mut touched_chunks = HashSet::new();

    for op in &req.ops {
        match op {
            GenerationOp::PlaceBlock { position, block } => {
                let block = parse_block(block)?;
                let pos = IVec3::new(position[0], position[1], position[2]);
                edits.push(BlockEdit { pos, block });
                touched_chunks.insert(IVec2::new(
                    div_floor(pos.x, CHUNK_SIZE as i32),
                    div_floor(pos.z, CHUNK_SIZE as i32),
                ));
            }
            GenerationOp::PlacePrefab {
                prefab,
                position,
                rotation,
                ..
            } => {
                compile_prefab(
                    prefab,
                    IVec3::new(position[0], position[1], position[2]),
                    rotation.rem_euclid(360),
                    &mut edits,
                    &mut touched_chunks,
                )?;
            }
            GenerationOp::PaintRegion {
                shape: _,
                center,
                radius,
                surface_block,
                ..
            } => {
                let block = parse_block(surface_block)?;
                compile_paint_circle(center[0], center[1], *radius, block, world, &mut edits, &mut touched_chunks);
            }
        }
    }

    if edits.len() > 200_000 {
        return Err(format!("plan too large: {} edits (max 200000)", edits.len()));
    }

    Ok(CompiledPlan {
        request_id: req.request_id.clone(),
        source: req.source.clone(),
        edits,
        touched_chunks,
    })
}

fn compile_paint_circle(
    cx: i32,
    cz: i32,
    radius: i32,
    block: Block,
    world: &VoxelWorld,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) {
    let rr = radius * radius;
    for z in (cz - radius)..=(cz + radius) {
        for x in (cx - radius)..=(cx + radius) {
            let dx = x - cx;
            let dz = z - cz;
            if dx * dx + dz * dz > rr {
                continue;
            }

            if let Some(y) = find_surface_y(world, x, z) {
                edits.push(BlockEdit {
                    pos: IVec3::new(x, y, z),
                    block,
                });
                touched_chunks.insert(IVec2::new(
                    div_floor(x, CHUNK_SIZE as i32),
                    div_floor(z, CHUNK_SIZE as i32),
                ));
            }
        }
    }
}

fn find_surface_y(world: &VoxelWorld, x: i32, z: i32) -> Option<i32> {
    for y in (0..WORLD_HEIGHT as i32).rev() {
        let b = get_block_world(&world.chunks, x, y, z);
        if b != Block::Air && b != Block::Leaves {
            return Some(y);
        }
    }
    None
}

fn compile_prefab(
    prefab: &str,
    origin: IVec3,
    rot_deg: i32,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) -> Result<(), String> {
    let normalized = normalize_prefab_name(prefab);
    let voxels = prefab_voxels(&normalized)?;
    for (offset, block) in voxels {
        let r = rotate_y(offset, rot_deg);
        let p = origin + r;
        edits.push(BlockEdit { pos: p, block });
        touched_chunks.insert(IVec2::new(
            div_floor(p.x, CHUNK_SIZE as i32),
            div_floor(p.z, CHUNK_SIZE as i32),
        ));
    }
    Ok(())
}

fn rotate_y(v: IVec3, deg: i32) -> IVec3 {
    match deg.rem_euclid(360) {
        0 => v,
        90 => IVec3::new(-v.z, v.y, v.x),
        180 => IVec3::new(-v.x, v.y, -v.z),
        270 => IVec3::new(v.z, v.y, -v.x),
        _ => v,
    }
}

fn prefab_voxels(name: &str) -> Result<Vec<(IVec3, Block)>, String> {
    match name {
        "oak_tree_small" => {
            let mut out = Vec::new();
            for y in 0..5 {
                out.push((IVec3::new(0, y, 0), Block::Wood));
            }
            for y in 3..=5 {
                for z in -2_i32..=2_i32 {
                    for x in -2_i32..=2_i32 {
                        if x == 0 && z == 0 && y <= 4 {
                            continue;
                        }
                        if x.abs() + z.abs() <= 3 {
                            out.push((IVec3::new(x, y, z), Block::Leaves));
                        }
                    }
                }
            }
            Ok(out)
        }
        "pine_tree_large" => {
            let mut out = Vec::new();
            for y in 0..8 {
                out.push((IVec3::new(0, y, 0), Block::Wood));
            }
            for layer in 0..6 {
                let y = 7 - layer;
                let r: i32 = if layer <= 1 { 1 } else { 2 };
                for z in -r..=r {
                    for x in -r..=r {
                        if x.abs() + z.abs() <= r + 1 {
                            out.push((IVec3::new(x, y, z), Block::Leaves));
                        }
                    }
                }
            }
            out.push((IVec3::new(0, 8, 0), Block::Leaves));
            Ok(out)
        }
        "stone_ring" => {
            let mut out = Vec::new();
            let points = [
                IVec3::new(0, 0, -4),
                IVec3::new(3, 0, -3),
                IVec3::new(4, 0, 0),
                IVec3::new(3, 0, 3),
                IVec3::new(0, 0, 4),
                IVec3::new(-3, 0, 3),
                IVec3::new(-4, 0, 0),
                IVec3::new(-3, 0, -3),
            ];
            for p in points {
                out.push((p, Block::Stone));
                out.push((p + IVec3::Y, Block::Stone));
            }
            Ok(out)
        }
        _ => Err(format!("unknown prefab: {name}")),
    }
}

fn parse_block(s: &str) -> Result<Block, String> {
    match s.trim().to_lowercase().as_str() {
        "grass" => Ok(Block::Grass),
        "dirt" => Ok(Block::Dirt),
        "stone" => Ok(Block::Stone),
        "sand" => Ok(Block::Sand),
        "snow" => Ok(Block::Snow),
        "wood" => Ok(Block::Wood),
        "leaves" => Ok(Block::Leaves),
        "red" => Ok(Block::Red),
        "blue" => Ok(Block::Blue),
        "yellow" => Ok(Block::Yellow),
        "purple" => Ok(Block::Purple),
        "cyan" => Ok(Block::Cyan),
        _ => Err(format!("unsupported block: {s}")),
    }
}

fn normalize_prefab_name(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let token = lowered
        .split(['|', ',', '/', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'');

    match token {
        "oak_tree_small" | "oak tree small" | "oak" => "oak_tree_small".to_string(),
        "pine_tree_large" | "pine tree large" | "pine" => "pine_tree_large".to_string(),
        "stone_ring" | "stone ring" | "ring" => "stone_ring".to_string(),
        other => other.to_string(),
    }
}
