#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use bevy::math::primitives::Cuboid;
use bevy::prelude::*;

use crate::interact::LocalBlockEditEvent;
use crate::npc::{Npc, NpcKind};
use crate::player::FlyCam;
use crate::world::Block;
use crate::world::{div_floor, remesh_affected_chunks, set_block_world, LoadedChunks, VoxelWorld};
use vibecraft::net::protocol::{
    AckSnapshotMsg, BlockCell, BlockEditsMsg, BlockIdNet, ClientId, ClientMsg, HelloMsg, InputFrameMsg, NetEvent,
    NpcKindNet, NpcStateNet, NpcSyncMsg, PROTOCOL_VERSION, PlayerStateNet, ServerMsg, SnapshotSeq,
};

const NET_MAX_RECV_PER_FRAME: usize = 64;
const NET_INPUT_SEND_MS: u64 = 100;
const NET_NPC_SYNC_MS: u64 = 300;
const NET_MAX_BLOCK_EDITS_PER_FRAME: usize = 96;

#[derive(Resource, Debug, Clone)]
pub struct NetClientConfig {
    pub enabled: bool,
    pub server: Option<SocketAddr>,
    pub name: String,
}

impl Default for NetClientConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server: None,
            name: "player".to_string(),
        }
    }
}

impl NetClientConfig {
    pub fn from_args() -> Self {
        let mut cfg = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--connect" => {
                    if let Some(v) = args.next()
                        && let Ok(addr) = v.parse::<SocketAddr>()
                    {
                        cfg.server = Some(addr);
                        cfg.enabled = true;
                    }
                }
                "--name" => {
                    if let Some(v) = args.next() {
                        cfg.name = v;
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

#[derive(Resource)]
pub struct NetClientState {
    pub cfg: NetClientConfig,
    socket: Option<UdpSocket>,
    last_input_send: Instant,
    last_npc_send: Instant,
    input_seq: u32,
    hello_sent: bool,
    pub connected: bool,
    pub client_id: Option<ClientId>,
    pub authority_client_id: Option<ClientId>,
    pub last_snapshot: Option<SnapshotSeq>,
    pub remote_players: Vec<PlayerStateNet>,
    pub remote_npcs: Vec<NpcStateNet>,
    pub pending_block_edits: Vec<(ClientId, BlockCell)>,
    pub pending_fx: Vec<NetFxSpawn>,
}

impl NetClientState {
    pub fn new(cfg: NetClientConfig) -> Self {
        Self {
            cfg,
            socket: None,
            last_input_send: Instant::now(),
            last_npc_send: Instant::now(),
            input_seq: 1,
            hello_sent: false,
            connected: false,
            client_id: None,
            authority_client_id: None,
            last_snapshot: None,
            remote_players: Vec::new(),
            remote_npcs: Vec::new(),
            pending_block_edits: Vec::new(),
            pending_fx: Vec::new(),
        }
    }

    pub fn is_authority(&self) -> bool {
        self.connected
            && self.client_id.is_some()
            && self.authority_client_id.is_some()
            && self.client_id == self.authority_client_id
    }
}

#[derive(Resource, Default)]
pub struct NetEntityMap {
    players: HashMap<ClientId, Entity>,
    npcs: HashMap<u64, Entity>,
}

#[derive(Resource)]
pub struct NetVisualAssets {
    mesh: Handle<Mesh>,
    player_mat: Handle<StandardMaterial>,
    npc_friendly_mat: Handle<StandardMaterial>,
    npc_hostile_mat: Handle<StandardMaterial>,
    fx_tracer_mat: Handle<StandardMaterial>,
    fx_explosion_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct RemotePlayer {
    client_id: ClientId,
}

#[derive(Component)]
struct RemoteNpc {
    net_id: u64,
}

#[derive(Component)]
pub struct NetFx {
    ttl: f32,
    start_scale: Vec3,
    end_scale: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub enum NetFxSpawn {
    Gun { source_client: ClientId, from: Vec3, dir: Vec3 },
    Explosion { at: Vec3, radius: f32 },
}

pub fn setup_net_client(mut state: ResMut<NetClientState>) {
    if !state.cfg.enabled {
        return;
    }
    let Some(server) = state.cfg.server else {
        warn!("net client enabled without server addr");
        return;
    };

    let sock = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            warn!("net client bind failed: {e}");
            return;
        }
    };
    if let Err(e) = sock.set_nonblocking(true) {
        warn!("net client set_nonblocking failed: {e}");
        return;
    }
    info!("net client local={} server={server}", sock.local_addr().unwrap_or(server));
    state.socket = Some(sock);
}

pub fn setup_net_visual_assets(
    mut commands: Commands,
    state: Res<NetClientState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !state.cfg.enabled {
        return;
    }
    let mesh = meshes.add(Mesh::from(Cuboid::from_size(Vec3::ONE)));
    let player_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.24, 0.74, 0.98),
        ..default()
    });
    let npc_friendly_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.86, 0.42),
        ..default()
    });
    let npc_hostile_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.92, 0.30, 0.28),
        ..default()
    });
    let fx_tracer_mat = mats.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.90, 0.56, 0.85),
        emissive: Color::srgb(0.95, 0.74, 0.35).into(),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let fx_explosion_mat = mats.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.52, 0.22, 0.30),
        emissive: Color::srgb(0.92, 0.42, 0.14).into(),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.insert_resource(NetVisualAssets {
        mesh,
        player_mat,
        npc_friendly_mat,
        npc_hostile_mat,
        fx_tracer_mat,
        fx_explosion_mat,
    });
    commands.insert_resource(NetEntityMap::default());
}

