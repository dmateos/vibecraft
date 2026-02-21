//! UI layer for crosshair, HUD, hotbar inventory, and debug overlays.
//! Spawns text and slot widgets and keeps them synchronized with runtime
//! resources (FPS, mode state, selected block, and inventory counts).
use bevy::prelude::*;
use std::collections::VecDeque;

use crate::config::CHUNK_SIZE;
use crate::generation::{GenerationQueue, GenerationRuntimeStats};
use crate::interact::{BlockInventory, PlacementPalette};
use crate::npc::{Npc, NpcKind, NpcStimulus, NpcUiState, PlayerVitals};
use crate::player::FlyCam;
use crate::streaming::StreamingRuntimeStats;
use crate::weather::WeatherState;
use crate::world::{div_floor, LoadedChunks, TerrainMode, VoxelWorld};

#[derive(Component)]
pub(crate) struct HudText;

#[derive(Component)]
pub(crate) struct DebugHudText;

#[derive(Component)]
pub(crate) struct HotbarSlot {
    index: usize,
}

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
        TextBundle {
            background_color: BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.72)),
            ..TextBundle::from_section(
                "HUD",
                TextStyle {
                    font: font.clone(),
                    font_size: 17.0,
                    color: Color::srgba(0.96, 0.96, 0.96, 0.95),
                },
            )
            .with_style(Style {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(12.0),
                padding: UiRect::all(Val::Px(9.0)),
                ..default()
            })
        },
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

pub fn spawn_hotbar(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    palette: Res<PlacementPalette>,
    inv: Res<BlockInventory>,
) {
    let font = asset_server.load("fonts/DebugSans.ttf");
    commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                bottom: Val::Px(18.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-430.0)),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            },
            background_color: BackgroundColor(Color::NONE),
            ..default()
        })
        .with_children(|parent| {
            for index in 0..palette.len() {
                let (block, name) = palette
                    .entry(index)
                    .expect("hotbar index should always be valid");
                let count = inv.count(block);
                let selected = index == palette.selected_index();
                parent.spawn((
                    TextBundle {
                        background_color: BackgroundColor(if selected {
                            Color::srgba(0.34, 0.38, 0.44, 0.90)
                        } else {
                            Color::srgba(0.07, 0.09, 0.12, 0.78)
                        }),
                        ..TextBundle::from_section(
                            format!("{}\n{}", short_name(name), count),
                            TextStyle {
                                font: font.clone(),
                                font_size: 14.0,
                                color: Color::srgba(0.96, 0.97, 0.99, 0.96),
                            },
                        )
                        .with_style(Style {
                            width: Val::Px(72.0),
                            height: Val::Px(52.0),
                            padding: UiRect::all(Val::Px(6.0)),
                            ..default()
                        })
                    },
                    HotbarSlot { index },
                ));
            }
        });
}

pub fn update_hotbar_ui(
    palette: Res<PlacementPalette>,
    inv: Res<BlockInventory>,
    mut q: Query<(&HotbarSlot, &mut Text, &mut BackgroundColor)>,
) {
    if !palette.is_changed() && !inv.is_changed() {
        return;
    }

    for (slot, mut text, mut bg) in &mut q {
        let Some((block, name)) = palette.entry(slot.index) else {
            continue;
        };
        let count = inv.count(block);
        text.sections[0].value = format!("{}\n{}", short_name(name), count);
        if slot.index == palette.selected_index() {
            *bg = BackgroundColor(Color::srgba(0.34, 0.38, 0.44, 0.90));
        } else {
            *bg = BackgroundColor(Color::srgba(0.07, 0.09, 0.12, 0.78));
        }
    }
}

