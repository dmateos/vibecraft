use std::env;
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::thread;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use serde_json::Value;

use crate::config::WORLD_HEIGHT;
use crate::generation::schema::{GenerationOp, GenerationRequest};
use crate::generation::{submit_request, GenerationQueue};
use crate::player::FlyCam;
use crate::world::{get_block_world, Chunk, VoxelWorld};

#[derive(Resource, Default)]
pub struct LiveLlmState {
    pub in_flight: bool,
    rx: Mutex<Option<Receiver<Result<String, String>>>>,
    pending_anchor: Option<IVec3>,
}

#[derive(Resource, Default)]
pub struct PromptInputState {
    pub active: bool,
    pub buffer: String,
    pub pending_submit: bool,
}

pub fn initialize_prompt_input(mut prompt: ResMut<PromptInputState>) {
    if !prompt.buffer.is_empty() {
        return;
    }
    prompt.buffer = "Create a small scenic campsite with colorful accents.".to_string();
}

pub fn toggle_prompt_input_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut prompt: ResMut<PromptInputState>,
    mut windows: Query<&mut Window>,
) {
    if !keys.just_pressed(KeyCode::KeyP) {
        return;
    }

    prompt.active = !prompt.active;
    if let Ok(mut window) = windows.get_single_mut() {
        if prompt.active {
            window.cursor.visible = true;
            window.cursor.grab_mode = CursorGrabMode::None;
        } else {
            window.cursor.visible = false;
            window.cursor.grab_mode = CursorGrabMode::Locked;
        }
    }
}

pub fn edit_prompt_input(
    mut key_events: EventReader<KeyboardInput>,
    mut prompt: ResMut<PromptInputState>,
    mut windows: Query<&mut Window>,
) {
    if !prompt.active {
        key_events.clear();
        return;
    }

    for ev in key_events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }

        if ev.key_code == KeyCode::KeyP {
            continue;
        }

        match &ev.logical_key {
            Key::Character(chars) => {
                prompt.buffer.push_str(chars);
            }
            Key::Space => {
                prompt.buffer.push(' ');
            }
            Key::Backspace => {
                prompt.buffer.pop();
            }
            Key::Enter => {
                prompt.pending_submit = true;
                prompt.active = false;
                if let Ok(mut window) = windows.get_single_mut() {
                    window.cursor.visible = false;
                    window.cursor.grab_mode = CursorGrabMode::Locked;
                }
            }
            Key::Escape => {
                prompt.active = false;
                if let Ok(mut window) = windows.get_single_mut() {
                    window.cursor.visible = false;
                    window.cursor.grab_mode = CursorGrabMode::Locked;
                }
            }
            _ => {}
        }
    }
}

pub fn update_prompt_window_title(
    prompt: Res<PromptInputState>,
    llm: Res<LiveLlmState>,
    mut windows: Query<&mut Window>,
) {
    let Ok(mut window) = windows.get_single_mut() else {
        return;
    };

    if prompt.active {
        let preview = if prompt.buffer.is_empty() {
            "(empty)".to_string()
        } else {
            let chars: Vec<char> = prompt.buffer.chars().collect();
            if chars.len() > 42 {
                chars[..42].iter().collect::<String>() + "..."
            } else {
                prompt.buffer.clone()
            }
        };
        window.title = format!("VibeCraft [PROMPT] {preview}");
        return;
    }

    if llm.in_flight {
        window.title = "VibeCraft [LLM sending...]".to_string();
    } else {
        window.title = "VibeCraft".to_string();
    }
}

