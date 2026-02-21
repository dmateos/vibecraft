#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const SERVER_TICK_HZ: u16 = 20;

pub type ClientId = u64;
pub type NetEntityId = u64;
pub type SnapshotSeq = u32;
pub type InputSeq = u32;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Channel {
    ReliableOrdered,
    UnreliableSequenced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello(HelloMsg),
    InputFrame(InputFrameMsg),
    AckSnapshot(AckSnapshotMsg),
    BlockEdits(BlockEditsMsg),
    NpcSync(NpcSyncMsg),
    Chat(ChatMsg),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome(WelcomeMsg),
    Snapshot(WorldSnapshotMsg),
    ChunkBaseline(ChunkBaselineMsg),
    ChunkDelta(ChunkDeltaMsg),
    EventBatch(EventBatchMsg),
    ServerNotice(ServerNoticeMsg),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloMsg {
    pub protocol_version: u32,
    pub player_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WelcomeMsg {
    pub protocol_version: u32,
    pub your_client_id: ClientId,
    pub world_seed: u32,
    pub server_tick_hz: u16,
    pub server_time_s: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputFrameMsg {
    pub input_seq: InputSeq,
    pub client_time_s: f32,
    pub move_x: f32,
    pub move_z: f32,
    pub jump_pressed: bool,
    pub sprint_pressed: bool,
    pub fire_pressed: bool,
    pub grenade_pressed: bool,
    pub break_pressed: bool,
    pub place_pressed: bool,
    pub place_block: Option<BlockIdNet>,
    pub fire_cell: Option<[i32; 3]>,
    pub break_cell: Option<[i32; 3]>,
    pub place_cell: Option<[i32; 3]>,
    pub look_yaw: f32,
    pub look_pitch: f32,
    pub view_origin: [f32; 3],
    pub view_dir: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckSnapshotMsg {
    pub snapshot_seq: SnapshotSeq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockEditsMsg {
    pub edits: Vec<BlockCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcSyncMsg {
    pub npcs: Vec<NpcStateNet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMsg {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSnapshotMsg {
    pub snapshot_seq: SnapshotSeq,
    pub server_tick: u64,
    pub server_time_s: f32,
    pub players: Vec<PlayerStateNet>,
    pub npcs: Vec<NpcStateNet>,
    pub projectiles: Vec<ProjectileStateNet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStateNet {
    pub entity_id: NetEntityId,
    pub client_id: ClientId,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub hp: f32,
    pub dead: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcStateNet {
    pub entity_id: NetEntityId,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub kind: NpcKindNet,
    pub hp: f32,
    pub dead: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NpcKindNet {
    Friendly,
    Hostile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectileStateNet {
    pub entity_id: NetEntityId,
    pub kind: ProjectileKindNet,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectileKindNet {
    Bullet,
    Grenade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkBaselineMsg {
    pub chunk: [i32; 2],
    pub blocks: Vec<BlockCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDeltaMsg {
    pub chunk: [i32; 2],
    pub edits: Vec<BlockCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockCell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: BlockIdNet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockIdNet {
    Air,
    Grass,
    Dirt,
    Stone,
    Sand,
    Snow,
    Wood,
    Leaves,
    Red,
    Blue,
    Yellow,
    Purple,
    Cyan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatchMsg {
    pub events: Vec<NetEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetEvent {
    GunFired {
        shooter_entity: NetEntityId,
        from: [f32; 3],
        dir: [f32; 3],
    },
    BlockChanged {
        source_client: ClientId,
        x: i32,
        y: i32,
        z: i32,
        block: BlockIdNet,
    },
    NpcDamaged {
        npc_entity: NetEntityId,
        new_hp: f32,
    },
    NpcDied {
        npc_entity: NetEntityId,
        at: [f32; 3],
    },
    PlayerDamaged {
        player_entity: NetEntityId,
        new_hp: f32,
    },
    Explosion {
        at: [f32; 3],
        radius: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerNoticeMsg {
    pub code: NoticeCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NoticeCode {
    Info,
    Warning,
    ProtocolMismatch,
    RejectedInput,
}
