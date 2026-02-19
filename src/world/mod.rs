use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;
use noise::{NoiseFn, Perlin};

use crate::config::{CHUNK_SIZE, SEA_LEVEL, WORLD_HEIGHT};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
    Sand,
    Snow,
    Wood,
    Leaves,
    Red,
    Blue,
    Yellow,
    Purple,
    Cyan,
}

#[derive(Clone)]
pub struct Chunk {
    pub pos: IVec2,
    voxels: Vec<Block>,
    biomes: Vec<BiomeKind>,
}

impl Chunk {
    pub fn new(pos: IVec2) -> Self {
        Self {
            pos,
            voxels: vec![Block::Air; CHUNK_SIZE * WORLD_HEIGHT * CHUNK_SIZE],
            biomes: vec![BiomeKind::Plains; CHUNK_SIZE * CHUNK_SIZE],
        }
    }

    #[inline]
    fn index(x: usize, y: usize, z: usize) -> usize {
        x + z * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE
    }

    #[inline]
    pub fn get_local(&self, x: usize, y: usize, z: usize) -> Block {
        self.voxels[Self::index(x, y, z)]
    }

    #[inline]
    pub fn set_local(&mut self, x: usize, y: usize, z: usize, block: Block) {
        let idx = Self::index(x, y, z);
        self.voxels[idx] = block;
    }

    #[inline]
    fn biome_index(x: usize, z: usize) -> usize {
        x + z * CHUNK_SIZE
    }

    #[inline]
    fn set_biome_local(&mut self, x: usize, z: usize, biome: BiomeKind) {
        let idx = Self::biome_index(x, z);
        self.biomes[idx] = biome;
    }

    #[inline]
    fn get_biome_local(&self, x: usize, z: usize) -> BiomeKind {
        self.biomes[Self::biome_index(x, z)]
    }
}

#[derive(Resource)]
pub struct VoxelWorld {
    pub seed: u32,
    pub chunks: HashMap<IVec2, Chunk>,
}

#[derive(Resource, Debug, Copy, Clone, Eq, PartialEq)]
pub enum TerrainMode {
    Procedural,
    Flat,
}

impl TerrainMode {
    pub fn label(self) -> &'static str {
        match self {
            TerrainMode::Procedural => "Procedural",
            TerrainMode::Flat => "Flat",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            TerrainMode::Procedural => TerrainMode::Flat,
            TerrainMode::Flat => TerrainMode::Procedural,
        }
    }
}

#[derive(Clone)]
pub struct ChunkRender {
    pub entity: Entity,
    pub mesh: Handle<Mesh>,
    pub lod: u8,
}

#[derive(Resource, Default)]
pub struct LoadedChunks {
    pub entries: HashMap<IVec2, ChunkRender>,
}

#[derive(Resource)]
pub struct StreamTimer(pub Timer);

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
    stamp_biome_features(&mut chunk, &noise);
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

pub fn build_chunk_mesh_lod(pos: IVec2, chunks: &HashMap<IVec2, Chunk>, lod: u8) -> Mesh {
    let step = 1usize << lod.min(2);
    if step <= 1 {
        return build_chunk_mesh_full(pos, chunks);
    }
    build_chunk_mesh_coarse(pos, chunks, step)
}

