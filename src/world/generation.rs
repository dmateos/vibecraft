use noise::{NoiseFn, Perlin};

use bevy::prelude::*;

use crate::config::{CHUNK_SIZE, SEA_LEVEL, WORLD_HEIGHT};

use super::landmarks::{
    stamp_biome_features, stamp_dense_forest, stamp_desert_pyramid, stamp_grand_monument,
    stamp_maze, stamp_megacity, stamp_observatory, stamp_trees, stamp_villages,
};
use super::{BiomeKind, Block, Chunk, TerrainMode};
pub fn generate_chunk(pos: IVec2, seed: u32, mode: TerrainMode) -> Chunk {
    if mode == TerrainMode::Flat {
        return generate_flat_chunk(pos);
    }

    let mut chunk = Chunk::new(pos);
    let noise = TerrainNoise::new(seed);

    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = base_x + x as i32;
            let wz = base_z + z as i32;

            let surface = sample_surface(&noise, wx, wz);
            chunk.set_biome_local(x, z, surface.biome);
            let h = surface.height;

            for y in 0..=h {
                let yi = y as i32;
                let is_surface = y == h;
                let is_subsurface = y + 4 > h;

                if yi > 6 && y + 4 < h {
                    let warp = noise.perlin_cave_warp.get([
                        wx as f64 * 0.018,
                        yi as f64 * 0.018,
                        wz as f64 * 0.018,
                    ]) as f32;
                    let cave = noise.perlin_cave.get([
                        wx as f64 * 0.035 + warp as f64 * 0.8,
                        yi as f64 * 0.052,
                        wz as f64 * 0.035 - warp as f64 * 0.8,
                    ]) as f32;
                    let cave_threshold =
                        0.62 + ((yi as f32 / WORLD_HEIGHT as f32) - 0.5) * 0.06 + surface.ridge.abs() * 0.05;
                    if cave > cave_threshold {
                        continue;
                    }
                }

                let block = if is_surface {
                    biome_surface_block(surface.biome, yi)
                } else if is_subsurface {
                    biome_subsurface_block(surface.biome, yi)
                } else {
                    biome_core_block(surface.biome)
                };
                chunk.set_local(x, y, z, block);
            }
        }
    }

    stamp_trees(&mut chunk, &noise);
    stamp_dense_forest(&mut chunk, &noise);
    stamp_biome_features(&mut chunk, &noise);
    stamp_villages(&mut chunk, &noise);
    stamp_grand_monument(&mut chunk, &noise);
    stamp_megacity(&mut chunk, &noise);
    stamp_maze(&mut chunk, &noise);
    stamp_desert_pyramid(&mut chunk, &noise);
    stamp_observatory(&mut chunk, &noise);
    chunk
}

fn generate_flat_chunk(pos: IVec2) -> Chunk {
    let mut chunk = Chunk::new(pos);
    let surface = SEA_LEVEL + 1;
    let dirt_depth = 4;

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set_biome_local(x, z, BiomeKind::Plains);
            for y in 0..=surface {
                let yi = y as i32;
                let block = if yi == surface {
                    Block::Grass
                } else if yi >= surface - dirt_depth {
                    Block::Dirt
                } else {
                    Block::Stone
                };
                chunk.set_local(x, y as usize, z, block);
            }
        }
    }

    chunk
}
pub(super) struct TerrainNoise {
    pub(super) seed: u32,
    pub(super) perlin_macro: Perlin,
    pub(super) perlin_detail: Perlin,
    pub(super) perlin_ridge: Perlin,
    pub(super) perlin_biome: Perlin,
    pub(super) perlin_temp: Perlin,
    pub(super) perlin_moisture: Perlin,
    pub(super) perlin_continent: Perlin,
    pub(super) perlin_cave: Perlin,
    pub(super) perlin_cave_warp: Perlin,
}

impl TerrainNoise {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            seed,
            perlin_macro: Perlin::new(seed),
            perlin_detail: Perlin::new(seed ^ 0x9E37_79B9),
            perlin_ridge: Perlin::new(seed ^ 0xA341_316C),
            perlin_biome: Perlin::new(seed ^ 0xC801_3EA4),
            perlin_temp: Perlin::new(seed ^ 0x1E7A_9F2D),
            perlin_moisture: Perlin::new(seed ^ 0x8B41_A2C7),
            perlin_continent: Perlin::new(seed ^ 0x6F12_BD9A),
            perlin_cave: Perlin::new(seed ^ 0x7F4A_7C15),
            perlin_cave_warp: Perlin::new(seed ^ 0xB529_7A4D),
        }
    }
}

