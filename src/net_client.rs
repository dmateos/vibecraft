//! In-game network client integration for snapshots and remote entities.
//! Handles UDP session state, local input upload, remote state interpolation,
//! and applying authoritative server block edits into the rendered world.
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use bevy::math::primitives::Cuboid;
use bevy::prelude::*;

use crate::block_edit::{BlockMutationRequest, LocalBlockEditEvent};
use crate::interact::PlacementPalette;
use crate::player::FlyCam;
use crate::world::Block;
use crate::world::{VoxelWorld, get_block_world};
use vibecraft::net::protocol::{
    AckSnapshotMsg, BlockCell, BlockEditsMsg, BlockIdNet, ClientId, ClientMsg, HelloMsg,
    InputFrameMsg, NetEvent, NpcKindNet, NpcStateNet, PROTOCOL_VERSION, PlayerStateNet, ServerMsg,
    SnapshotSeq,
};

const NET_MAX_RECV_PER_FRAME: usize = 64;
const NET_INPUT_SEND_MS: u64 = 100;
const NET_MAX_BLOCK_EDITS_PER_FRAME: usize = 96;
const NET_BLOCK_EDITS_PER_PACKET: usize = 40;
const NET_MAX_BLOCK_PACKETS_PER_TICK: usize = 4;

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
    input_seq: u32,
    hello_sent: bool,
    pub connected: bool,
    pub client_id: Option<ClientId>,
    pub authority_client_id: Option<ClientId>,
    pub last_snapshot: Option<SnapshotSeq>,
    pub remote_players: Vec<PlayerStateNet>,
    pub remote_npcs: Vec<NpcStateNet>,
    pub pending_block_edits: Vec<BlockCell>,
    pub outgoing_block_edits: Vec<BlockCell>,
    pub pending_fx: Vec<NetFxSpawn>,
    latched_fire: bool,
    latched_grenade: bool,
    latched_break: bool,
    latched_place: bool,
    latched_place_block: Option<BlockIdNet>,
    latched_fire_cell: Option<[i32; 3]>,
    latched_break_cell: Option<[i32; 3]>,
    latched_place_cell: Option<[i32; 3]>,
}

impl NetClientState {
    pub fn new(cfg: NetClientConfig) -> Self {
        Self {
            cfg,
            socket: None,
            last_input_send: Instant::now(),
            input_seq: 1,
            hello_sent: false,
            connected: false,
            client_id: None,
            authority_client_id: None,
            last_snapshot: None,
            remote_players: Vec::new(),
            remote_npcs: Vec::new(),
            pending_block_edits: Vec::new(),
            outgoing_block_edits: Vec::new(),
            pending_fx: Vec::new(),
            latched_fire: false,
            latched_grenade: false,
            latched_break: false,
            latched_place: false,
            latched_place_block: None,
            latched_fire_cell: None,
            latched_break_cell: None,
            latched_place_cell: None,
        }
    }

    pub fn is_authority(&self) -> bool {
        !self.cfg.enabled
    }
}

#[inline]
pub fn is_remote_simulation(net: Option<&NetClientState>) -> bool {
    match net {
        Some(net) => net.cfg.enabled && net.connected && !net.is_authority(),
        None => false,
    }
}