fn build_chunk_mesh_full(pos: IVec2, chunks: &HashMap<IVec2, Chunk>) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    let Some(chunk) = chunks.get(&pos) else {
        return empty_mesh();
    };

    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;

    for y in 0..WORLD_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get_local(x, y, z);
                if block == Block::Air {
                    continue;
                }

                let wx = base_x + x as i32;
                let wy = y as i32;
                let wz = base_z + z as i32;
                let biome = chunk.get_biome_local(x, z);

                for face in FACES {
                    let neighbor = get_block_world(
                        chunks,
                        wx + face.normal[0],
                        wy + face.normal[1],
                        wz + face.normal[2],
                    );
                    if neighbor != Block::Air {
                        continue;
                    }

                    let start = positions.len() as u32;
                    let tint = block_tint(block);
                    let tex_id = block_face_texture_id(block, face, biome, wx, wz) as f32;

                    for (vidx, v) in face.verts.into_iter().enumerate() {
                        positions.push([
                            x as f32 + v[0],
                            y as f32 + v[1],
                            z as f32 + v[2],
                        ]);
                        normals.push([
                            face.normal[0] as f32,
                            face.normal[1] as f32,
                            face.normal[2] as f32,
                        ]);
                        uvs.push(face.uvs[vidx]);
                        let ao = face_vertex_ao(
                            chunks,
                            wx,
                            wy,
                            wz,
                            face,
                            vidx,
                        );
                        let ao_packed = ao.clamp(0.0, 0.999);
                        colors.push([tint[0], tint[1], tint[2], tex_id + ao_packed]);
                    }

                    indices.extend_from_slice(&[
                        start,
                        start + 1,
                        start + 2,
                        start,
                        start + 2,
                        start + 3,
                    ]);
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn build_chunk_mesh_coarse(pos: IVec2, chunks: &HashMap<IVec2, Chunk>, step: usize) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    let Some(chunk) = chunks.get(&pos) else {
        return empty_mesh();
    };

    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;
    let cube_size = step as f32;
    let step_i = step as i32;

    for y in (0..WORLD_HEIGHT).step_by(step) {
        for z in (0..CHUNK_SIZE).step_by(step) {
            for x in (0..CHUNK_SIZE).step_by(step) {
                let block = chunk.get_local(x, y, z);
                if block == Block::Air {
                    continue;
                }

                let wx = base_x + x as i32;
                let wy = y as i32;
                let wz = base_z + z as i32;
                let biome = chunk.get_biome_local(x, z);

                for face in FACES {
                    let neighbor = get_block_world(
                        chunks,
                        wx + face.normal[0] * step_i,
                        wy + face.normal[1] * step_i,
                        wz + face.normal[2] * step_i,
                    );
                    if neighbor != Block::Air {
                        continue;
                    }

                    let start = positions.len() as u32;
                    let tint = block_tint(block);
                    let tex_id = block_face_texture_id(block, face, biome, wx, wz) as f32;

                    for (vidx, v) in face.verts.into_iter().enumerate() {
                        positions.push([
                            x as f32 + v[0] * cube_size,
                            y as f32 + v[1] * cube_size,
                            z as f32 + v[2] * cube_size,
                        ]);
                        normals.push([
                            face.normal[0] as f32,
                            face.normal[1] as f32,
                            face.normal[2] as f32,
                        ]);
                        uvs.push(face.uvs[vidx]);
                        colors.push([tint[0], tint[1], tint[2], tex_id + 1.0]);
                    }

                    indices.extend_from_slice(&[
                        start,
                        start + 1,
                        start + 2,
                        start,
                        start + 2,
                        start + 3,
                    ]);
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(Vec::new()));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh
}

pub fn get_block_world(chunks: &HashMap<IVec2, Chunk>, x: i32, y: i32, z: i32) -> Block {
    if !(0..WORLD_HEIGHT as i32).contains(&y) {
        return Block::Air;
    }

    let cx = div_floor(x, CHUNK_SIZE as i32);
    let cz = div_floor(z, CHUNK_SIZE as i32);
    let lx = x - cx * CHUNK_SIZE as i32;
    let lz = z - cz * CHUNK_SIZE as i32;

    let pos = IVec2::new(cx, cz);
    let Some(chunk) = chunks.get(&pos) else {
        return Block::Air;
    };

    chunk.get_local(lx as usize, y as usize, lz as usize)
}

pub fn set_block_world(
    chunks: &mut HashMap<IVec2, Chunk>,
    x: i32,
    y: i32,
    z: i32,
    block: Block,
) -> bool {
    if !(0..WORLD_HEIGHT as i32).contains(&y) {
        return false;
    }

    let cx = div_floor(x, CHUNK_SIZE as i32);
    let cz = div_floor(z, CHUNK_SIZE as i32);
    let lx = x - cx * CHUNK_SIZE as i32;
    let lz = z - cz * CHUNK_SIZE as i32;
    let pos = IVec2::new(cx, cz);

    let Some(chunk) = chunks.get_mut(&pos) else {
        return false;
    };

    chunk.set_local(lx as usize, y as usize, lz as usize, block);
    true
}

pub fn remesh_affected_chunks(
    center: IVec2,
    chunks: &HashMap<IVec2, Chunk>,
    loaded: &LoadedChunks,
    meshes: &mut Assets<Mesh>,
) {
    let candidates = [
        center,
        center + IVec2::new(1, 0),
        center + IVec2::new(-1, 0),
        center + IVec2::new(0, 1),
        center + IVec2::new(0, -1),
    ];

    for pos in candidates {
        if let Some(render) = loaded.entries.get(&pos)
            && let Some(mesh) = meshes.get_mut(&render.mesh)
        {
            *mesh = build_chunk_mesh_lod(pos, chunks, render.lod);
        }
    }
}

#[inline]
pub fn div_floor(a: i32, b: i32) -> i32 {
    let mut q = a / b;
    let r = a % b;
    if r != 0 && ((r > 0) != (b > 0)) {
        q -= 1;
    }
    q
}

#[inline]
pub fn chunk_distance_sq(a: IVec2, b: IVec2) -> i32 {
    let d = a - b;
    d.x * d.x + d.y * d.y
}

fn block_tint(block: Block) -> [f32; 3] {
    match block {
        Block::Air => [1.0, 1.0, 1.0],
        Block::Grass
        | Block::Dirt
        | Block::Stone
        | Block::Sand
        | Block::Snow
        | Block::Wood
        | Block::Leaves => [1.0, 1.0, 1.0],
        Block::Red => [0.98, 0.28, 0.30],
        Block::Blue => [0.26, 0.40, 0.96],
        Block::Yellow => [0.96, 0.86, 0.24],
        Block::Purple => [0.70, 0.38, 0.96],
        Block::Cyan => [0.30, 0.88, 0.92],
    }
}

fn block_face_texture_id(block: Block, face: Face, biome: BiomeKind, wx: i32, wz: i32) -> u32 {
    // Atlas is 9x10 tiles (1152x1280) with 128x128 cells.
    // tile_id = row * 9 + col
    const COLS: u32 = 9;
    let tile = |col: u32, row: u32| row * COLS + col;
    let pick3 = |a: u32, b: u32, c: u32| -> u32 {
        let h = hash2(div_floor(wx, 8), div_floor(wz, 8));
        match h % 6 {
            0 | 1 => a,
            2 | 3 => b,
            _ => c,
        }
    };

    let grass_top = tile(6, 1);
    let sand = tile(3, 6);
    let red_sand = tile(4, 3);
    let grey_sand = tile(5, 8);
    let snow = tile(3, 5);
    let dirt = tile(7, 5);
    let dirt_grass = tile(7, 4);
    let dirt_sand = tile(7, 3);
    let dirt_snow = tile(7, 2);
    let stone = tile(3, 4);
    let grey_stone = tile(5, 7);
    let gravel_stone = tile(5, 9);
    let stone_sand = tile(2, 1);
    let stone_snow = tile(1, 8);
    let stone_dirt = tile(5, 6);
    let trunk_top = tile(0, 9);
    let trunk_bottom = tile(1, 2);
    let trunk_side = tile(1, 0);
    let leaves = tile(5, 1);
    let brick_red = tile(8, 3);
    let redstone = tile(8, 4);
    let redstone_emerald = tile(4, 1);
    let redstone_sand = tile(3, 9);

    match block {
        Block::Grass => {
            if face.normal[1] > 0 {
                match biome {
                    BiomeKind::Desert => sand,
                    BiomeKind::Tundra => snow,
                    BiomeKind::Rocky => pick3(stone, grey_stone, gravel_stone),
                    BiomeKind::Swamp | BiomeKind::Forest | BiomeKind::Plains => grass_top,
                }
            } else if face.normal[1] < 0 {
                dirt
            } else {
                match biome {
                    BiomeKind::Desert => dirt_sand,
                    BiomeKind::Tundra => dirt_snow,
                    BiomeKind::Rocky => stone,
                    BiomeKind::Swamp | BiomeKind::Forest | BiomeKind::Plains => dirt_grass,
                }
            }
        }
        Block::Dirt => match biome {
            BiomeKind::Desert => dirt_sand,
            BiomeKind::Tundra => dirt_snow,
            BiomeKind::Swamp => pick3(dirt, stone_dirt, dirt),
            _ => dirt,
        },
        Block::Stone => match biome {
            BiomeKind::Desert => stone_sand,
            BiomeKind::Tundra => stone_snow,
            BiomeKind::Rocky => pick3(stone, grey_stone, gravel_stone),
            BiomeKind::Swamp => stone_dirt,
            _ => stone,
        },
        Block::Sand => pick3(sand, red_sand, grey_sand),
        Block::Snow => {
            if face.normal[1] > 0 {
                snow
            } else if face.normal[1] < 0 {
                dirt
            } else {
                dirt_snow
            }
        }
        Block::Wood => {
            if face.normal[1] > 0 {
                trunk_top
            } else if face.normal[1] < 0 {
                trunk_bottom
            } else {
                trunk_side
            }
        }
        Block::Leaves => leaves,
        Block::Red => brick_red,
        Block::Blue => redstone_emerald,
        Block::Yellow => red_sand,
        Block::Purple => redstone,
        Block::Cyan => redstone_sand,
        Block::Air => stone,
    }
}

#[inline]
fn hash2(x: i32, z: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (z as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn face_vertex_ao(
    chunks: &HashMap<IVec2, Chunk>,
    wx: i32,
    wy: i32,
    wz: i32,
    face: Face,
    vidx: usize,
) -> f32 {
    let n = face.normal;
    let su = face.ao_signs[vidx][0];
    let sv = face.ao_signs[vidx][1];
    let u = face.u_axis;
    let v = face.v_axis;

    let s1 = is_occluding(get_block_world(
        chunks,
        wx + n[0] + u[0] * su,
        wy + n[1] + u[1] * su,
        wz + n[2] + u[2] * su,
    ));
    let s2 = is_occluding(get_block_world(
        chunks,
        wx + n[0] + v[0] * sv,
        wy + n[1] + v[1] * sv,
        wz + n[2] + v[2] * sv,
    ));
    let c = is_occluding(get_block_world(
        chunks,
        wx + n[0] + u[0] * su + v[0] * sv,
        wy + n[1] + u[1] * su + v[1] * sv,
        wz + n[2] + u[2] * su + v[2] * sv,
    ));

    let level = if s1 && s2 { 0 } else { 3 - (s1 as i32 + s2 as i32 + c as i32) };
    match level {
        0 => 0.56,
        1 => 0.72,
        2 => 0.86,
        _ => 1.0,
    }
}

#[inline]
fn is_occluding(block: Block) -> bool {
    block != Block::Air && block != Block::Leaves
}

struct TerrainNoise {
    seed: u32,
    perlin_macro: Perlin,
    perlin_detail: Perlin,
    perlin_ridge: Perlin,
    perlin_biome: Perlin,
    perlin_temp: Perlin,
    perlin_moisture: Perlin,
    perlin_continent: Perlin,
    perlin_cave: Perlin,
    perlin_cave_warp: Perlin,
}

impl TerrainNoise {
    fn new(seed: u32) -> Self {
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BiomeKind {
    Plains,
    Forest,
    Desert,
    Tundra,
    Rocky,
    Swamp,
}

#[derive(Copy, Clone, Debug)]
struct SurfaceSample {
    height: usize,
    ridge: f32,
    biome: BiomeKind,
}

#[inline]
fn sample_surface(noise: &TerrainNoise, wx: i32, wz: i32) -> SurfaceSample {
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

#[derive(Copy, Clone)]
enum TreeKind {
    Oak,
    Pine,
}

fn stamp_trees(chunk: &mut Chunk, noise: &TerrainNoise) {
    const TREE_CELL: i32 = 9;
    const TREE_MARGIN: i32 = 6;

    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let min_x = base_x - TREE_MARGIN;
    let max_x = base_x + CHUNK_SIZE as i32 - 1 + TREE_MARGIN;
    let min_z = base_z - TREE_MARGIN;
    let max_z = base_z + CHUNK_SIZE as i32 - 1 + TREE_MARGIN;

    let cell_min_x = div_floor(min_x, TREE_CELL);
    let cell_max_x = div_floor(max_x, TREE_CELL);
    let cell_min_z = div_floor(min_z, TREE_CELL);
    let cell_max_z = div_floor(max_z, TREE_CELL);

    for cz in cell_min_z..=cell_max_z {
        for cx in cell_min_x..=cell_max_x {
            let h = hash3(cx, cz, noise.seed ^ 0x2A6B_45D1);
            let tx = cx * TREE_CELL + (h as i32 & 7);
            let tz = cz * TREE_CELL + (((h >> 8) as i32) & 7);

            if tx < min_x || tx > max_x || tz < min_z || tz > max_z {
                continue;
            }

            let sample = sample_surface(noise, tx, tz);
            let sy = sample.height as i32;
            if sy <= SEA_LEVEL + 2 {
                continue;
            }
            if sample.ridge.abs() > 0.68 {
                continue;
            }
            let roll = ((h >> 16) & 0xFF) as i32;
            let (threshold, kind) = match sample.biome {
                BiomeKind::Desert => (255, TreeKind::Oak),
                BiomeKind::Rocky => (250, TreeKind::Pine),
                BiomeKind::Tundra => (246, TreeKind::Pine),
                BiomeKind::Plains => (214, TreeKind::Oak),
                BiomeKind::Swamp => (190, TreeKind::Oak),
                BiomeKind::Forest => (
                    158,
                    if (h & 1) == 0 {
                        TreeKind::Oak
                    } else {
                        TreeKind::Pine
                    },
                ),
            };
            if roll < threshold {
                continue;
            }

            let local_seed = hash3(tx, tz, noise.seed ^ 0x91C2_8E4B);
            place_tree(chunk, tx, sy + 1, tz, kind, local_seed);
        }
    }
}

fn stamp_biome_features(chunk: &mut Chunk, noise: &TerrainNoise) {
    const FEATURE_CELL: i32 = 10;
    const FEATURE_MARGIN: i32 = 7;

    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let min_x = base_x - FEATURE_MARGIN;
    let max_x = base_x + CHUNK_SIZE as i32 - 1 + FEATURE_MARGIN;
    let min_z = base_z - FEATURE_MARGIN;
    let max_z = base_z + CHUNK_SIZE as i32 - 1 + FEATURE_MARGIN;

    let cell_min_x = div_floor(min_x, FEATURE_CELL);
    let cell_max_x = div_floor(max_x, FEATURE_CELL);
    let cell_min_z = div_floor(min_z, FEATURE_CELL);
    let cell_max_z = div_floor(max_z, FEATURE_CELL);

    for cz in cell_min_z..=cell_max_z {
        for cx in cell_min_x..=cell_max_x {
            let h = hash3(cx, cz, noise.seed ^ 0x4D2A_9C17);
            let tx = cx * FEATURE_CELL + ((h as i32) & 7);
            let tz = cz * FEATURE_CELL + (((h >> 8) as i32) & 7);
            if tx < min_x || tx > max_x || tz < min_z || tz > max_z {
                continue;
            }

            let sample = sample_surface(noise, tx, tz);
            let sy = sample.height as i32;
            if sy <= SEA_LEVEL + 1 {
                continue;
            }
            if sample.ridge.abs() > 0.76 {
                continue;
            }

            let roll = ((h >> 16) & 0xFF) as i32;
            match sample.biome {
                BiomeKind::Rocky => {
                    if roll >= 216 {
                        let radius = 1 + ((h >> 3) % 2) as i32;
                        place_boulder(chunk, tx, sy + 1, tz, radius, Block::Stone, h);
                    }
                }
                BiomeKind::Desert => {
                    if roll >= 224 {
                        let radius = 1 + ((h >> 5) % 2) as i32;
                        place_boulder(chunk, tx, sy + 1, tz, radius, Block::Sand, h);
                    }
                }
                BiomeKind::Swamp => {
                    if sy <= SEA_LEVEL + 6 && roll >= 200 {
                        place_reed_clump(chunk, tx, sy + 1, tz, h);
                    }
                }
                BiomeKind::Forest | BiomeKind::Plains => {}
                BiomeKind::Tundra => {
                    if roll >= 242 {
                        place_boulder(chunk, tx, sy + 1, tz, 1, Block::Stone, h);
                    }
                }
            }
        }
    }
}

fn place_boulder(chunk: &mut Chunk, cx: i32, cy: i32, cz: i32, radius: i32, material: Block, seed: u32) {
    for dy in -radius..=radius {
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 > radius * radius + (seed as i32 & 1) {
                    continue;
                }
                set_feature_if_air(chunk, cx + dx, cy + dy, cz + dz, material);
            }
        }
    }
}

fn place_reed_clump(chunk: &mut Chunk, x: i32, y: i32, z: i32, seed: u32) {
    let stalks = 2 + ((seed >> 3) % 3) as i32;
    for i in 0..stalks {
        let ox = ((seed >> (i * 2)) as i32 & 1) - (((seed >> (i * 2 + 1)) as i32) & 1);
        let oz = (((seed >> (i * 2 + 4)) as i32) & 1) - (((seed >> (i * 2 + 5)) as i32) & 1);
        let h = 2 + (((seed >> (i * 3 + 8)) % 3) as i32);
        for dy in 0..h {
            set_feature_if_air(chunk, x + ox, y + dy, z + oz, Block::Wood);
        }
        set_feature_if_air(chunk, x + ox, y + h, z + oz, Block::Leaves);
    }
}

#[inline]
fn set_feature_if_air(chunk: &mut Chunk, wx: i32, wy: i32, wz: i32, block: Block) {
    if !(0..WORLD_HEIGHT as i32).contains(&wy) {
        return;
    }
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
        return;
    }
    let x = lx as usize;
    let y = wy as usize;
    let z = lz as usize;
    if chunk.get_local(x, y, z) == Block::Air {
        chunk.set_local(x, y, z, block);
    }
}

fn place_tree(chunk: &mut Chunk, tx: i32, base_y: i32, tz: i32, kind: TreeKind, local_seed: u32) {
    match kind {
        TreeKind::Oak => {
            let trunk_h = 4 + (local_seed % 3) as i32;
            for dy in 0..trunk_h {
                set_in_chunk(chunk, tx, base_y + dy, tz, Block::Wood);
            }
            let top = base_y + trunk_h - 1;
            for dy in -2_i32..=2_i32 {
                let r: i32 = if dy.abs() == 2 { 1 } else { 2 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() + dz.abs() > r + 1 {
                            continue;
                        }
                        set_in_chunk(chunk, tx + dx, top + dy, tz + dz, Block::Leaves);
                    }
                }
            }
        }
        TreeKind::Pine => {
            let trunk_h = 6 + (local_seed % 3) as i32;
            for dy in 0..trunk_h {
                set_in_chunk(chunk, tx, base_y + dy, tz, Block::Wood);
            }
            let top = base_y + trunk_h;
            for layer in 0..5 {
                let y = top - layer;
                let r = if layer <= 1 { 1_i32 } else { 2_i32 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() + dz.abs() > r + 1 {
                            continue;
                        }
                        set_in_chunk(chunk, tx + dx, y, tz + dz, Block::Leaves);
                    }
                }
            }
            set_in_chunk(chunk, tx, top + 1, tz, Block::Leaves);
        }
    }
}

fn set_in_chunk(chunk: &mut Chunk, wx: i32, wy: i32, wz: i32, block: Block) {
    if !(0..WORLD_HEIGHT as i32).contains(&wy) {
        return;
    }
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
        return;
    }

    let x = lx as usize;
    let y = wy as usize;
    let z = lz as usize;
    let current = chunk.get_local(x, y, z);

    match block {
        Block::Wood => {
            if current == Block::Air || current == Block::Leaves {
                chunk.set_local(x, y, z, Block::Wood);
            }
        }
        Block::Leaves => {
            if current == Block::Air {
                chunk.set_local(x, y, z, Block::Leaves);
            }
        }
        _ => {}
    }
}

#[inline]
fn hash3(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(374_761_393)
        ^ (z as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

#[derive(Copy, Clone)]
struct Face {
    normal: [i32; 3],
    verts: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
    u_axis: [i32; 3],
    v_axis: [i32; 3],
    ao_signs: [[i32; 2]; 4],
}

const FACES: [Face; 6] = [
    Face {
        normal: [1, 0, 0],
        verts: [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
        uvs: [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        u_axis: [0, 1, 0],
        v_axis: [0, 0, 1],
        ao_signs: [[-1, -1], [1, -1], [1, 1], [-1, 1]],
    },
    Face {
        normal: [-1, 0, 0],
        verts: [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0]],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        u_axis: [0, 1, 0],
        v_axis: [0, 0, 1],
        ao_signs: [[-1, -1], [-1, 1], [1, 1], [1, -1]],
    },
    Face {
        normal: [0, 1, 0],
        verts: [[0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        u_axis: [1, 0, 0],
        v_axis: [0, 0, -1],
        ao_signs: [[-1, -1], [1, -1], [1, 1], [-1, 1]],
    },
    Face {
        normal: [0, -1, 0],
        verts: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        u_axis: [1, 0, 0],
        v_axis: [0, 0, 1],
        ao_signs: [[-1, -1], [1, -1], [1, 1], [-1, 1]],
    },
    Face {
        normal: [0, 0, 1],
        verts: [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        u_axis: [1, 0, 0],
        v_axis: [0, 1, 0],
        ao_signs: [[-1, -1], [1, -1], [1, 1], [-1, 1]],
    },
    Face {
        normal: [0, 0, -1],
        verts: [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        u_axis: [-1, 0, 0],
        v_axis: [0, 1, 0],
        ao_signs: [[-1, -1], [1, -1], [1, 1], [-1, 1]],
    },
];
