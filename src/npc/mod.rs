//! NPC subsystem split by concern: data/components, spawning, simulation,
//! movement helpers, and debug visualization.

mod ai;
mod components;
mod debug;
mod movement;
mod spawn;

pub use ai::{capture_player_noise, npc_interactions, tick_npcs};
pub use components::{
    DeadNpcCells, LoadedNpcs, Npc, NpcRig, NpcStimulus, NpcStreamTimer, NpcUiState, NpcKind,
    PlayerVitals, setup_npcs,
};
pub use debug::draw_npc_debug_gizmos;
pub use spawn::stream_npcs_around_camera;

use crate::config::SEA_LEVEL;

pub(super) const NPC_HEIGHT: f32 = 1.72;
pub(super) const NPC_RADIUS: f32 = 0.28;
pub(super) const NPC_STEP_HEIGHT: f32 = 1.05;
pub(super) const NPC_GRAVITY: f32 = -22.0;
pub(super) const NPC_MAX_FALL: f32 = -30.0;
pub(super) const NPC_SPAWN_RADIUS_CHUNKS: i32 = 6;
pub(super) const NPC_DESPAWN_RADIUS_CHUNKS: i32 = 10;
pub(super) const NPC_MAX_COUNT: usize = 14;
pub(super) const NPC_MAX_SPAWNS_PER_TICK: usize = 2;
pub(super) const NPC_CELL_SIZE: i32 = 18;
pub(super) const FRIENDLY_INTERACT_RANGE: f32 = 4.8;
pub(super) const HOSTILE_AGGRO_RANGE: f32 = 18.0;
pub(super) const HOSTILE_ATTACK_RANGE: f32 = 1.45;
pub(super) const WATER_AVOID_LEVEL: i32 = SEA_LEVEL;
pub(super) const HOSTILE_VISION_RANGE: f32 = 32.0;
pub(super) const FRIENDLY_VISION_RANGE: f32 = 18.0;
pub(super) const HOSTILE_VISION_DOT: f32 = -0.15;
pub(super) const FRIENDLY_VISION_DOT: f32 = -0.40;
pub(super) const HOSTILE_HEARING_RANGE: f32 = 52.0;
pub(super) const NPC_SIGHT_MEMORY: f32 = 3.0;
pub(super) const NPC_INVESTIGATE_MEMORY: f32 = 4.5;
pub(super) const VILLAGE_SETTLEMENT_RADIUS: f32 = 40.0;