pub fn tick_net_client(
    mut state: ResMut<NetClientState>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    npc_q: Query<(&Transform, &Npc)>,
    mut local_block_edits: EventReader<LocalBlockEditEvent>,
) {
    if !state.cfg.enabled {
        return;
    }
    let Some(server) = state.cfg.server else {
        return;
    };
    let Some(sock) = state.socket.take() else {
        return;
    };

    if !state.hello_sent {
        let hello = ClientMsg::Hello(HelloMsg {
            protocol_version: PROTOCOL_VERSION,
            player_name: state.cfg.name.clone(),
        });
        send(&sock, server, &hello);
        state.hello_sent = true;
    }

    // local block edits -> server
    let mut edits = Vec::new();
    for e in local_block_edits.read() {
        edits.push(BlockCell {
            x: e.x,
            y: e.y,
            z: e.z,
            block: block_to_net(e.block),
        });
    }
    if !edits.is_empty() {
        send(&sock, server, &ClientMsg::BlockEdits(BlockEditsMsg { edits }));
    }

    // incoming
    let mut buf = [0u8; 8192];
    let mut recv_count = 0usize;
    loop {
        if recv_count >= NET_MAX_RECV_PER_FRAME {
            break;
        }
        let (n, _from) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) => {
                warn!("net recv error: {e}");
                break;
            }
        };
        recv_count += 1;

        let msg: ServerMsg = match serde_json::from_slice(&buf[..n]) {
            Ok(m) => m,
            Err(e) => {
                warn!("net invalid server msg: {e}");
                continue;
            }
        };

        match msg {
            ServerMsg::Welcome(w) => {
                state.connected = true;
                state.client_id = Some(w.your_client_id);
                info!(
                    "net welcome client_id={} seed={} tick_hz={}",
                    w.your_client_id, w.world_seed, w.server_tick_hz
                );
            }
            ServerMsg::Snapshot(s) => {
                state.last_snapshot = Some(s.snapshot_seq);
                state.authority_client_id = s.players.iter().map(|p| p.client_id).min();
                state.remote_players = s.players;
                state.remote_npcs = s.npcs;
                let ack = ClientMsg::AckSnapshot(AckSnapshotMsg {
                    snapshot_seq: s.snapshot_seq,
                });
                send(&sock, server, &ack);
            }
            ServerMsg::EventBatch(batch) => {
                for ev in batch.events {
                    match ev {
                        NetEvent::BlockChanged {
                            source_client,
                            x,
                            y,
                            z,
                            block,
                        } => {
                            state.pending_block_edits.push((
                                source_client,
                                BlockCell { x, y, z, block },
                            ));
                        }
                        NetEvent::GunFired {
                            shooter_entity,
                            from,
                            dir,
                        } => {
                            state.pending_fx.push(NetFxSpawn::Gun {
                                source_client: shooter_entity as ClientId,
                                from: Vec3::new(from[0], from[1], from[2]),
                                dir: Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero(),
                            });
                        }
                        NetEvent::Explosion { at, radius } => {
                            state.pending_fx.push(NetFxSpawn::Explosion {
                                at: Vec3::new(at[0], at[1], at[2]),
                                radius,
                            });
                        }
                        _ => {}
                    }
                }
            }
            ServerMsg::ServerNotice(n) => {
                warn!("server notice {:?}: {}", n.code, n.message);
            }
            _ => {}
        }
    }

    // input heartbeat
    if state.last_input_send.elapsed() >= Duration::from_millis(NET_INPUT_SEND_MS)
        && let Ok(cam) = cam_q.get_single()
    {
        let move_x = (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
        let move_z = (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32;
        let view_dir = *cam.forward();
        let input = ClientMsg::InputFrame(InputFrameMsg {
            input_seq: state.input_seq,
            client_time_s: time.elapsed_seconds(),
            move_x,
            move_z,
            jump_pressed: keys.pressed(KeyCode::Space),
            sprint_pressed: keys.pressed(KeyCode::ControlLeft),
            fire_pressed: keys.just_pressed(KeyCode::KeyZ),
            grenade_pressed: keys.just_pressed(KeyCode::KeyQ),
            break_pressed: false,
            place_pressed: false,
            look_yaw: 0.0,
            look_pitch: 0.0,
            view_origin: [cam.translation.x, cam.translation.y, cam.translation.z],
            view_dir: [view_dir.x, view_dir.y, view_dir.z],
        });
        send(&sock, server, &input);
        state.input_seq = state.input_seq.wrapping_add(1);
        state.last_input_send = Instant::now();
    }

    // NPC authority publish
    if state.connected
        && state.is_authority()
        && state.last_npc_send.elapsed() >= Duration::from_millis(NET_NPC_SYNC_MS)
    {
        let npcs = npc_q
            .iter()
            .map(|(t, npc)| NpcStateNet {
                entity_id: npc_net_id(npc.cell),
                pos: [t.translation.x, t.translation.y, t.translation.z],
                yaw: npc.heading,
                kind: match npc.kind {
                    NpcKind::Friendly => NpcKindNet::Friendly,
                    NpcKind::Hostile => NpcKindNet::Hostile,
                },
                hp: npc.health,
                dead: npc.dead,
            })
            .collect();
        send(&sock, server, &ClientMsg::NpcSync(NpcSyncMsg { npcs }));
        state.last_npc_send = Instant::now();
    }
    state.socket = Some(sock);
}

pub fn apply_remote_block_edits(
    mut state: ResMut<NetClientState>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !state.cfg.enabled || state.pending_block_edits.is_empty() {
        return;
    }
    let my_id = state.client_id;
    let mut edits = std::mem::take(&mut state.pending_block_edits);
    let overflow = edits.split_off(edits.len().min(NET_MAX_BLOCK_EDITS_PER_FRAME));
    state.pending_block_edits = overflow;
    let mut touched_chunks = HashSet::new();
    for (source_client, e) in edits {
        if my_id == Some(source_client) {
            continue;
        }
        if set_block_world(
            &mut world.chunks,
            e.x,
            e.y,
            e.z,
            block_from_net(e.block),
        ) {
            let chunk = IVec2::new(
                div_floor(e.x, crate::config::CHUNK_SIZE as i32),
                div_floor(e.z, crate::config::CHUNK_SIZE as i32),
            );
            touched_chunks.insert(chunk);
        }
    }
    for chunk in touched_chunks {
        remesh_affected_chunks(chunk, &world.chunks, &loaded, &mut meshes);
    }
}

pub fn spawn_net_fx(
    mut commands: Commands,
    mut state: ResMut<NetClientState>,
    assets: Option<Res<NetVisualAssets>>,
) {
    if !state.cfg.enabled || state.pending_fx.is_empty() {
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    let my = state.client_id;
    let fx = std::mem::take(&mut state.pending_fx);
    for f in fx {
        match f {
            NetFxSpawn::Gun {
                source_client,
                from,
                dir,
            } => {
                if my == Some(source_client) {
                    continue;
                }
                commands.spawn((
                    PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.fx_tracer_mat.clone(),
                        transform: Transform {
                            translation: from + dir * 1.0,
                            rotation: Quat::from_rotation_arc(Vec3::Z, dir),
                            scale: Vec3::new(0.05, 0.05, 1.1),
                        },
                        ..default()
                    },
                    NetFx {
                        ttl: 0.08,
                        start_scale: Vec3::new(0.05, 0.05, 1.1),
                        end_scale: Vec3::new(0.02, 0.02, 0.2),
                    },
                ));
            }
            NetFxSpawn::Explosion { at, radius } => {
                commands.spawn((
                    PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.fx_explosion_mat.clone(),
                        transform: Transform {
                            translation: at + Vec3::Y * 0.3,
                            scale: Vec3::splat(0.5),
                            ..default()
                        },
                        ..default()
                    },
                    NetFx {
                        ttl: 0.30,
                        start_scale: Vec3::splat(0.5),
                        end_scale: Vec3::splat((radius * 1.7).max(1.0)),
                    },
                ));
            }
        }
    }
}

