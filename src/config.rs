//! Global gameplay and world constants used across systems.
pub const CHUNK_SIZE: usize = 32;
pub const WORLD_HEIGHT: usize = 192;
pub const VIEW_DISTANCE_CHUNKS: i32 = 20;
pub const SEA_LEVEL: i32 = 48;
pub const MAX_CHUNKS_GENERATED_PER_TICK: usize = 120;
pub const MAX_CHUNKS_MESHED_PER_TICK: usize = 96;

pub const WALK_SPEED: f32 = 8.0;
pub const SPRINT_MULTIPLIER: f32 = 1.8;
pub const GRAVITY: f32 = -32.0;
pub const JUMP_SPEED: f32 = 11.0;
pub const PLAYER_RADIUS: f32 = 0.35;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const EYE_HEIGHT: f32 = 1.62;
pub const STEP_HEIGHT: f32 = 0.9;

pub const BREAK_REACH: f32 = 6.5;
