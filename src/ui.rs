use bevy::prelude::*;
use std::collections::VecDeque;

use crate::config::CHUNK_SIZE;
use crate::generation::{GenerationQueue, GenerationRuntimeStats, LiveLlmState, PromptInputState};
use crate::interact::PlacementPalette;
use crate::player::FlyCam;
use crate::streaming::StreamingRuntimeStats;
use crate::weather::WeatherState;
use crate::world::{div_floor, LoadedChunks, TerrainMode, VoxelWorld};

#[derive(Component)]
pub(crate) struct HudText;

#[derive(Component)]
pub(crate) struct DebugHudText;

#[derive(Resource, Default)]
pub struct DebugOverlayState {
    pub visible: bool,
}

#[derive(Resource)]
pub struct FrameStats {
    samples_ms: VecDeque<f32>,
    pub avg_ms: f32,
    pub fps_now: f32,
    pub low_1pct_fps_proxy: f32,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            samples_ms: VecDeque::with_capacity(240),
            avg_ms: 0.0,
            fps_now: 0.0,
            low_1pct_fps_proxy: 0.0,
        }
    }
}

pub fn spawn_crosshair(mut commands: Commands) {
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            background_color: BackgroundColor(Color::NONE),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Px(18.0),
                    height: Val::Px(2.0),
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                ..default()
            });
            parent.spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Px(2.0),
                    height: Val::Px(18.0),
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                ..default()
            });
        });
}

pub fn spawn_hud(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/DebugSans.ttf");
    commands.spawn((
        TextBundle::from_section(
            "HUD",
            TextStyle {
                font: font.clone(),
                font_size: 16.0,
                color: Color::srgba(0.96, 0.96, 0.96, 0.95),
            },
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(12.0),
            ..default()
        }),
        HudText,
    ));

    commands.spawn((
        TextBundle::from_section(
            "DEBUG",
            TextStyle {
                font,
                font_size: 15.0,
                color: Color::srgba(0.95, 0.98, 1.0, 0.95),
            },
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(12.0),
            display: Display::None,
            ..default()
        }),
        DebugHudText,
    ));
}

pub fn update_hud_text(
    palette: Res<PlacementPalette>,
    prompt: Res<PromptInputState>,
    llm: Res<LiveLlmState>,
    terrain_mode: Res<TerrainMode>,
    weather: Res<WeatherState>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let Ok(mut text) = q.get_single_mut() else {
        return;
    };

    let prompt_preview = if prompt.buffer.is_empty() {
        "(empty)".to_string()
    } else {
        let chars: Vec<char> = prompt.buffer.chars().collect();
        if chars.len() > 72 {
            chars[..72].iter().collect::<String>() + "..."
        } else {
            prompt.buffer.clone()
        }
    };

    let status = if prompt.active {
        "editing"
    } else if llm.in_flight {
        "sending"
    } else {
        "ready"
    };

    text.sections[0].value = format!(
        "Terrain: {} (F6) | Weather: {} (F7)\nBlock [{} / {}]: {}  |  Wheel=Cycle\nPrompt ({status}): {prompt_preview}\nP=Open Prompt, Enter=Submit, Esc=Close, L/T=Send LLM, J=Load JSON, G=Demo, R/F5=Reseed, F3=Debug",
        terrain_mode.label(),
        weather.target.label(),
        palette.selected_index() + 1,
        palette.len(),
        palette.selected_name(),
    );
}

pub fn toggle_debug_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<DebugOverlayState>,
) {
    if keys.just_pressed(KeyCode::F3) {
        overlay.visible = !overlay.visible;
    }
}

pub fn sample_frame_stats(time: Res<Time>, mut stats: ResMut<FrameStats>) {
    let dt = time.delta_seconds().max(0.0001);
    let ms = dt * 1000.0;
    stats.fps_now = 1.0 / dt;
    stats.samples_ms.push_back(ms);
    while stats.samples_ms.len() > 240 {
        let _ = stats.samples_ms.pop_front();
    }

    if stats.samples_ms.is_empty() {
        stats.avg_ms = ms;
        stats.low_1pct_fps_proxy = stats.fps_now;
        return;
    }

    let sum: f32 = stats.samples_ms.iter().sum();
    stats.avg_ms = sum / stats.samples_ms.len() as f32;

    let mut sorted = stats.samples_ms.iter().copied().collect::<Vec<_>>();
    sorted.sort_by(f32::total_cmp);
    let idx = ((sorted.len() as f32) * 0.99).floor() as usize;
    let sample = sorted[idx.min(sorted.len().saturating_sub(1))].max(0.0001);
    stats.low_1pct_fps_proxy = 1000.0 / sample;
}

pub fn update_debug_hud_text(
    overlay: Res<DebugOverlayState>,
    frame: Res<FrameStats>,
    loaded_chunks: Res<LoadedChunks>,
    world: Res<VoxelWorld>,
    streaming_stats: Res<StreamingRuntimeStats>,
    gen_queue: Res<GenerationQueue>,
    gen_stats: Res<GenerationRuntimeStats>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut q: Query<(&mut Text, &mut Style), With<DebugHudText>>,
) {
    let Ok((mut text, mut style)) = q.get_single_mut() else {
        return;
    };

    style.display = if overlay.visible {
        Display::Flex
    } else {
        Display::None
    };
    if !overlay.visible {
        return;
    }

    let (cam_pos, cam_chunk) = if let Ok(cam) = cam_q.get_single() {
        let cx = div_floor(cam.translation.x.floor() as i32, CHUNK_SIZE as i32);
        let cz = div_floor(cam.translation.z.floor() as i32, CHUNK_SIZE as i32);
        (cam.translation, IVec2::new(cx, cz))
    } else {
        (Vec3::ZERO, IVec2::ZERO)
    };

    text.sections[0].value = format!(
        "DEBUG [F3]\nFPS now: {:.1} | Avg frame: {:.2} ms | 1% low proxy: {:.1} fps\nChunks loaded(world/render): {}/{} | desired: {} | generated tick: {} | meshed tick: {}\nGeneration queue plans: {} | ops applied tick: {} | plans completed: {}\nCam pos: ({:.1}, {:.1}, {:.1}) | Cam chunk: ({}, {})",
        frame.fps_now,
        frame.avg_ms,
        frame.low_1pct_fps_proxy,
        world.chunks.len(),
        loaded_chunks.entries.len(),
        streaming_stats.desired_chunks,
        streaming_stats.generated_last_tick,
        streaming_stats.meshed_last_tick,
        gen_queue.pending.len(),
        gen_stats.ops_applied_last_tick,
        gen_stats.plans_completed_total,
        cam_pos.x,
        cam_pos.y,
        cam_pos.z,
        cam_chunk.x,
        cam_chunk.y,
    );
}