#[derive(Resource, Default)]
pub struct NetEntityMap {
    players: HashMap<ClientId, Entity>,
    npcs: HashMap<u64, RemoteNpcBundle>,
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

#[derive(Clone, Copy)]
struct RemoteNpcBundle {
    root: Entity,
    body: Entity,
    head: Entity,
}

#[derive(Component)]
pub struct NetFx {
    ttl: f32,
    start_scale: Vec3,
    end_scale: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub enum NetFxSpawn {
    Gun {
        source_client: ClientId,
        from: Vec3,
        dir: Vec3,
    },
    Explosion {
        at: Vec3,
        radius: f32,
    },
}

pub fn setup_net_client(mut state: ResMut<NetClientState>) {
    if !state.cfg.enabled {
        return;
    }
    if state.socket.is_some() {
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
    info!(
        "net client local={} server={server}",
        sock.local_addr().unwrap_or(server)
    );
    state.socket = Some(sock);
}

pub fn setup_net_visual_assets(
    mut commands: Commands,
    state: Res<NetClientState>,
    existing_assets: Option<Res<NetVisualAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !state.cfg.enabled {
        return;
    }
    if existing_assets.is_some() {
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
    buttons: Res<ButtonInput<MouseButton>>,
    palette: Res<PlacementPalette>,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
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

    // local block edits -> server (only for offline fallback/testing)
    if !state.connected {
        for e in local_block_edits.read() {
            state.outgoing_block_edits.push(BlockCell {
                x: e.x,
                y: e.y,
                z: e.z,
                block: block_to_net(e.block),
            });
        }
    } else {
        local_block_edits.clear();
    }
    let mut packets_sent = 0usize;
    while !state.outgoing_block_edits.is_empty() && packets_sent < NET_MAX_BLOCK_PACKETS_PER_TICK {
        let take = state
            .outgoing_block_edits
            .len()
            .min(NET_BLOCK_EDITS_PER_PACKET);
        let edits = state.outgoing_block_edits.drain(..take).collect();
        send(
            &sock,
            server,
            &ClientMsg::BlockEdits(BlockEditsMsg { edits }),
        );
        packets_sent += 1;
    }

    // Latch one-shot gameplay intents every frame so 10Hz send loop doesn't drop clicks/presses.
    if keys.just_pressed(KeyCode::KeyE) {
        state.latched_fire = true;
        if let Ok(cam) = cam_q.get_single()
            && let Some((solid, _)) = raycast_target(
                cam.translation,
                *cam.forward(),
                &world.chunks,
                crate::config::BREAK_REACH * 2.3,
            )
        {
            state.latched_fire_cell = Some([solid.x, solid.y, solid.z]);
        }
    }
    state.latched_grenade |= keys.just_pressed(KeyCode::KeyQ);
    if buttons.just_pressed(MouseButton::Left) {
        state.latched_break = true;
        if let Ok(cam) = cam_q.get_single()
            && let Some((solid, _)) = raycast_target(
                cam.translation,
                *cam.forward(),
                &world.chunks,
                crate::config::BREAK_REACH,
            )
        {
            state.latched_break_cell = Some([solid.x, solid.y, solid.z]);
        }
    }
    if buttons.just_pressed(MouseButton::Right) {
        state.latched_place = true;
        state.latched_place_block = Some(block_to_net(palette.selected_block()));
        if let Ok(cam) = cam_q.get_single()
            && let Some((_solid, prev)) = raycast_target(
                cam.translation,
                *cam.forward(),
                &world.chunks,
                crate::config::BREAK_REACH,
            )
        {
            state.latched_place_cell = Some([prev.x, prev.y, prev.z]);
        }
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
                state.authority_client_id = None;
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
                            if state.client_id != Some(source_client) {
                                state.pending_block_edits.push(BlockCell { x, y, z, block });
                            }
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
            ServerMsg::ChunkDelta(delta) => {
                state.pending_block_edits.extend(delta.edits);
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
        let move_x =
            (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
        let move_z =
            (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32;
        let view_dir = *cam.forward();
        let input = ClientMsg::InputFrame(InputFrameMsg {
            input_seq: state.input_seq,
            client_time_s: time.elapsed_seconds(),
            move_x,
            move_z,
            jump_pressed: keys.pressed(KeyCode::Space),
            sprint_pressed: keys.pressed(KeyCode::ControlLeft),
            fire_pressed: state.latched_fire,
            grenade_pressed: state.latched_grenade,
            break_pressed: state.latched_break,
            place_pressed: state.latched_place,
            place_block: state.latched_place_block,
            fire_cell: state.latched_fire_cell,
            break_cell: state.latched_break_cell,
            place_cell: state.latched_place_cell,
            look_yaw: 0.0,
            look_pitch: 0.0,
            view_origin: [cam.translation.x, cam.translation.y, cam.translation.z],
            view_dir: [view_dir.x, view_dir.y, view_dir.z],
        });
        send(&sock, server, &input);
        state.latched_fire = false;
        state.latched_grenade = false;
        state.latched_break = false;
        state.latched_place = false;
        state.latched_place_block = None;
        state.latched_fire_cell = None;
        state.latched_break_cell = None;
        state.latched_place_cell = None;
        state.input_seq = state.input_seq.wrapping_add(1);
        state.last_input_send = Instant::now();
    }

    state.socket = Some(sock);
}

pub fn apply_remote_block_edits(
    mut state: ResMut<NetClientState>,
    mut mutation_requests: EventWriter<BlockMutationRequest>,
) {
    if !state.cfg.enabled || state.pending_block_edits.is_empty() {
        return;
    }
    let mut edits = std::mem::take(&mut state.pending_block_edits);
    let overflow = edits.split_off(edits.len().min(NET_MAX_BLOCK_EDITS_PER_FRAME));
    state.pending_block_edits = overflow;
    for e in edits {
        mutation_requests.send(BlockMutationRequest {
            x: e.x,
            y: e.y,
            z: e.z,
            block: block_from_net(e.block),
            emit_local_event: false,
        });
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
        if let Some(bundle) = map.npcs.get(&n.entity_id).copied() {
            if let Ok(mut t) = transforms.get_mut(bundle.root) {
                t.translation = Vec3::new(n.pos[0], n.pos[1], n.pos[2]);
                let yaw = -n.yaw + std::f32::consts::FRAC_PI_2;
                t.rotation = Quat::from_rotation_y(yaw);
                if n.dead {
                    t.rotation *= Quat::from_rotation_z(1.15);
                }
            }
            let body_scale = match n.kind {
                NpcKindNet::Friendly => Vec3::new(0.58, 0.86, 0.34),
                NpcKindNet::Hostile => Vec3::new(0.84, 0.52, 1.02),
            };
            let head_scale = match n.kind {
                NpcKindNet::Friendly => Vec3::new(0.34, 0.34, 0.34),
                NpcKindNet::Hostile => Vec3::new(0.42, 0.30, 0.50),
            };
            let head_pos = match n.kind {
                NpcKindNet::Friendly => Vec3::new(0.0, 1.44, 0.0),
                NpcKindNet::Hostile => Vec3::new(0.0, 0.90, -0.38),
            };
            if let Ok(mut t) = transforms.get_mut(bundle.body) {
                t.scale = body_scale;
            }
            if let Ok(mut t) = transforms.get_mut(bundle.head) {
                t.translation = head_pos;
                t.scale = head_scale;
            }
        } else {
            let mat = match n.kind {
                NpcKindNet::Friendly => assets.npc_friendly_mat.clone(),
                NpcKindNet::Hostile => assets.npc_hostile_mat.clone(),
            };
            let mut transform =
                Transform::from_translation(Vec3::new(n.pos[0], n.pos[1], n.pos[2]));
            if n.dead {
                transform.rotation = Quat::from_rotation_z(1.15);
            }
            let root = commands
                .spawn((
                    SpatialBundle {
                        transform,
                        ..default()
                    },
                    RemoteNpc {
                        net_id: n.entity_id,
                    },
                ))
                .id();
            let body = commands
                .spawn((PbrBundle {
                    mesh: assets.mesh.clone(),
                    material: mat.clone(),
                    transform: Transform {
                        translation: Vec3::new(0.0, 0.92, 0.0),
                        scale: match n.kind {
                            NpcKindNet::Friendly => Vec3::new(0.58, 0.86, 0.34),
                            NpcKindNet::Hostile => Vec3::new(0.84, 0.52, 1.02),
                        },
                        ..default()
                    },
                    ..default()
                },))
                .id();
            let head = commands
                .spawn(PbrBundle {
                    mesh: assets.mesh.clone(),
                    material: mat,
                    transform: Transform {
                        translation: match n.kind {
                            NpcKindNet::Friendly => Vec3::new(0.0, 1.44, 0.0),
                            NpcKindNet::Hostile => Vec3::new(0.0, 0.90, -0.38),
                        },
                        scale: match n.kind {
                            NpcKindNet::Friendly => Vec3::new(0.34, 0.34, 0.34),
                            NpcKindNet::Hostile => Vec3::new(0.42, 0.30, 0.50),
                        },
                        ..default()
                    },
                    ..default()
                })
                .id();
            commands.entity(root).add_child(body).add_child(head);
            map.npcs
                .insert(n.entity_id, RemoteNpcBundle { root, body, head });
        }
    }
    let stale_npcs: Vec<u64> = map
        .npcs
        .keys()
        .copied()
        .filter(|id| !keep_npcs.contains(id))
        .collect();
    for id in stale_npcs {
        if let Some(bundle) = map.npcs.remove(&id) {
            commands.entity(bundle.root).despawn_recursive();
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

fn raycast_target(
    origin: Vec3,
    dir: Vec3,
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    max_dist: f32,
) -> Option<(IVec3, IVec3)> {
    let step = 0.05;
    let mut t = 0.0;
    let mut last_cell = IVec3::new(i32::MIN, i32::MIN, i32::MIN);
    let mut last_air = IVec3::new(i32::MIN, i32::MIN, i32::MIN);

    while t <= max_dist {
        let p = origin + dir * t;
        let cell = IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if cell == last_cell {
            t += step;
            continue;
        }
        last_cell = cell;

        if get_block_world(chunks, cell.x, cell.y, cell.z) == Block::Air {
            last_air = cell;
            t += step;
            continue;
        }
        if last_air.x == i32::MIN {
            return None;
        }
        return Some((cell, last_air));
    }
    None
}
