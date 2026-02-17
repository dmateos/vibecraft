use bevy::prelude::*;

use crate::clouds::CloudMaterial;
use crate::materials::{TerrainMaterial, VoxelMaterial};
use crate::water::{WaterMaterial, WaterSurfaceMaterial};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WeatherPreset {
    Clear,
    Foggy,
    Rainy,
}

impl WeatherPreset {
    pub fn next(self) -> Self {
        match self {
            WeatherPreset::Clear => WeatherPreset::Foggy,
            WeatherPreset::Foggy => WeatherPreset::Rainy,
            WeatherPreset::Rainy => WeatherPreset::Clear,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WeatherPreset::Clear => "Clear",
            WeatherPreset::Foggy => "Foggy",
            WeatherPreset::Rainy => "Rainy",
        }
    }
}

#[derive(Resource)]
pub struct WeatherState {
    pub current: WeatherPreset,
    pub target: WeatherPreset,
    pub blend: f32,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            current: WeatherPreset::Clear,
            target: WeatherPreset::Clear,
            blend: 1.0,
        }
    }
}

impl WeatherState {
    pub fn rain_factor(&self) -> f32 {
        let (_, _, _, _, _, rain) = blended_env(self.current, self.target, self.blend);
        rain
    }
}

pub fn cycle_weather_on_key(keys: Res<ButtonInput<KeyCode>>, mut weather: ResMut<WeatherState>) {
    if !keys.just_pressed(KeyCode::F7) {
        return;
    }
    let next = weather.target.next();
    weather.current = sample_blended_preset(weather.current, weather.target, weather.blend);
    weather.target = next;
    weather.blend = 0.0;
    info!("weather target -> {}", next.label());
}

pub fn tick_weather_blend(time: Res<Time>, mut weather: ResMut<WeatherState>) {
    if weather.current == weather.target {
        weather.blend = 1.0;
        return;
    }
    weather.blend = (weather.blend + time.delta_seconds() * 0.22).clamp(0.0, 1.0);
    if weather.blend >= 0.999 {
        weather.current = weather.target;
        weather.blend = 1.0;
    }
}

pub fn apply_weather_to_materials(
    weather: Res<WeatherState>,
    terrain_handle: Res<TerrainMaterial>,
    water_handle: Res<WaterMaterial>,
    cloud_handle: Res<CloudMaterial>,
    mut terrain_assets: ResMut<Assets<VoxelMaterial>>,
    mut water_assets: ResMut<Assets<WaterSurfaceMaterial>>,
    mut stdmats: ResMut<Assets<StandardMaterial>>,
) {
    let (sun_strength, fog_color, fog_start, fog_end, cloud_shadow, rain_darken) = blended_env(weather.current, weather.target, weather.blend);
    if let Some(terrain) = terrain_assets.get_mut(&terrain_handle.0) {
        terrain.params.sun_dir_and_strength.w = sun_strength;
        terrain.params.fog_color = fog_color.extend(1.0);
        terrain.params.fog_distances.x = fog_start;
        terrain.params.fog_distances.y = fog_end;
        terrain.params.weather = Vec4::new(cloud_shadow, 0.018, 0.013, rain_darken);
    }

    if let Some(water) = water_assets.get_mut(&water_handle.0) {
        water.params.shallow_color = Vec4::new(
            0.20 + rain_darken * 0.03,
            0.62 - rain_darken * 0.10,
            0.92 - rain_darken * 0.16,
            0.90,
        );
        water.params.deep_color = Vec4::new(
            0.02,
            0.10 - rain_darken * 0.04,
            0.24 - rain_darken * 0.08,
            0.95,
        );
        water.params.wave = Vec4::new(0.085 + rain_darken * 0.022, 2.9 + rain_darken * 1.9, 1.25 + rain_darken * 0.55, 0.5);
        water.params.foam = Vec4::new(0.30 + rain_darken * 0.24, 0.0, 0.0, 0.0);
        water.params.weather = Vec4::new(rain_darken, 0.0, 0.0, 0.0);
    }

    if let Some(clouds) = stdmats.get_mut(&cloud_handle.0) {
        let a = 0.84 + rain_darken * 0.12;
        clouds.base_color = Color::srgba(0.94 - rain_darken * 0.08, 0.96 - rain_darken * 0.10, 1.0 - rain_darken * 0.14, a);
    }
}

fn blended_env(
    current: WeatherPreset,
    target: WeatherPreset,
    blend: f32,
) -> (f32, Vec3, f32, f32, f32, f32) {
    let (sun_a, fog_a, s_a, e_a, cloud_a, rain_a) = env_for(current);
    let (sun_b, fog_b, s_b, e_b, cloud_b, rain_b) = env_for(target);
    let t = blend.clamp(0.0, 1.0);
    (
        sun_a + (sun_b - sun_a) * t,
        fog_a + (fog_b - fog_a) * t,
        s_a + (s_b - s_a) * t,
        e_a + (e_b - e_a) * t,
        cloud_a + (cloud_b - cloud_a) * t,
        rain_a + (rain_b - rain_a) * t,
    )
}

fn env_for(p: WeatherPreset) -> (f32, Vec3, f32, f32, f32, f32) {
    match p {
        WeatherPreset::Clear => (1.08, Vec3::new(0.46, 0.74, 0.98), 320.0, 860.0, 0.16, 0.0),
        WeatherPreset::Foggy => (0.86, Vec3::new(0.64, 0.74, 0.82), 170.0, 560.0, 0.24, 0.2),
        WeatherPreset::Rainy => (0.72, Vec3::new(0.36, 0.48, 0.60), 160.0, 560.0, 0.42, 0.55),
    }
}

fn sample_blended_preset(current: WeatherPreset, target: WeatherPreset, blend: f32) -> WeatherPreset {
    if blend < 0.5 {
        current
    } else {
        target
    }
}
