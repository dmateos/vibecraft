//! Unified block mutation pipeline.
//! Collects edit requests from gameplay/network systems, applies voxel writes,
//! emits local edit notifications, and remeshes affected chunks once per tick.
use std::collections::HashSet;

use bevy::prelude::*;

use crate::config::CHUNK_SIZE;
use crate::world::{
    div_floor, get_block_world, remesh_affected_chunks, set_block_world, Block, LoadedChunks,
    VoxelWorld,
};

#[derive(Event, Clone, Copy, Debug)]
pub struct LocalBlockEditEvent {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: Block,
}

#[derive(Event, Clone, Copy, Debug)]
pub struct BlockMutationRequest {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: Block,
    pub emit_local_event: bool,
}

pub fn enqueue_block_mutation(
    edits: &mut EventWriter<BlockMutationRequest>,
    x: i32,
    y: i32,
    z: i32,
    block: Block,
    emit_local_event: bool,
) {
    edits.send(BlockMutationRequest {
        x,
        y,
        z,
        block,
        emit_local_event,
    });
}

pub fn apply_block_mutations(
    mut requests: EventReader<BlockMutationRequest>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut local_events: EventWriter<LocalBlockEditEvent>,
) {
    let mut touched_chunks = HashSet::new();
    for req in requests.read() {
        if get_block_world(&world.chunks, req.x, req.y, req.z) == req.block {
            continue;
        }
        if !set_block_world(&mut world.chunks, req.x, req.y, req.z, req.block) {
            continue;
        }
        if req.emit_local_event {
            local_events.send(LocalBlockEditEvent {
                x: req.x,
                y: req.y,
                z: req.z,
                block: req.block,
            });
        }
        touched_chunks.insert(IVec2::new(
            div_floor(req.x, CHUNK_SIZE as i32),
            div_floor(req.z, CHUNK_SIZE as i32),
        ));
    }

    for chunk in touched_chunks {
        remesh_affected_chunks(chunk, &world.chunks, &loaded, &mut meshes);
    }
}
