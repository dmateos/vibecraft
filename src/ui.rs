use bevy::prelude::*;

use crate::generation::{LiveLlmState, PromptInputState};
use crate::interact::PlacementPalette;
use crate::world::TerrainMode;

#[derive(Component)]
pub(crate) struct HudText;

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
                font,
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
}

pub fn update_hud_text(
    palette: Res<PlacementPalette>,
    prompt: Res<PromptInputState>,
    llm: Res<LiveLlmState>,
    terrain_mode: Res<TerrainMode>,
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
        "Terrain: {} (F6 toggle)\nBlock [{} / {}]: {}  |  Wheel=Cycle\nPrompt ({status}): {prompt_preview}\nP=Open Prompt, Enter=Submit, Esc=Close, L/T=Send LLM, J=Load JSON, G=Demo, R/F5=Reseed",
        terrain_mode.label(),
        palette.selected_index() + 1,
        palette.len(),
        palette.selected_name(),
    );
}
