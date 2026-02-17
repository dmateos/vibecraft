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
                let material = parse_material(block)?;
                let pos = IVec3::new(position[0], position[1], position[2]);
                push_material_edit(&mut edits, &mut touched_chunks, pos, &material);
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
                let material = parse_material(surface_block)?;
                compile_paint_circle(
                    center[0],
                    center[1],
                    *radius,
                    &material,
                    world,
                    &mut edits,
                    &mut touched_chunks,
                );
            }
            GenerationOp::FillBox { min, max, block } => {
                let material = parse_material(block)?;
                compile_fill_box(
                    IVec3::new(min[0], min[1], min[2]),
                    IVec3::new(max[0], max[1], max[2]),
                    &material,
                    &mut edits,
                    &mut touched_chunks,
                );
            }
            GenerationOp::HollowBox {
                min,
                max,
                wall_block,
                wall_thickness,
                floor_block,
                roof_block,
            } => {
                let wall = parse_material(wall_block)?;
                let floor = floor_block
                    .as_ref()
                    .map(|v| parse_material(v))
                    .transpose()?;
                let roof = roof_block.as_ref().map(|v| parse_material(v)).transpose()?;
                compile_hollow_box(
                    IVec3::new(min[0], min[1], min[2]),
                    IVec3::new(max[0], max[1], max[2]),
                    *wall_thickness,
                    &wall,
                    floor.as_ref(),
                    roof.as_ref(),
                    &mut edits,
                    &mut touched_chunks,
                );
            }
            GenerationOp::Cylinder {
                center,
                radius,
                height,
                block,
                hollow,
            } => {
                let material = parse_material(block)?;
                compile_cylinder(
                    IVec3::new(center[0], center[1], center[2]),
                    *radius,
                    *height,
                    *hollow,
                    &material,
                    &mut edits,
                    &mut touched_chunks,
                );
            }
            GenerationOp::Sphere {
                center,
                radius,
                block,
                hollow,
            } => {
                let material = parse_material(block)?;
                compile_sphere(
                    IVec3::new(center[0], center[1], center[2]),
                    *radius,
                    *hollow,
                    &material,
                    &mut edits,
                    &mut touched_chunks,
                );
            }
            GenerationOp::Line {
                from,
                to,
                block,
                thickness,
            } => {
                let material = parse_material(block)?;
                compile_line(
                    IVec3::new(from[0], from[1], from[2]),
                    IVec3::new(to[0], to[1], to[2]),
                    *thickness,
                    &material,
                    &mut edits,
                    &mut touched_chunks,
                );
            }
        }
    }

    if edits.len() > 300_000 {
        return Err(format!("plan too large: {} edits (max 300000)", edits.len()));
    }

    Ok(CompiledPlan {
        request_id: req.request_id.clone(),
        source: req.source.clone(),
        edits,
        touched_chunks,
    })
}

fn compile_fill_box(
    min: IVec3,
    max: IVec3,
    material: &MaterialSpec,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) {
    for y in min.y..=max.y {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                push_material_edit(edits, touched_chunks, IVec3::new(x, y, z), material);
            }
        }
    }
}

fn compile_hollow_box(
    min: IVec3,
    max: IVec3,
    wall_thickness: i32,
    wall: &MaterialSpec,
    floor: Option<&MaterialSpec>,
    roof: Option<&MaterialSpec>,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) {
    for y in min.y..=max.y {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                let dx = (x - min.x).min(max.x - x);
                let dy = (y - min.y).min(max.y - y);
                let dz = (z - min.z).min(max.z - z);
                let boundary_dist = dx.min(dy).min(dz);
                let pos = IVec3::new(x, y, z);

                if boundary_dist < wall_thickness {
                    push_material_edit(edits, touched_chunks, pos, wall);
                    continue;
                }

                if y == min.y
                    && let Some(floor_material) = floor
                {
                    push_material_edit(edits, touched_chunks, pos, floor_material);
                    continue;
                }
                if y == max.y
                    && let Some(roof_material) = roof
                {
                    push_material_edit(edits, touched_chunks, pos, roof_material);
                }
            }
        }
    }
}

fn compile_cylinder(
    center: IVec3,
    radius: i32,
    height: i32,
    hollow: bool,
    material: &MaterialSpec,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) {
    let r2 = radius * radius;
    let inner_r = (radius - 1).max(0);
    let inner_r2 = inner_r * inner_r;

    for y in center.y..(center.y + height) {
        for z in (center.z - radius)..=(center.z + radius) {
            for x in (center.x - radius)..=(center.x + radius) {
                let dx = x - center.x;
                let dz = z - center.z;
                let d2 = dx * dx + dz * dz;
                if d2 > r2 {
                    continue;
                }
                if hollow && radius > 1 && d2 < inner_r2 {
                    continue;
                }
                push_material_edit(edits, touched_chunks, IVec3::new(x, y, z), material);
            }
        }
    }
}

fn compile_sphere(
    center: IVec3,
    radius: i32,
    hollow: bool,
    material: &MaterialSpec,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) {
    let r2 = radius * radius;
    let inner_r = (radius - 1).max(0);
    let inner_r2 = inner_r * inner_r;
    for z in (center.z - radius)..=(center.z + radius) {
        for y in (center.y - radius)..=(center.y + radius) {
            for x in (center.x - radius)..=(center.x + radius) {
                let dx = x - center.x;
                let dy = y - center.y;
                let dz = z - center.z;
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 > r2 {
                    continue;
                }
                if hollow && radius > 1 && d2 < inner_r2 {
                    continue;
                }
                push_material_edit(edits, touched_chunks, IVec3::new(x, y, z), material);
            }
        }
    }
}

