mod clouds;
mod config;
mod interact;
mod materials;
mod player;
mod streaming;
mod ui;
mod water;
mod world;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;

use config::SEA_LEVEL;
use materials::{TerrainMaterial, VoxelMaterial, VoxelMaterialParams};
use player::FlyCam;
use world::{LoadedChunks, StreamTimer, VoxelWorld};

fn main() {
    App::new()
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::srgb(0.55, 0.8, 0.98)))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 180.0,
        })
        .insert_resource(VoxelWorld {
            seed: 1337,
            chunks: HashMap::new(),
        })
        .insert_resource(LoadedChunks::default())
        .insert_resource(StreamTimer(Timer::from_seconds(0.05, TimerMode::Repeating)))
        .insert_resource(clouds::LoadedClouds::default())
        .insert_resource(clouds::CloudStreamTimer(Timer::from_seconds(
            0.12,
            TimerMode::Repeating,
        )))
        .insert_resource(water::LoadedWater::default())
        .insert_resource(water::WaterStreamTimer(Timer::from_seconds(
            0.08,
            TimerMode::Repeating,
        )))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "VibeCraft".to_string(),
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(MaterialPlugin::<VoxelMaterial>::default())
        .add_plugins(water::water_material_plugin())
        .add_systems(
            Startup,
            (setup, clouds::setup_clouds, water::setup_water, ui::spawn_crosshair),
        )
        .add_systems(
            Update,
            (
                player::camera_look,
                player::player_move_and_collision,
                interact::break_targeted_block,
                interact::place_targeted_block,
                streaming::stream_chunks_around_camera,
                water::stream_water_around_camera,
                clouds::stream_clouds_around_camera,
                clouds::animate_clouds,
                interact::highlight_targeted_block,
                regenerate_world_on_key,
            ),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    mut windows: Query<&mut Window>,
) {
    let material = materials.add(VoxelMaterial {
        params: VoxelMaterialParams {
            sun_dir_and_strength: Vec4::new(-0.45, -1.0, -0.25, 1.0),
            fog_color: Vec4::new(0.55, 0.8, 0.98, 1.0),
            fog_distances: Vec4::new(300.0, 760.0, 0.0, 0.0),
            ao: Vec4::new(0.48, 0.50, 0.0, 0.0),
        },
    });
    commands.insert_resource(TerrainMaterial(material));

    let mut transform = Transform::from_xyz(0.0, SEA_LEVEL as f32 + 34.0, 0.0);
    transform.look_at(Vec3::new(30.0, 65.0, 30.0), Vec3::Y);

    commands.spawn((
        Camera3dBundle {
            transform,
            ..default()
        },
        FlyCam {
            yaw: -90.0f32.to_radians(),
            pitch: -20.0f32.to_radians(),
            sensitivity: 0.002,
            velocity: Vec3::ZERO,
            grounded: false,
            fly_mode: false,
        },
    ));

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.5, 0.0)),
        ..default()
    });

    if let Ok(mut window) = windows.get_single_mut() {
        window.cursor.visible = false;
        window.cursor.grab_mode = CursorGrabMode::Locked;
    }
}

fn regenerate_world_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut loaded_clouds: ResMut<clouds::LoadedClouds>,
    mut loaded_water: ResMut<water::LoadedWater>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }

    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut new_seed = (t as u32) ^ ((t >> 32) as u32) ^ world.seed.rotate_left(13);
    if new_seed == 0 {
        new_seed = 1;
    }
    world.seed = new_seed;
    world.chunks.clear();

    let chunk_entities: Vec<Entity> = loaded_chunks.entries.values().map(|c| c.entity).collect();
    loaded_chunks.entries.clear();
    for entity in chunk_entities {
        commands.entity(entity).despawn_recursive();
    }

    loaded_clouds.clear_and_despawn(&mut commands);
    loaded_water.clear_and_despawn(&mut commands);
}
