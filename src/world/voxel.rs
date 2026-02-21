use std::collections::HashMap;

use bevy::prelude::*;

use crate::config::{CHUNK_SIZE, WORLD_HEIGHT};

use super::meshing::build_chunk_mesh_lod;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BiomeKind {
    Plains,
    Forest,
    Desert,
    Tundra,
    Rocky,
    Swamp,
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
    pub(super) fn set_biome_local(&mut self, x: usize, z: usize, biome: BiomeKind) {
        let idx = Self::biome_index(x, z);
        self.biomes[idx] = biome;
    }

    #[inline]
    pub(super) fn get_biome_local(&self, x: usize, z: usize) -> BiomeKind {
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
        remesh_chunk(pos, chunks, loaded, meshes);
    }
}

pub fn remesh_chunk(
    pos: IVec2,
    chunks: &HashMap<IVec2, Chunk>,
    loaded: &LoadedChunks,
    meshes: &mut Assets<Mesh>,
) {
    if let Some(render) = loaded.entries.get(&pos)
        && let Some(mesh) = meshes.get_mut(&render.mesh)
    {
        *mesh = build_chunk_mesh_lod(pos, chunks, render.lod);
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
