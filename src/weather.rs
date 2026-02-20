use bevy::prelude::*;

use crate::clouds::CloudMaterial;
use crate::materials::{TerrainMaterial, VoxelMaterial};
use crate::water::{WaterMaterial, WaterSurfaceMaterial};

#[derive(Resource, Debug, Clone, Copy)]
pub struct DayNightState {
    pub time_of_day: f32,
    pub cycle_speed: f32,
    pub paused: bool,
}

impl Default for DayNightState {
    fn default() -> Self {
        Self {
            time_of_day: 0.22,
            cycle_speed: 0.004,
            paused: false,
        }
    }
}

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

pub fn tick_day_night(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cycle: ResMut<DayNightState>,
) {
    if keys.just_pressed(KeyCode::F8) {
        cycle.paused = !cycle.paused;
        info!(
            "day/night cycle {}",
            if cycle.paused { "paused" } else { "running" }
        );
    }
    if cycle.paused {
        return;
    }
    cycle.time_of_day = (cycle.time_of_day + time.delta_seconds() * cycle.cycle_speed).fract();
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
    cycle: Res<DayNightState>,
    terrain_handle: Res<TerrainMaterial>,
    water_handle: Res<WaterMaterial>,
    cloud_handle: Res<CloudMaterial>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient: ResMut<AmbientLight>,
    mut terrain_assets: ResMut<Assets<VoxelMaterial>>,
    mut water_assets: ResMut<Assets<WaterSurfaceMaterial>>,
    mut stdmats: ResMut<Assets<StandardMaterial>>,
    mut sun_q: Query<(&mut DirectionalLight, &mut Transform)>,
) {
    let (sun_strength, fog_color, fog_start, fog_end, cloud_shadow, rain_darken) = blended_env(weather.current, weather.target, weather.blend);
    let (sun_dir, daylight, sky_tint, sun_angle) = day_night_terms(cycle.time_of_day);
    let weather_dim = 1.0 - rain_darken * 0.22;
    let sun_energy = (0.10 + daylight * 0.98) * weather_dim;

    if let Some(terrain) = terrain_assets.get_mut(&terrain_handle.0) {
        terrain.params.sun_dir_and_strength.x = sun_dir.x;
        terrain.params.sun_dir_and_strength.y = sun_dir.y;
        terrain.params.sun_dir_and_strength.z = sun_dir.z;
        terrain.params.sun_dir_and_strength.w = sun_strength * sun_energy;
        terrain.params.fog_color = (fog_color * sky_tint).extend(1.0);
        terrain.params.fog_distances.x = fog_start * (0.78 + daylight * 0.28);
        terrain.params.fog_distances.y = fog_end * (0.72 + daylight * 0.34);
        terrain.params.weather = Vec4::new(cloud_shadow, 0.018, 0.013, rain_darken);
    }

    if let Some(water) = water_assets.get_mut(&water_handle.0) {
        water.params.shallow_color = Vec4::new(
            0.10 + rain_darken * 0.02,
            0.58 - rain_darken * 0.08,
            0.88 - rain_darken * 0.13,
            0.91,
        );
        water.params.deep_color = Vec4::new(
            0.01,
            0.12 - rain_darken * 0.03,
            0.36 - rain_darken * 0.10,
            0.96,
        );
        water.params.wave = Vec4::new(0.080 + rain_darken * 0.020, 2.7 + rain_darken * 1.8, 1.15 + rain_darken * 0.52, 0.5);
        water.params.foam = Vec4::new(0.26 + rain_darken * 0.22, 0.0, 0.0, 0.0);
        water.params.weather = Vec4::new(rain_darken, 0.0, 0.0, 0.0);
    }

    if let Some(clouds) = stdmats.get_mut(&cloud_handle.0) {
        let a = 0.84 + rain_darken * 0.12;
        clouds.base_color = Color::srgba(0.94 - rain_darken * 0.08, 0.96 - rain_darken * 0.10, 1.0 - rain_darken * 0.14, a);
    }

    if let Ok((mut sun, mut transform)) = sun_q.get_single_mut() {
        sun.illuminance = 350.0 + 42_000.0 * daylight * weather_dim;
        sun.color = Color::srgb(
            1.0,
            0.87 + daylight * 0.10,
            0.74 + daylight * 0.26,
        );
        // Keep the directional light and voxel material sun direction aligned.
        transform.rotation = Quat::from_euler(EulerRot::XYZ, -0.72 + (sun_angle.sin() * 0.62), 0.52 + sun_angle.cos() * 0.38, 0.0);
    }

    clear_color.0 = Color::srgb(
        0.06 + sky_tint.x * (0.34 + daylight * 0.32),
        0.08 + sky_tint.y * (0.34 + daylight * 0.36),
        0.12 + sky_tint.z * (0.38 + daylight * 0.44),
    );
    ambient.brightness = 24.0 + daylight * 150.0;
    ambient.color = Color::srgb(
        0.46 + daylight * 0.34,
        0.50 + daylight * 0.32,
        0.58 + daylight * 0.26,
    );
}

fn day_night_terms(time_of_day: f32) -> (Vec3, f32, Vec3, f32) {
    let angle = time_of_day * std::f32::consts::TAU;
    let elev = angle.sin();
    let daylight = smoothstep(-0.16, 0.20, elev).clamp(0.0, 1.0);
    let sun_dir = Vec3::new(angle.cos() * 0.58, -elev.max(-0.88), angle.sin() * 0.36).normalize();
    let dusk = (1.0 - ((elev + 0.05) * 4.0).abs()).clamp(0.0, 1.0);
    let sky_day = Vec3::new(0.76, 0.88, 1.00);
    let sky_night = Vec3::new(0.17, 0.22, 0.34);
    let sky = sky_night.lerp(sky_day, daylight).lerp(Vec3::new(1.00, 0.66, 0.48), dusk * 0.16);
    (sun_dir, daylight, sky, angle)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