pub(super) struct SurfaceSample {
    pub(super) height: usize,
    pub(super) ridge: f32,
    pub(super) biome: BiomeKind,
}

#[inline]
pub(super) fn sample_surface(noise: &TerrainNoise, wx: i32, wz: i32) -> SurfaceSample {
    let n1 = noise.perlin_macro.get([wx as f64 * 0.008, wz as f64 * 0.008]) as f32;
    let n2 = noise.perlin_detail.get([wx as f64 * 0.024, wz as f64 * 0.024]) as f32;
    let n3 = noise.perlin_macro.get([wx as f64 * 0.0035, wz as f64 * 0.0035]) as f32;
    let ridge = noise.perlin_ridge.get([wx as f64 * 0.015, wz as f64 * 0.015]) as f32;
    let biome = noise.perlin_biome.get([wx as f64 * 0.0024, wz as f64 * 0.0024]) as f32;
    let temp = noise.perlin_temp.get([wx as f64 * 0.0019, wz as f64 * 0.0019]) as f32;
    let moisture = noise
        .perlin_moisture
        .get([wx as f64 * 0.0022 + 121.3, wz as f64 * 0.0022 - 87.1]) as f32;
    let continent = noise
        .perlin_continent
        .get([wx as f64 * 0.0017, wz as f64 * 0.0017]) as f32;
    let ridge_abs = ridge.abs();
    let cliff_mask = (ridge_abs - 0.58).max(0.0);
    let cliff = cliff_mask * cliff_mask * 62.0;
    let continental = ((continent + 1.0) * 0.5).powf(1.3);
    // Slightly raise continental baseline to reduce ocean coverage.
    let base = SEA_LEVEL as f32 - 14.0 + continental * 50.0;
    let mut height = base + n1 * 16.0 + n2 * 7.0 + n3 * 20.0 + cliff + biome * 6.0;
    height = height.clamp(6.0, (WORLD_HEIGHT - 2) as f32);
    let biome_kind = classify_biome(temp, moisture, ridge, height as i32);
    SurfaceSample {
        height: height as usize,
        ridge,
        biome: biome_kind,
    }
}

#[inline]
fn classify_biome(temp: f32, moisture: f32, ridge: f32, height: i32) -> BiomeKind {
    if height >= SEA_LEVEL + 52 || (temp < -0.45 && height >= SEA_LEVEL + 20) {
        return BiomeKind::Tundra;
    }
    if moisture > 0.34 && temp > -0.10 && height <= SEA_LEVEL + 10 {
        return BiomeKind::Swamp;
    }
    if temp > 0.30 && moisture < -0.20 {
        return BiomeKind::Desert;
    }
    if ridge.abs() > 0.62 || height >= SEA_LEVEL + 34 {
        return BiomeKind::Rocky;
    }
    if moisture > 0.12 {
        return BiomeKind::Forest;
    }
    BiomeKind::Plains
}

#[inline]
fn biome_surface_block(biome: BiomeKind, y: i32) -> Block {
    if y <= SEA_LEVEL - 1 {
        return Block::Sand;
    }
    match biome {
        BiomeKind::Desert => Block::Sand,
        BiomeKind::Tundra => Block::Snow,
        BiomeKind::Rocky => Block::Stone,
        BiomeKind::Swamp => {
            if y <= SEA_LEVEL + 1 {
                Block::Dirt
            } else {
                Block::Grass
            }
        }
        BiomeKind::Forest | BiomeKind::Plains => Block::Grass,
    }
}

#[inline]
fn biome_subsurface_block(biome: BiomeKind, y: i32) -> Block {
    if y <= SEA_LEVEL - 2 {
        return Block::Sand;
    }
    match biome {
        BiomeKind::Desert => Block::Sand,
        BiomeKind::Tundra => {
            if y >= SEA_LEVEL + 18 {
                Block::Snow
            } else {
                Block::Stone
            }
        }
        BiomeKind::Rocky => Block::Stone,
        BiomeKind::Swamp => Block::Dirt,
        BiomeKind::Forest | BiomeKind::Plains => Block::Dirt,
    }
}

#[inline]
fn biome_core_block(biome: BiomeKind) -> Block {
    match biome {
        BiomeKind::Desert => Block::Sand,
        BiomeKind::Swamp => Block::Dirt,
        _ => Block::Stone,
    }
}
