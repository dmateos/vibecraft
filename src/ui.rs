use bevy::prelude::*;

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