pub fn trigger_live_llm_generation_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
    mut state: ResMut<LiveLlmState>,
    mut prompt: ResMut<PromptInputState>,
) {
    let triggered =
        keys.just_pressed(KeyCode::KeyL) || keys.just_pressed(KeyCode::KeyT) || prompt.pending_submit;
    if !triggered {
        return;
    }
    if prompt.active && !prompt.pending_submit {
        return;
    }
    prompt.pending_submit = false;

    if state.in_flight {
        info!("LLM request already in flight");
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };
    let anchor = compute_generation_anchor(cam.translation, *cam.forward(), &world);

    let user_prompt = if prompt.buffer.trim().is_empty() {
        "Create a small scenic campsite with colorful accents.".to_string()
    } else {
        prompt.buffer.clone()
    };

    let api_key = match env::var("OPENAI_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            warn!("OPENAI_API_KEY is not set");
            return;
        }
    };

    let api_url = env::var("VIBECRAFT_LLM_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
    let model = env::var("VIBECRAFT_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

    let context = format!(
        "World seed: {}. Target anchor (world): [{}, {}, {}]. Use LOCAL offsets around [0,0,0] for all positions and centers; engine translates to target anchor.",
        world.seed,
        anchor.x,
        anchor.y,
        anchor.z
    );

    let schema_hint = r#"Return ONLY valid JSON matching this shape:
{
  "version":"1",
  "request_id":"string",
  "source":"llm_live",
  "ops":[
                    {"type":"place_block","position":[x,y,z],"block":"grass|dirt|stone|sand|snow|wood|leaves|red|blue|yellow|purple|cyan"},
                    {"type":"place_prefab","prefab":"oak_tree_small|pine_tree_large|stone_ring","position":[x,y,z],"rotation":0,"seed":0},
                    {"type":"paint_region","shape":"circle","center":[x,z],"radius":12,"surface_block":"grass|dirt|stone|sand|snow|wood|leaves|red|blue|yellow|purple|cyan","seed":0}
                ]
}
All coordinates must be local offsets near origin (around -32..32), not absolute world coordinates.
No markdown fences. No explanation text."#;

    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    state.in_flight = true;
    if let Ok(mut lock) = state.rx.lock() {
        *lock = Some(rx);
    } else {
        warn!("LLM state mutex poisoned");
        state.in_flight = false;
        return;
    }

    state.pending_anchor = Some(anchor);

    thread::spawn(move || {
        let client = reqwest::blocking::Client::new();
        let payload = serde_json::json!({
            "model": model,
            "temperature": 0.4,
            "messages": [
                {"role":"system","content":"You generate compact voxel-world edit plans."},
                {"role":"user","content": format!("{}\n\n{}\n\n{}", schema_hint, context, user_prompt)}
            ]
        });

        let result = (|| -> Result<String, String> {
            let response = client
                .post(&api_url)
                .bearer_auth(api_key)
                .json(&payload)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;

            let status = response.status();
            let text = response
                .text()
                .map_err(|e| format!("failed to read response: {e}"))?;

            if !status.is_success() {
                return Err(format!("llm http {}: {}", status, text));
            }

            extract_content_from_chat_completions(&text)
        })();

        let _ = tx.send(result);
    });
}

pub fn poll_live_llm_result(
    mut state: ResMut<LiveLlmState>,
    world: Res<VoxelWorld>,
    mut queue: ResMut<GenerationQueue>,
) {
    let maybe_result = {
        let Ok(lock) = state.rx.lock() else {
            warn!("LLM state mutex poisoned");
            return;
        };
        let Some(rx) = lock.as_ref() else {
            return;
        };
        rx.try_recv()
    };

    match maybe_result {
        Ok(Ok(content)) => {
            let json = extract_json_object(&content).unwrap_or(content);
            let anchor = state.pending_anchor.take().unwrap_or(IVec3::ZERO);
            let mut req: GenerationRequest = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(e) => {
                    warn!("LLM plan rejected: invalid request json: {e}");
                    state.in_flight = false;
                    if let Ok(mut lock) = state.rx.lock() {
                        *lock = None;
                    }
                    return;
                }
            };
            apply_anchor_to_request(&mut req, anchor);
            match submit_request(req, &world, &mut queue) {
                Ok(()) => info!("queued LLM generation plan"),
                Err(e) => warn!("LLM plan rejected: {e}"),
            }
            state.in_flight = false;
            if let Ok(mut lock) = state.rx.lock() {
                *lock = None;
            }
        }
        Ok(Err(e)) => {
            warn!("LLM generation request failed: {e}");
            state.in_flight = false;
            state.pending_anchor = None;
            if let Ok(mut lock) = state.rx.lock() {
                *lock = None;
            }
        }
        Err(mpsc::TryRecvError::Empty) => {}
        Err(mpsc::TryRecvError::Disconnected) => {
            warn!("LLM worker disconnected");
            state.in_flight = false;
            state.pending_anchor = None;
            if let Ok(mut lock) = state.rx.lock() {
                *lock = None;
            }
        }
    }
}

fn extract_content_from_chat_completions(text: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("invalid json response: {e}"))?;
    let content = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "missing choices[0].message.content".to_string())?;

    Ok(content.to_string())
}

fn extract_json_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(s[start..=end].to_string())
}

fn apply_anchor_to_request(req: &mut GenerationRequest, anchor: IVec3) {
    for op in &mut req.ops {
        match op {
            GenerationOp::PlaceBlock { position, .. } => {
                position[0] += anchor.x;
                position[1] += anchor.y;
                position[2] += anchor.z;
            }
            GenerationOp::PlacePrefab { position, .. } => {
                position[0] += anchor.x;
                position[1] += anchor.y;
                position[2] += anchor.z;
            }
            GenerationOp::PaintRegion { center, .. } => {
                center[0] += anchor.x;
                center[1] += anchor.z;
            }
        }
    }
}

fn compute_generation_anchor(origin: Vec3, dir: Vec3, world: &VoxelWorld) -> IVec3 {
    if let Some(hit) = raycast_previous_air(origin, dir, &world.chunks, 96.0) {
        return hit;
    }

    let fallback = origin + dir * 24.0;
    IVec3::new(
        fallback.x.floor() as i32,
        fallback.y.floor().clamp(4.0, (WORLD_HEIGHT as f32 - 4.0).max(4.0)) as i32,
        fallback.z.floor() as i32,
    )
}

fn raycast_previous_air(
    origin: Vec3,
    dir: Vec3,
    chunks: &std::collections::HashMap<IVec2, Chunk>,
    max_dist: f32,
) -> Option<IVec3> {
    let step = 0.15;
    let mut t = 0.0;
    let mut last_cell = IVec3::new(i32::MIN, i32::MIN, i32::MIN);
    let mut last_air = IVec3::new(i32::MIN, i32::MIN, i32::MIN);

    while t <= max_dist {
        let p = origin + dir * t;
        let cell = IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if cell == last_cell {
            t += step;
            continue;
        }
        last_cell = cell;

        if get_block_world(chunks, cell.x, cell.y, cell.z) == crate::world::Block::Air {
            last_air = cell;
            t += step;
            continue;
        }

        if last_air.x != i32::MIN {
            return Some(last_air);
        }

        return Some(cell + IVec3::Y);
    }

    None
}
