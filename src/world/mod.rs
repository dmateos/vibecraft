use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;
use noise::{NoiseFn, Perlin};

use crate::config::{CHUNK_SIZE, SEA_LEVEL, WORLD_HEIGHT};

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
    Sand,
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

pub fn generate_chunk(pos: IVec2, seed: u32) -> Chunk {
    let mut chunk = Chunk::new(pos);
    let perlin = Perlin::new(seed);

    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = base_x + x as i32;
            let wz = base_z + z as i32;

            let n1 = perlin.get([wx as f64 * 0.008, wz as f64 * 0.008]) as f32;
            let n2 = perlin.get([wx as f64 * 0.021, wz as f64 * 0.021]) as f32;
            let n3 = perlin.get([wx as f64 * 0.004, wz as f64 * 0.004]) as f32;

            let mut height = SEA_LEVEL as f32 + n1 * 22.0 + n2 * 9.0 + n3 * 30.0;
            height = height.clamp(6.0, (WORLD_HEIGHT - 1) as f32);
            let h = height as usize;

            for y in 0..=h {
                let block = if y == h {
                    if y as i32 <= SEA_LEVEL + 1 {
                        Block::Sand
                    } else {
                        Block::Grass
                    }
                } else if y + 4 > h {
                    if y as i32 <= SEA_LEVEL {
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
    }
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
