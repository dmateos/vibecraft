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
}

impl Chunk {
    pub fn new(pos: IVec2) -> Self {
        Self {
            pos,
            voxels: vec![Block::Air; CHUNK_SIZE * WORLD_HEIGHT * CHUNK_SIZE],
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

            let (h, biome, ridge) = sample_surface(&noise, wx, wz);

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
                        0.62 + ((yi as f32 / WORLD_HEIGHT as f32) - 0.5) * 0.06 + ridge.abs() * 0.05;
                    if cave > cave_threshold {
                        continue;
                    }
                }

                let block = if is_surface {
                    if yi <= SEA_LEVEL || biome < -0.62 {
                        Block::Sand
                    } else if yi >= SEA_LEVEL + 46 {
                        Block::Snow
                    } else {
                        Block::Grass
                    }
                } else if is_subsurface {
                    if yi <= SEA_LEVEL - 1 || biome < -0.55 {
                        Block::Sand
                    } else {
                        Block::Dirt
                    }
                } else {
                    Block::Stone
                };
                chunk.set_local(x, y, z, block);
            }
        }
    }

    stamp_trees(&mut chunk, &noise);
    chunk
}

fn generate_flat_chunk(pos: IVec2) -> Chunk {
    let mut chunk = Chunk::new(pos);
    let surface = SEA_LEVEL + 1;
    let dirt_depth = 4;

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
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

pub fn build_chunk_mesh(pos: IVec2, chunks: &HashMap<IVec2, Chunk>) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
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
                    let color = block_color(block);

                    for v in face.verts {
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
                        colors.push(color);
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
            *mesh = build_chunk_mesh(pos, chunks);
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

fn block_color(block: Block) -> [f32; 4] {
    match block {
        Block::Air => [0.0, 0.0, 0.0, 0.0],
        Block::Grass => [0.24, 0.68, 0.25, 1.0],
        Block::Dirt => [0.44, 0.31, 0.18, 1.0],
        Block::Stone => [0.48, 0.48, 0.52, 1.0],
        Block::Sand => [0.83, 0.77, 0.53, 1.0],
        Block::Snow => [0.88, 0.91, 0.95, 1.0],
        Block::Wood => [0.41, 0.30, 0.18, 1.0],
        Block::Leaves => [0.20, 0.46, 0.21, 1.0],
        Block::Red => [0.84, 0.20, 0.20, 1.0],
        Block::Blue => [0.20, 0.34, 0.84, 1.0],
        Block::Yellow => [0.90, 0.82, 0.20, 1.0],
        Block::Purple => [0.58, 0.30, 0.82, 1.0],
        Block::Cyan => [0.20, 0.74, 0.78, 1.0],
    }
}

struct TerrainNoise {
    seed: u32,
    perlin_macro: Perlin,
    perlin_detail: Perlin,
    perlin_ridge: Perlin,
    perlin_biome: Perlin,
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
            perlin_continent: Perlin::new(seed ^ 0x6F12_BD9A),
            perlin_cave: Perlin::new(seed ^ 0x7F4A_7C15),
            perlin_cave_warp: Perlin::new(seed ^ 0xB529_7A4D),
        }
    }
}

#[inline]
fn sample_surface(noise: &TerrainNoise, wx: i32, wz: i32) -> (usize, f32, f32) {
    let n1 = noise.perlin_macro.get([wx as f64 * 0.008, wz as f64 * 0.008]) as f32;
    let n2 = noise.perlin_detail.get([wx as f64 * 0.024, wz as f64 * 0.024]) as f32;
    let n3 = noise.perlin_macro.get([wx as f64 * 0.0035, wz as f64 * 0.0035]) as f32;
    let ridge = noise.perlin_ridge.get([wx as f64 * 0.015, wz as f64 * 0.015]) as f32;
    let biome = noise.perlin_biome.get([wx as f64 * 0.0024, wz as f64 * 0.0024]) as f32;
    let continent = noise
        .perlin_continent
        .get([wx as f64 * 0.0017, wz as f64 * 0.0017]) as f32;
    let ridge_abs = ridge.abs();
    let cliff_mask = (ridge_abs - 0.58).max(0.0);
    let cliff = cliff_mask * cliff_mask * 62.0;
    let continental = ((continent + 1.0) * 0.5).powf(1.3);
    let base = SEA_LEVEL as f32 - 20.0 + continental * 50.0;
    let mut height = base + n1 * 16.0 + n2 * 7.0 + n3 * 20.0 + cliff + biome * 6.0;
    height = height.clamp(6.0, (WORLD_HEIGHT - 2) as f32);
    (height as usize, biome, ridge)
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

            let (surface_y, biome, ridge) = sample_surface(noise, tx, tz);
            let sy = surface_y as i32;
            if sy <= SEA_LEVEL + 2 {
                continue;
            }
            if biome < -0.48 {
                continue;
            }
            if ridge.abs() > 0.62 {
                continue;
            }
            if (h >> 16) & 0xFF < 92 {
                continue;
            }

            let kind = if biome > 0.30 || sy >= SEA_LEVEL + 42 {
                TreeKind::Pine
            } else {
                TreeKind::Oak
            };

            let local_seed = hash3(tx, tz, noise.seed ^ 0x91C2_8E4B);
            place_tree(chunk, tx, sy + 1, tz, kind, local_seed);
        }
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
}

const FACES: [Face; 6] = [
    Face {
        normal: [1, 0, 0],
        verts: [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
    },
    Face {
        normal: [-1, 0, 0],
        verts: [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0]],
    },
    Face {
        normal: [0, 1, 0],
        verts: [[0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
    },
    Face {
        normal: [0, -1, 0],
        verts: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
    },
    Face {
        normal: [0, 0, 1],
        verts: [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
    },
    Face {
        normal: [0, 0, -1],
        verts: [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
    },
];
