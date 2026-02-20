use bevy::math::primitives::{Cuboid, Sphere};
use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;

use crate::player::FlyCam;
use crate::weather::DayNightState;

const SKY_RADIUS: f32 = 1400.0;
const SUN_DISTANCE: f32 = 980.0;
const STAR_DISTANCE: f32 = 1180.0;
const STAR_COUNT: usize = 140;

#[derive(Component)]
pub(crate) struct SkyDome;

#[derive(Component)]
pub(crate) struct SunDisc;

#[derive(Component)]
pub(crate) struct MoonDisc;

#[derive(Component)]
pub(crate) struct Star {
    idx: usize,
}

#[derive(Resource)]
pub(crate) struct SkyAssets {
    sun_mat: Handle<StandardMaterial>,
    moon_mat: Handle<StandardMaterial>,
    star_mat: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub(crate) struct StarField {
    dirs: Vec<Vec3>,
}

pub fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut dome_mesh = Mesh::from(Sphere { radius: SKY_RADIUS });
    let Some(VertexAttributeValues::Float32x3(positions)) =
        dome_mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };

    let mut colors = Vec::with_capacity(positions.len());
    for p in positions {
        let ny = (p[1] / SKY_RADIUS).clamp(-1.0, 1.0);
        let bottom = Vec3::new(0.06, 0.09, 0.16);
        let horizon = Vec3::new(0.44, 0.61, 0.88);
        let top = Vec3::new(0.16, 0.30, 0.62);
        let c = if ny < 0.0 {
            bottom.lerp(horizon, ny + 1.0)
        } else {
            horizon.lerp(top, ny)
        };
        colors.push([c.x, c.y, c.z, 1.0]);
    }
    dome_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);

    let dome_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    });

    let sun_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.86, 0.52, 0.98),
        emissive: Color::srgb(0.92, 0.65, 0.28).into(),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    });
    let moon_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.82, 0.90, 1.0, 0.92),
        emissive: Color::srgb(0.20, 0.24, 0.30).into(),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    });
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.96, 0.98, 1.0, 0.0),
        emissive: Color::srgb(0.92, 0.94, 1.0).into(),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    });

    let dome = commands
        .spawn((
            PbrBundle {
                mesh: meshes.add(dome_mesh),
                material: dome_mat,
                ..default()
            },
            SkyDome,
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id();

    let disc_mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));

    let sun = commands
        .spawn((
            PbrBundle {
                mesh: disc_mesh.clone(),
                material: sun_mat.clone(),
                transform: Transform {
                    scale: Vec3::new(52.0, 52.0, 0.8),
                    ..default()
                },
                ..default()
            },
            SunDisc,
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id();

    let moon = commands
        .spawn((
            PbrBundle {
                mesh: disc_mesh.clone(),
                material: moon_mat.clone(),
                transform: Transform {
                    scale: Vec3::new(44.0, 44.0, 0.8),
                    ..default()
                },
                ..default()
            },
            MoonDisc,
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id();

    commands.entity(dome).add_child(sun).add_child(moon);

    let mut dirs = Vec::with_capacity(STAR_COUNT);
    for i in 0..STAR_COUNT {
        let h = hash11(i as u32 + 17);
        let a = h.x * std::f32::consts::TAU;
        let y = h.y * 0.58 + 0.28;
        let r = (1.0 - y * y).sqrt();
        let dir = Vec3::new(a.cos() * r, y, a.sin() * r).normalize();
        dirs.push(dir);

        let scale = 1.2 + h.z * 1.7;
        commands.spawn((
            PbrBundle {
                mesh: disc_mesh.clone(),
                material: star_mat.clone(),
                transform: Transform {
                    scale: Vec3::new(scale, scale, 0.25),
                    ..default()
                },
                ..default()
            },
            Star { idx: i },
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }

    commands.insert_resource(SkyAssets {
        sun_mat,
        moon_mat,
        star_mat,
    });
    commands.insert_resource(StarField { dirs });
}

pub fn update_sky(
    cycle: Res<DayNightState>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut sky_parts: ParamSet<(
        Query<&mut Transform, (With<SkyDome>, Without<FlyCam>)>,
        Query<&mut Transform, (With<SunDisc>, Without<FlyCam>)>,
        Query<&mut Transform, (With<MoonDisc>, Without<FlyCam>)>,
        Query<(&Star, &mut Transform), Without<FlyCam>>,
    )>,
    sky_assets: Res<SkyAssets>,
    star_field: Res<StarField>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let (sun_dir, daylight) = sun_terms(cycle.time_of_day);
    let cam_pos = cam.translation;

    if let Ok(mut t) = sky_parts.p0().get_single_mut() {
        t.translation = cam_pos;
    }

    if let Ok(mut sun_t) = sky_parts.p1().get_single_mut() {
        let p = cam_pos + sun_dir * SUN_DISTANCE;
        sun_t.translation = p;
        sun_t.look_at(cam_pos, Vec3::Y);
    }

    if let Ok(mut moon_t) = sky_parts.p2().get_single_mut() {
        let p = cam_pos - sun_dir * SUN_DISTANCE;
        moon_t.translation = p;
        moon_t.look_at(cam_pos, Vec3::Y);
    }

    for (star, mut t) in &mut sky_parts.p3() {
        let dir = star_field.dirs[star.idx];
        t.translation = cam_pos + dir * STAR_DISTANCE;
        t.look_at(cam_pos, Vec3::Y);
    }

    if let Some(sun) = mats.get_mut(&sky_assets.sun_mat) {
        let a = (daylight * 1.05).clamp(0.0, 1.0);
        sun.base_color = Color::srgba(1.0, 0.86, 0.52, a);
    }
    if let Some(moon) = mats.get_mut(&sky_assets.moon_mat) {
        let a = (1.0 - daylight).powf(1.2).clamp(0.0, 1.0);
        moon.base_color = Color::srgba(0.82, 0.90, 1.0, a * 0.95);
    }
    if let Some(stars) = mats.get_mut(&sky_assets.star_mat) {
        let a = (1.0 - daylight).powf(2.1).clamp(0.0, 1.0);
        stars.base_color = Color::srgba(0.96, 0.98, 1.0, a * 0.92);
    }
}

#[inline]
fn sun_terms(time_of_day: f32) -> (Vec3, f32) {
    let angle = time_of_day * std::f32::consts::TAU;
    let elev = angle.sin();
    let daylight = smoothstep(-0.16, 0.20, elev).clamp(0.0, 1.0);
    let dir = Vec3::new(angle.cos() * 0.58, elev.max(-0.88), angle.sin() * 0.36).normalize();
    (dir, daylight)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn hash11(n: u32) -> Vec3 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x ^= x >> 16;
    x = x.wrapping_mul(2_246_822_519);
    let a = (x & 0xFFFF) as f32 / 65535.0;
    let b = ((x >> 8) & 0xFFFF) as f32 / 65535.0;
    let c = ((x >> 16) & 0xFFFF) as f32 / 65535.0;
    Vec3::new(a, b, c)
}