pub fn tick_net_fx(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut NetFx, &mut Transform)>,
) {
    let dt = time.delta_seconds();
    for (e, mut fx, mut t) in &mut q {
        fx.ttl -= dt;
        let life = (fx.ttl / 0.30).clamp(0.0, 1.0);
        let k = 1.0 - life;
        t.scale = fx.start_scale.lerp(fx.end_scale, k);
        if fx.ttl <= 0.0 {
            commands.entity(e).despawn_recursive();
        }
    }
}

pub fn sync_remote_entities(
    mut commands: Commands,
    state: Res<NetClientState>,
    assets: Option<Res<NetVisualAssets>>,
    map: Option<ResMut<NetEntityMap>>,
    mut transforms: Query<&mut Transform>,
) {
    if !state.cfg.enabled {
        return;
    }
    let (Some(assets), Some(mut map)) = (assets, map) else {
        return;
    };
    let my_id = state.client_id;

    let mut keep_players = HashSet::new();
    for p in &state.remote_players {
        if Some(p.client_id) == my_id {
            continue;
        }
        keep_players.insert(p.client_id);
        if let Some(e) = map.players.get(&p.client_id).copied() {
            if let Ok(mut t) = transforms.get_mut(e) {
                t.translation = Vec3::new(p.pos[0], p.pos[1], p.pos[2]);
            }
        } else {
            let e = commands
                .spawn((
                    PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: assets.player_mat.clone(),
                        transform: Transform {
                            translation: Vec3::new(p.pos[0], p.pos[1], p.pos[2]),
                            scale: Vec3::new(0.52, 1.72, 0.42),
                            ..default()
                        },
                        ..default()
                    },
                    RemotePlayer {
                        client_id: p.client_id,
                    },
                ))
                .id();
            map.players.insert(p.client_id, e);
        }
    }
    let stale_players: Vec<ClientId> = map
        .players
        .keys()
        .copied()
        .filter(|id| !keep_players.contains(id))
        .collect();
    for id in stale_players {
        if let Some(e) = map.players.remove(&id) {
            commands.entity(e).despawn_recursive();
        }
    }

    let mut keep_npcs = HashSet::new();
    for n in &state.remote_npcs {
        keep_npcs.insert(n.entity_id);
        if let Some(e) = map.npcs.get(&n.entity_id).copied() {
            if let Ok(mut t) = transforms.get_mut(e) {
                t.translation = Vec3::new(n.pos[0], n.pos[1], n.pos[2]);
                let yaw = -n.yaw + std::f32::consts::FRAC_PI_2;
                t.rotation = Quat::from_rotation_y(yaw);
                if n.dead {
                    t.rotation *= Quat::from_rotation_z(1.15);
                }
            }
        } else {
            let mat = match n.kind {
                NpcKindNet::Friendly => assets.npc_friendly_mat.clone(),
                NpcKindNet::Hostile => assets.npc_hostile_mat.clone(),
            };
            let mut transform = Transform {
                translation: Vec3::new(n.pos[0], n.pos[1], n.pos[2]),
                scale: Vec3::new(0.58, 1.22, 0.50),
                ..default()
            };
            if n.dead {
                transform.rotation = Quat::from_rotation_z(1.15);
            }
            let e = commands
                .spawn((
                    PbrBundle {
                        mesh: assets.mesh.clone(),
                        material: mat,
                        transform,
                        ..default()
                    },
                    RemoteNpc { net_id: n.entity_id },
                ))
                .id();
            map.npcs.insert(n.entity_id, e);
        }
    }
    let stale_npcs: Vec<u64> = map
        .npcs
        .keys()
        .copied()
        .filter(|id| !keep_npcs.contains(id))
        .collect();
    for id in stale_npcs {
        if let Some(e) = map.npcs.remove(&id) {
            commands.entity(e).despawn_recursive();
        }
    }
}

