use std::collections::{HashMap, HashSet};

use bevy::math::primitives::Cuboid;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpcKind {
    Friendly,
    Hostile,
}

#[derive(Component)]
pub struct Npc {
    pub kind: NpcKind,
    pub cell: IVec2,
    pub health: f32,
    pub dead: bool,
    pub heading: f32,
    pub speed: f32,
    pub turn_timer: f32,
    pub vertical_velocity: f32,
    pub rng: u32,
    pub follow_player: bool,
    pub attack_cooldown: f32,
    pub chat_cooldown: f32,
    pub last_seen_player: Vec3,
    pub last_seen_timer: f32,
    pub investigate_target: Vec3,
    pub investigate_timer: f32,
    pub last_pos: Vec3,
    pub stuck_timer: f32,
    pub knockback_velocity: Vec3,
    pub hurt_stun: f32,
    pub home_center: Vec2,
    pub home_radius: f32,
}

#[derive(Component)]
pub struct NpcRig {
    pub left_leg: Entity,
    pub right_leg: Entity,
    pub left_arm: Entity,
    pub right_arm: Entity,
    pub quadruped: bool,
}

#[derive(Resource, Default)]
pub struct LoadedNpcs {
    pub(super) entries: HashMap<IVec2, Entity>,
}

#[derive(Resource, Default)]
pub struct DeadNpcCells {
    killed: HashSet<IVec2>,
}

impl DeadNpcCells {
    pub fn clear(&mut self) {
        self.killed.clear();
    }

    pub fn mark_killed(&mut self, cell: IVec2) {
        self.killed.insert(cell);
    }

    pub fn is_killed(&self, cell: IVec2) -> bool {
        self.killed.contains(&cell)
    }
}

impl LoadedNpcs {
    pub fn clear_and_despawn(&mut self, commands: &mut Commands) {
        let entities: Vec<Entity> = self.entries.values().copied().collect();
        self.entries.clear();
        for entity in entities {
            commands.entity(entity).despawn_recursive();
        }
    }
}

#[derive(Resource)]
pub struct NpcStreamTimer(pub Timer);

#[derive(Resource, Debug, Clone, Copy)]
pub struct NpcStimulus {
    pub loud_pos: Vec3,
    pub ttl: f32,
}

impl Default for NpcStimulus {
    fn default() -> Self {
        Self {
            loud_pos: Vec3::ZERO,
            ttl: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct NpcUiState {
    pub message: String,
    pub ttl: f32,
}

impl Default for NpcUiState {
    fn default() -> Self {
        Self {
            message: "No NPC nearby".to_string(),
            ttl: 0.0,
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerVitals {
    pub health: f32,
    pub max_health: f32,
}

impl Default for PlayerVitals {
    fn default() -> Self {
        Self {
            health: 100.0,
            max_health: 100.0,
        }
    }
}

#[derive(Resource)]
pub(crate) struct NpcAssets {
    pub(super) mesh: Handle<Mesh>,
    pub(super) skin: Handle<StandardMaterial>,
    pub(super) cloth_a: Handle<StandardMaterial>,
    pub(super) cloth_b: Handle<StandardMaterial>,
    pub(super) cloth_c: Handle<StandardMaterial>,
    pub(super) hostile: Handle<StandardMaterial>,
}

pub fn setup_npcs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let skin = materials.add(StandardMaterial {
        base_color: Color::srgb(0.80, 0.68, 0.55),
        perceptual_roughness: 0.95,
        ..default()
    });
    let cloth_a = materials.add(StandardMaterial {
        base_color: Color::srgb(0.56, 0.66, 0.84),
        perceptual_roughness: 0.95,
        ..default()
    });
    let cloth_b = materials.add(StandardMaterial {
        base_color: Color::srgb(0.64, 0.78, 0.60),
        perceptual_roughness: 0.95,
        ..default()
    });
    let cloth_c = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.68, 0.52),
        perceptual_roughness: 0.95,
        ..default()
    });
    let hostile = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.30, 0.30),
        perceptual_roughness: 0.93,
        ..default()
    });

    commands.insert_resource(NpcAssets {
        mesh,
        skin,
        cloth_a,
        cloth_b,
        cloth_c,
        hostile,
    });
}
