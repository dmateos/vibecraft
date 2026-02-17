use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;

use crate::generation::compiler::CompiledPlan;
use crate::world::{
    remesh_affected_chunks, set_block_world, LoadedChunks, VoxelWorld,
};

#[derive(Resource)]
pub struct GenerationConfig {
    pub max_block_edits_per_tick: usize,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            max_block_edits_per_tick: 6000,
        }
    }
}

#[derive(Resource, Default)]
pub struct GenerationQueue {
    pub pending: VecDeque<RunningPlan>,
}

#[derive(Debug, Clone)]
pub struct RunningPlan {
    pub plan: CompiledPlan,
    pub cursor: usize,
}

pub fn enqueue_plan(queue: &mut GenerationQueue, plan: CompiledPlan) {
    queue.pending.push_back(RunningPlan { plan, cursor: 0 });
}

pub fn process_generation_queue(
    config: Res<GenerationConfig>,
    mut queue: ResMut<GenerationQueue>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(front) = queue.pending.front_mut() else {
        return;
    };

    let start = front.cursor;
    let end = (start + config.max_block_edits_per_tick).min(front.plan.edits.len());
    let mut changed_chunks = HashSet::new();

    for edit in &front.plan.edits[start..end] {
        if set_block_world(&mut world.chunks, edit.pos.x, edit.pos.y, edit.pos.z, edit.block) {
            changed_chunks.insert(IVec2::new(
                crate::world::div_floor(edit.pos.x, crate::config::CHUNK_SIZE as i32),
                crate::world::div_floor(edit.pos.z, crate::config::CHUNK_SIZE as i32),
            ));
        }
    }

    for c in changed_chunks {
        remesh_affected_chunks(c, &world.chunks, &loaded, &mut meshes);
    }

    front.cursor = end;
    if front.cursor >= front.plan.edits.len() {
        info!(
            "generation plan applied: id={} source={} edits={} chunks={}",
            front.plan.request_id,
            front.plan.source,
            front.plan.edits.len(),
            front.plan.touched_chunks.len()
        );
        queue.pending.pop_front();
    }
}
