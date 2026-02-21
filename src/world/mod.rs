//! Core voxel world model split into focused modules.

mod generation;
mod landmarks;
mod meshing;
mod voxel;

pub use generation::generate_chunk;
pub use meshing::build_chunk_mesh_lod;
pub use voxel::{
    Block, BiomeKind, Chunk, ChunkRender, LoadedChunks, StreamTimer, TerrainMode, VoxelWorld,
    chunk_distance_sq, div_floor, get_block_world, remesh_affected_chunks, set_block_world,
};
