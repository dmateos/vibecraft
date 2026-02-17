mod compiler;
mod executor;
mod live;
mod schema;
mod validator;

use std::fs;

use bevy::prelude::*;

use crate::generation::compiler::compile_request;
use crate::generation::executor::enqueue_plan;
use crate::generation::schema::{GenerationOp, GenerationRequest};
use crate::generation::validator::validate_request;
use crate::player::FlyCam;
use crate::world::VoxelWorld;

pub use executor::{process_generation_queue, GenerationConfig, GenerationQueue};
pub use live::{
    edit_prompt_input, initialize_prompt_input, poll_live_llm_result, toggle_prompt_input_mode,
    trigger_live_llm_generation_on_key, update_prompt_window_title, LiveLlmState, PromptInputState,
};

pub fn submit_request(
    req: GenerationRequest,
    world: &VoxelWorld,
    queue: &mut executor::GenerationQueue,
) -> Result<(), String> {
    validate_request(&req)?;
    let plan = compile_request(&req, world)?;
    enqueue_plan(queue, plan);
    Ok(())
}

pub fn submit_request_json(
    json: &str,
    world: &VoxelWorld,
    queue: &mut executor::GenerationQueue,
) -> Result<(), String> {
    let req: GenerationRequest =
        serde_json::from_str(json).map_err(|e| format!("invalid request json: {e}"))?;
    submit_request(req, world, queue)
}

pub fn trigger_demo_generation_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
    mut queue: ResMut<GenerationQueue>,
    prompt: Res<PromptInputState>,
) {
    if prompt.active {
        return;
    }
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let base = cam.translation + *cam.forward() * 24.0;
    let px = base.x.floor() as i32;
    let pz = base.z.floor() as i32;

    let req = GenerationRequest {
        version: "1".to_string(),
        request_id: format!("demo-{}", world.seed),
        source: "demo_hotkey".to_string(),
        ops: vec![
            GenerationOp::PaintRegion {
                shape: "circle".to_string(),
                center: [px, pz],
                radius: 12,
                surface_block: "grass".to_string(),
                seed: world.seed ^ 101,
            },
            GenerationOp::PlacePrefab {
                prefab: "stone_ring".to_string(),
                position: [px, crate::config::SEA_LEVEL + 2, pz],
                rotation: 0,
                seed: world.seed ^ 202,
            },
            GenerationOp::PlacePrefab {
                prefab: "oak_tree_small".to_string(),
                position: [px + 6, crate::config::SEA_LEVEL + 2, pz + 4],
                rotation: 90,
                seed: world.seed ^ 303,
            },
            GenerationOp::PlacePrefab {
                prefab: "pine_tree_large".to_string(),
                position: [px - 6, crate::config::SEA_LEVEL + 2, pz - 4],
                rotation: 180,
                seed: world.seed ^ 404,
            },
        ],
    };

    match submit_request(req, &world, &mut queue) {
        Ok(()) => info!("queued demo generation plan"),
        Err(e) => warn!("demo generation rejected: {e}"),
    }
}

pub fn load_generation_request_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    world: Res<VoxelWorld>,
    mut queue: ResMut<GenerationQueue>,
    prompt: Res<PromptInputState>,
) {
    if prompt.active {
        return;
    }
    if !keys.just_pressed(KeyCode::KeyJ) {
        return;
    }

    let path = "assets/generation/request.json";
    let json = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to read {path}: {e}");
            return;
        }
    };

    match submit_request_json(&json, &world, &mut queue) {
        Ok(()) => info!("queued generation request from {path}"),
        Err(e) => warn!("generation request rejected ({path}): {e}"),
    }
}