fn compile_line(
    from: IVec3,
    to: IVec3,
    thickness: i32,
    material: &MaterialSpec,
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
) {
    let d = to - from;
    let steps = d.x.abs().max(d.y.abs()).max(d.z.abs()).max(1);
    let r = thickness - 1;

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = from.x as f32 + d.x as f32 * t;
        let y = from.y as f32 + d.y as f32 * t;
        let z = from.z as f32 + d.z as f32 * t;
        let p = IVec3::new(x.round() as i32, y.round() as i32, z.round() as i32);
        for oz in -r..=r {
            for oy in -r..=r {
                for ox in -r..=r {
                    push_material_edit(edits, touched_chunks, p + IVec3::new(ox, oy, oz), material);
                }
            }
        }
    }
}

fn compile_paint_circle(
    cx: i32,
    cz: i32,
    radius: i32,
    material: &MaterialSpec,
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
                push_material_edit(edits, touched_chunks, IVec3::new(x, y, z), material);
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
        push_edit(edits, touched_chunks, p, block);
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

enum MaterialSpec {
    Single(Block),
    Palette(Vec<Block>),
}

fn parse_material(raw: &str) -> Result<MaterialSpec, String> {
    let s = normalize_token(raw);
    match s.as_str() {
        "grass" => Ok(MaterialSpec::Single(Block::Grass)),
        "dirt" => Ok(MaterialSpec::Single(Block::Dirt)),
        "stone" => Ok(MaterialSpec::Single(Block::Stone)),
        "sand" => Ok(MaterialSpec::Single(Block::Sand)),
        "snow" => Ok(MaterialSpec::Single(Block::Snow)),
        "wood" => Ok(MaterialSpec::Single(Block::Wood)),
        "leaves" => Ok(MaterialSpec::Single(Block::Leaves)),
        "red" => Ok(MaterialSpec::Single(Block::Red)),
        "blue" => Ok(MaterialSpec::Single(Block::Blue)),
        "yellow" => Ok(MaterialSpec::Single(Block::Yellow)),
        "purple" => Ok(MaterialSpec::Single(Block::Purple)),
        "cyan" => Ok(MaterialSpec::Single(Block::Cyan)),
        "castle_stone" => Ok(MaterialSpec::Palette(vec![
            Block::Stone,
            Block::Stone,
            Block::Stone,
            Block::Snow,
        ])),
        "castle_trim" => Ok(MaterialSpec::Palette(vec![Block::Snow, Block::Stone])),
        "castle_floor" => Ok(MaterialSpec::Palette(vec![Block::Stone, Block::Sand])),
        "roof_dark" => Ok(MaterialSpec::Palette(vec![Block::Wood, Block::Purple])),
        "banner_warm" => Ok(MaterialSpec::Palette(vec![Block::Red, Block::Yellow])),
        "banner_cool" => Ok(MaterialSpec::Palette(vec![Block::Blue, Block::Cyan])),
        // Common LLM color aliases mapped onto our limited voxel palette.
        "white" | "light_gray" | "light_grey" => Ok(MaterialSpec::Single(Block::Snow)),
        "gray" | "grey" | "dark_gray" | "dark_grey" | "black" => {
            Ok(MaterialSpec::Single(Block::Stone))
        }
        "brown" => Ok(MaterialSpec::Single(Block::Wood)),
        "green" => Ok(MaterialSpec::Single(Block::Grass)),
        "orange" => Ok(MaterialSpec::Palette(vec![Block::Sand, Block::Red])),
        "pink" => Ok(MaterialSpec::Palette(vec![Block::Red, Block::Snow])),
        "magenta" => Ok(MaterialSpec::Palette(vec![Block::Purple, Block::Red])),
        _ => Err(format!("unsupported block/material: {raw}")),
    }
}

fn normalize_token(raw: &str) -> String {
    raw.to_lowercase()
        .split(['|', ',', '/', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace(' ', "_")
}

fn pick_material_block(material: &MaterialSpec, pos: IVec3) -> Block {
    match material {
        MaterialSpec::Single(block) => *block,
        MaterialSpec::Palette(blocks) => {
            if blocks.is_empty() {
                return Block::Stone;
            }
            let h = hash_pos(pos);
            blocks[(h as usize) % blocks.len()]
        }
    }
}

fn push_material_edit(
    edits: &mut Vec<BlockEdit>,
    touched_chunks: &mut HashSet<IVec2>,
    pos: IVec3,
    material: &MaterialSpec,
) {
    let block = pick_material_block(material, pos);
    push_edit(edits, touched_chunks, pos, block);
}

fn push_edit(edits: &mut Vec<BlockEdit>, touched_chunks: &mut HashSet<IVec2>, pos: IVec3, block: Block) {
    edits.push(BlockEdit { pos, block });
    touched_chunks.insert(IVec2::new(
        div_floor(pos.x, CHUNK_SIZE as i32),
        div_floor(pos.z, CHUNK_SIZE as i32),
    ));
}

fn hash_pos(pos: IVec3) -> u32 {
    let mut h = (pos.x as u32).wrapping_mul(374_761_393)
        ^ (pos.y as u32).wrapping_mul(668_265_263)
        ^ (pos.z as u32).wrapping_mul(2_147_483_647);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn normalize_prefab_name(raw: &str) -> String {
    match normalize_token(raw).as_str() {
        "oak_tree_small" | "oak" => "oak_tree_small".to_string(),
        "pine_tree_large" | "pine" => "pine_tree_large".to_string(),
        "stone_ring" | "ring" => "stone_ring".to_string(),
        other => other.to_string(),
    }
}
