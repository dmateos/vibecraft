use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;

use crate::config::{CHUNK_SIZE, WORLD_HEIGHT};

use super::{BiomeKind, Block, Chunk, div_floor, get_block_world};
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
