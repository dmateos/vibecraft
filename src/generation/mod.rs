//! Generation pipeline root: schema, planning, validation, compile, execute.
//! Exposes user-triggered entry points (demo/json/live LLM) while keeping
//! each stage isolated so behavior and safety rules can evolve independently.
mod compiler;
mod executor;
mod live;
mod planner;
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

pub use executor::{
    process_generation_queue, GenerationConfig, GenerationQueue, GenerationRuntimeStats,
};
pub use live::{
    edit_prompt_input, initialize_prompt_input, poll_live_llm_result, toggle_prompt_input_mode,
    trigger_live_llm_generation_on_key, update_prompt_window_title, LiveLlmState, PromptInputState,
};
pub use planner::build_structured_plan;

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
                radius: 16,
                surface_block: "grass".to_string(),
                seed: world.seed ^ 101,
            },
            GenerationOp::HollowBox {
                min: [px - 8, crate::config::SEA_LEVEL + 2, pz - 8],
                max: [px + 8, crate::config::SEA_LEVEL + 14, pz + 8],
                wall_block: "castle_stone".to_string(),
                wall_thickness: 1,
                floor_block: Some("castle_floor".to_string()),
                roof_block: Some("castle_trim".to_string()),
            },
            GenerationOp::HollowBox {
                min: [px - 18, crate::config::SEA_LEVEL + 2, pz - 18],
                max: [px + 18, crate::config::SEA_LEVEL + 10, pz + 18],
                wall_block: "castle_stone".to_string(),
                wall_thickness: 1,
                floor_block: Some("castle_floor".to_string()),
                roof_block: None,
            },
            GenerationOp::Cylinder {
                center: [px - 18, crate::config::SEA_LEVEL + 2, pz - 18],
                radius: 3,
                height: 16,
                block: "castle_trim".to_string(),
                hollow: true,
            },
            GenerationOp::Cylinder {
                center: [px + 18, crate::config::SEA_LEVEL + 2, pz - 18],
                radius: 3,
                height: 16,
                block: "castle_trim".to_string(),
                hollow: true,
            },
            GenerationOp::Cylinder {
                center: [px - 18, crate::config::SEA_LEVEL + 2, pz + 18],
                radius: 3,
                height: 16,
                block: "castle_trim".to_string(),
                hollow: true,
            },
            GenerationOp::Cylinder {
                center: [px + 18, crate::config::SEA_LEVEL + 2, pz + 18],
                radius: 3,
                height: 16,
                block: "castle_trim".to_string(),
                hollow: true,
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