fn send(sock: &UdpSocket, server: SocketAddr, msg: &ClientMsg) {
    match serde_json::to_vec(msg) {
        Ok(payload) => {
            if let Err(e) = sock.send_to(&payload, server) {
                warn!("net send error: {e}");
            }
        }
        Err(e) => warn!("net serialize error: {e}"),
    }
}

fn npc_net_id(cell: IVec2) -> u64 {
    let x = cell.x as u32 as u64;
    let z = cell.y as u32 as u64;
    (x << 32) | z
}

fn block_to_net(block: Block) -> BlockIdNet {
    match block {
        Block::Air => BlockIdNet::Air,
        Block::Grass => BlockIdNet::Grass,
        Block::Dirt => BlockIdNet::Dirt,
        Block::Stone => BlockIdNet::Stone,
        Block::Sand => BlockIdNet::Sand,
        Block::Snow => BlockIdNet::Snow,
        Block::Wood => BlockIdNet::Wood,
        Block::Leaves => BlockIdNet::Leaves,
        Block::Red => BlockIdNet::Red,
        Block::Blue => BlockIdNet::Blue,
        Block::Yellow => BlockIdNet::Yellow,
        Block::Purple => BlockIdNet::Purple,
        Block::Cyan => BlockIdNet::Cyan,
    }
}

fn block_from_net(block: BlockIdNet) -> Block {
    match block {
        BlockIdNet::Air => Block::Air,
        BlockIdNet::Grass => Block::Grass,
        BlockIdNet::Dirt => Block::Dirt,
        BlockIdNet::Stone => Block::Stone,
        BlockIdNet::Sand => Block::Sand,
        BlockIdNet::Snow => Block::Snow,
        BlockIdNet::Wood => Block::Wood,
        BlockIdNet::Leaves => Block::Leaves,
        BlockIdNet::Red => Block::Red,
        BlockIdNet::Blue => Block::Blue,
        BlockIdNet::Yellow => Block::Yellow,
        BlockIdNet::Purple => Block::Purple,
        BlockIdNet::Cyan => Block::Cyan,
    }
}
