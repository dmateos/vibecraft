mod clouds;
mod config;
mod generation;
mod interact;
mod materials;
mod player;
mod streaming;
mod ui;
mod water;
mod weather;
mod world;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::pbr::{CascadeShadowConfigBuilder, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::texture::{ImageLoaderSettings, ImageSampler};
use bevy::window::CursorGrabMode;

use config::SEA_LEVEL;
use materials::{TerrainMaterial, VoxelMaterial, VoxelMaterialParams};
use player::FlyCam;
use world::{LoadedChunks, StreamTimer, TerrainMode, VoxelWorld};

fn main() {
    App::new()
        .insert_resource(Msaa::Off)
        .insert_resource(ClearColor(Color::srgb(0.40, 0.72, 0.96)))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 180.0,
        })
        .insert_resource(VoxelWorld {
            seed: 1337,
            chunks: HashMap::new(),
        })
        .insert_resource(TerrainMode::Procedural)
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
        .insert_resource(generation::GenerationConfig::default())
        .insert_resource(generation::GenerationQueue::default())
        .insert_resource(generation::GenerationRuntimeStats::default())
        .insert_resource(generation::LiveLlmState::default())
        .insert_resource(generation::PromptInputState::default())
        .insert_resource(weather::DayNightState::default())
        .insert_resource(weather::WeatherState::default())
        .insert_resource(streaming::StreamingRuntimeStats::default())
        .insert_resource(interact::PlacementPalette::default())
        .insert_resource(ui::DebugOverlayState::default())
        .insert_resource(ui::FrameStats::default())
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
            (
                setup,
                clouds::setup_clouds,
                water::setup_water,
                generation::initialize_prompt_input,
                ui::spawn_crosshair,
                ui::spawn_hud,
            ),
        )
        .add_systems(
            Update,
            (
                player::camera_look,
                player::player_move_and_collision,
                interact::cycle_palette_on_scroll,
                interact::break_targeted_block,
                interact::place_targeted_block,
                streaming::stream_chunks_around_camera,
                water::stream_water_around_camera,
                clouds::stream_clouds_around_camera,
                clouds::animate_clouds,
            ),
        )
        .add_systems(
            Update,
            (
                interact::highlight_targeted_block,
                generation::trigger_demo_generation_on_key,
                generation::load_generation_request_on_key,
                generation::toggle_prompt_input_mode,
                generation::edit_prompt_input,
                generation::trigger_live_llm_generation_on_key,
                generation::poll_live_llm_result,
                generation::update_prompt_window_title,
                generation::process_generation_queue,
                regenerate_world_on_key,
                toggle_terrain_mode_on_key,
                weather::tick_day_night,
                weather::cycle_weather_on_key,
                weather::tick_weather_blend,
                weather::apply_weather_to_materials,
                ui::toggle_debug_overlay,
                ui::sample_frame_stats,
                ui::update_hud_text,
                ui::update_debug_hud_text,
            ),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
    mut windows: Query<&mut Window>,
) {
    let material = materials.add(VoxelMaterial {
        params: VoxelMaterialParams {
            sun_dir_and_strength: Vec4::new(-0.45, -1.0, -0.25, 1.0),
            fog_color: Vec4::new(0.55, 0.8, 0.98, 1.0),
            fog_distances: Vec4::new(300.0, 760.0, 0.0, 0.0),
            ao: Vec4::new(0.48, 0.50, 0.0, 0.0),
            weather: Vec4::new(0.18, 0.018, 0.013, 0.0),
        },
        atlas: asset_server.load_with_settings(
            "textures/vibecraft/terrain/atlas.png",
            |settings: &mut ImageLoaderSettings| {
                // Atlas tiles should use nearest filtering to avoid sampling neighboring tiles.
                settings.sampler = ImageSampler::nearest();
            },
        ),
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
            shadows_enabled: true,
            ..default()
        },
        cascade_shadow_config: CascadeShadowConfigBuilder {
            first_cascade_far_bound: 40.0,
            maximum_distance: 180.0,
            ..default()
        }
        .into(),
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
    prompt: Res<generation::PromptInputState>,
    mut cam_q: Query<&mut Transform, With<FlyCam>>,
) {
    if prompt.active {
        return;
    }
    if !(keys.just_pressed(KeyCode::KeyR) || keys.just_pressed(KeyCode::F5)) {
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
    info!("reseeding world: {} -> {}", world.seed, new_seed);
    world.seed = new_seed;
    world.chunks.clear();

    let chunk_entities: Vec<Entity> = loaded_chunks.entries.values().map(|c| c.entity).collect();
    loaded_chunks.entries.clear();
    for entity in chunk_entities {
        commands.entity(entity).despawn_recursive();
    }

    loaded_clouds.clear_and_despawn(&mut commands);
    loaded_water.clear_and_despawn(&mut commands);

    if let Ok(mut cam) = cam_q.get_single_mut() {
        let min_eye_y = SEA_LEVEL as f32 + 12.0;
        if cam.translation.y < min_eye_y {
            cam.translation.y = min_eye_y;
        }
    }
}

fn toggle_terrain_mode_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut terrain_mode: ResMut<TerrainMode>,
    mut world: ResMut<VoxelWorld>,
    mut loaded_chunks: ResMut<LoadedChunks>,
    mut loaded_clouds: ResMut<clouds::LoadedClouds>,
    mut loaded_water: ResMut<water::LoadedWater>,
    prompt: Res<generation::PromptInputState>,
    mut cam_q: Query<&mut Transform, With<FlyCam>>,
) {
    if prompt.active {
        return;
    }
    if !keys.just_pressed(KeyCode::F6) {
        return;
    }

    *terrain_mode = terrain_mode.toggled();
    info!("terrain mode switched to {}", terrain_mode.label());

    world.chunks.clear();
    let chunk_entities: Vec<Entity> = loaded_chunks.entries.values().map(|c| c.entity).collect();
    loaded_chunks.entries.clear();
    for entity in chunk_entities {
        commands.entity(entity).despawn_recursive();
    }

    loaded_clouds.clear_and_despawn(&mut commands);
    loaded_water.clear_and_despawn(&mut commands);

    if let Ok(mut cam) = cam_q.get_single_mut() {
        let min_eye_y = SEA_LEVEL as f32 + 12.0;
        if cam.translation.y < min_eye_y {
            cam.translation.y = min_eye_y;
        }
    }
}