pub fn update_hud_text(
    palette: Res<PlacementPalette>,
    inv: Res<BlockInventory>,
    terrain_mode: Res<TerrainMode>,
    weather: Res<WeatherState>,
    vitals: Res<PlayerVitals>,
    npc_ui: Res<NpcUiState>,
    frame: Res<FrameStats>,
    mut q: Query<&mut Text, With<HudText>>,
) {
    let Ok(mut text) = q.get_single_mut() else {
        return;
    };

    let selected = palette.selected_block();
    let selected_count = inv.count(selected);

    text.sections[0].value = format!(
        "FPS {:.0} | {:.2} ms\nTerrain: {} (F6) | Weather: {} (F7) | HP: {:.0}/{:.0}\nBlock [{} / {}]: {} x{}  |  Wheel=Cycle\nNPC: {}\nControls: E=Gun/Interact, Q=Grenade, F=Fly, R/F5=Reseed, F8=Day/Night, F3=Debug",
        frame.fps_now,
        frame.avg_ms,
        terrain_mode.label(),
        weather.target.label(),
        vitals.health,
        vitals.max_health,
        palette.selected_index() + 1,
        palette.len(),
        palette.selected_name(),
        selected_count,
        npc_ui.message,
    );
}

fn short_name(name: &str) -> String {
    name.chars().take(6).collect()
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
    npc_stim: Res<NpcStimulus>,
    cam_q: Query<&Transform, With<FlyCam>>,
    npc_q: Query<(&Transform, &Npc)>,
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

    let mut npc_total = 0usize;
    let mut npc_friendly = 0usize;
    let mut npc_hostile = 0usize;
    let mut nearest: Option<(f32, Vec3, NpcKind, bool, f32, f32, f32)> = None;
    for (t, npc) in &npc_q {
        npc_total += 1;
        match npc.kind {
            NpcKind::Friendly => npc_friendly += 1,
            NpcKind::Hostile => npc_hostile += 1,
        }
        let d = cam_pos.distance(t.translation);
        if nearest.map(|n| d < n.0).unwrap_or(true) {
            nearest = Some((
                d,
                t.translation,
                npc.kind,
                npc.follow_player,
                npc.last_seen_timer,
                npc.investigate_timer,
                npc.stuck_timer,
            ));
        }
    }
    let nearest_line = if let Some((dist, pos, kind, follow, seen_t, hear_t, stuck_t)) = nearest {
        let kind_str = match kind {
            NpcKind::Friendly => "Friendly",
            NpcKind::Hostile => "Hostile",
        };
        let mode = match kind {
            NpcKind::Friendly => {
                if follow {
                    "Follow"
                } else {
                    "Wander"
                }
            }
            NpcKind::Hostile => {
                if seen_t > 0.05 {
                    "Chase(LOS)"
                } else if hear_t > 0.05 {
                    "Investigate(sound)"
                } else {
                    "Patrol"
                }
            }
        };
        format!(
            "Nearest NPC: {} @ {:.1}m mode={} seen={:.1}s hear={:.1}s stuck={:.2}s pos=({:.1},{:.1},{:.1})",
            kind_str, dist, mode, seen_t, hear_t, stuck_t, pos.x, pos.y, pos.z
        )
    } else {
        "Nearest NPC: none".to_string()
    };

    text.sections[0].value = format!(
        "DEBUG [F3]\nFPS now: {:.1} | Avg frame: {:.2} ms | 1% low proxy: {:.1} fps\nChunks loaded(world/render): {}/{} | desired: {} | generated tick: {} | meshed tick: {}\nGeneration queue plans: {} | ops applied tick: {} | plans completed: {}\nCam pos: ({:.1}, {:.1}, {:.1}) | Cam chunk: ({}, {})\nNPCs total/friendly/hostile: {}/{}/{} | noise ttl: {:.1}s @ ({:.1},{:.1},{:.1})\n{}",
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
        npc_total,
        npc_friendly,
        npc_hostile,
        npc_stim.ttl,
        npc_stim.loud_pos.x,
        npc_stim.loud_pos.y,
        npc_stim.loud_pos.z,
        nearest_line,
    );
}
