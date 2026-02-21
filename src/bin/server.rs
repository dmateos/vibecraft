//! Headless dedicated-server scaffold for multiplayer experiments.
//! Owns simple authoritative state, processes client input frames, and
//! broadcasts snapshots/block edits/events over the shared wire protocol.
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use vibecraft::net::protocol::{
    BlockCell, ChunkDeltaMsg, ClientId, ClientMsg, HelloMsg, NetEvent, NpcStateNet, PlayerStateNet,
    ProjectileStateNet, PROTOCOL_VERSION, ServerMsg, SERVER_TICK_HZ, SnapshotSeq, WelcomeMsg, WorldSnapshotMsg,
};

const SERVER_SNAPSHOT_EVERY_TICKS: u64 = 2;
const SERVER_MAX_RECV_PER_TICK: usize = 256;

#[derive(Debug, Clone, Copy)]
struct ServerConfig {
    bind_addr: SocketAddr,
    port: u16,
    seed: u32,
    tick_hz: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let bind_addr: SocketAddr = "0.0.0.0:40000".parse().expect("valid default bind addr");
        Self {
            bind_addr,
            port: 40000,
            seed: 1337,
            tick_hz: SERVER_TICK_HZ,
        }
    }
}

fn main() {
    let cfg = parse_args();
    let socket = UdpSocket::bind(cfg.bind_addr).expect("bind udp socket");
    socket
        .set_nonblocking(true)
        .expect("set nonblocking socket");
    println!(
        "vibecraft-server boot: protocol=v{} bind={} seed={} tick_hz={}",
        PROTOCOL_VERSION, cfg.bind_addr, cfg.seed, cfg.tick_hz
    );

    let tick_dt = Duration::from_secs_f64(1.0 / cfg.tick_hz as f64);
    let mut tick: u64 = 0;
    let start = Instant::now();
    let mut next = Instant::now();
    let mut last_log = Instant::now();
    let mut next_snapshot: SnapshotSeq = 1;
    let mut clients: HashMap<SocketAddr, ClientConn> = HashMap::new();
    let mut next_client_id: ClientId = 1;
    let mut pending_events: Vec<NetEvent> = Vec::new();
    let mut pending_chunk_edits: HashMap<[i32; 2], HashMap<(i32, i32, i32), vibecraft::net::protocol::BlockIdNet>> =
        HashMap::new();
    let mut server_npcs: Vec<ServerNpc> = init_server_npcs(cfg.seed);

    loop {
        pump_incoming(
            &socket,
            cfg.seed,
            cfg.tick_hz,
            &mut clients,
            &mut next_client_id,
            start.elapsed().as_secs_f32(),
            &mut pending_events,
            &mut pending_chunk_edits,
        );

        tick += 1;
        tick_server_npcs(&mut server_npcs, 1.0 / cfg.tick_hz as f32);
        if tick % SERVER_SNAPSHOT_EVERY_TICKS == 0 {
            broadcast_snapshot(
                &socket,
                &mut clients,
                next_snapshot,
                tick,
                start.elapsed().as_secs_f32(),
                &mut pending_events,
                &mut pending_chunk_edits,
                &server_npcs,
            );
            next_snapshot = next_snapshot.wrapping_add(1);
        }

        if last_log.elapsed() >= Duration::from_secs(1) {
            println!(
                "server tick={} uptime_s={:.1} clients={}",
                tick,
                tick as f64 / cfg.tick_hz as f64,
                clients.len()
            );
            last_log = Instant::now();
        }

        next += tick_dt;
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}

#[derive(Debug, Clone)]
struct ClientConn {
    id: ClientId,
    name: String,
    last_seen: Instant,
    last_input_time_s: f32,
    pos: [f32; 3],
    yaw: f32,
    pitch: f32,
    hp: f32,
    dead: bool,
}

fn pump_incoming(
    socket: &UdpSocket,
    seed: u32,
    tick_hz: u16,
    clients: &mut HashMap<SocketAddr, ClientConn>,
    next_client_id: &mut ClientId,
    server_time_s: f32,
    pending_events: &mut Vec<NetEvent>,
    pending_chunk_edits: &mut HashMap<[i32; 2], HashMap<(i32, i32, i32), vibecraft::net::protocol::BlockIdNet>>,
) {
    let mut buf = [0u8; 8192];
    let mut recv_count = 0usize;
    loop {
        if recv_count >= SERVER_MAX_RECV_PER_TICK {
            break;
        }
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) => {
                eprintln!("recv_from error: {e}");
                break;
            }
        };
        recv_count += 1;

        let msg: ClientMsg = match serde_json::from_slice(&buf[..n]) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("drop invalid packet from {from}: {e}");
                continue;
            }
        };

        match msg {
            ClientMsg::Hello(HelloMsg {
                protocol_version,
                player_name,
            }) => {
                if protocol_version != PROTOCOL_VERSION {
                    let notice = ServerMsg::ServerNotice(vibecraft::net::protocol::ServerNoticeMsg {
                        code: vibecraft::net::protocol::NoticeCode::ProtocolMismatch,
                        message: format!(
                            "protocol mismatch server={} client={}",
                            PROTOCOL_VERSION, protocol_version
                        ),
                    });
                    send_msg(socket, from, &notice);
                    continue;
                }

                let entry = clients.entry(from).or_insert_with(|| {
                    let id = *next_client_id;
                    *next_client_id += 1;
                    ClientConn {
                        id,
                        name: player_name.clone(),
                        last_seen: Instant::now(),
                        last_input_time_s: 0.0,
                        pos: [0.0, 80.0, 0.0],
                        yaw: 0.0,
                        pitch: 0.0,
                        hp: 100.0,
                        dead: false,
                    }
                });
                entry.name = player_name;
                entry.last_seen = Instant::now();

                let welcome = ServerMsg::Welcome(WelcomeMsg {
                    protocol_version: PROTOCOL_VERSION,
                    your_client_id: entry.id,
                    world_seed: seed,
                    server_tick_hz: tick_hz,
                    server_time_s,
                });
                send_msg(socket, from, &welcome);
            }
            ClientMsg::InputFrame(input) => {
                if let Some(c) = clients.get_mut(&from) {
                    c.last_seen = Instant::now();
                    c.last_input_time_s = input.client_time_s;
                    c.pos = input.view_origin;
                    c.yaw = input.look_yaw;
                    c.pitch = input.look_pitch;
                    if input.fire_pressed {
                        pending_events.push(NetEvent::GunFired {
                            shooter_entity: c.id as u64,
                            from: input.view_origin,
                            dir: input.view_dir,
                        });
                        if let Some(p) = input.fire_cell {
                            let chunk = [
                                div_floor(p[0], vibecraft::config::CHUNK_SIZE as i32),
                                div_floor(p[2], vibecraft::config::CHUNK_SIZE as i32),
                            ];
                            pending_chunk_edits
                                .entry(chunk)
                                .or_default()
                                .insert((p[0], p[1], p[2]), vibecraft::net::protocol::BlockIdNet::Air);
                        }
                    }
                    if input.grenade_pressed {
                        let at = [
                            input.view_origin[0] + input.view_dir[0] * 7.0,
                            input.view_origin[1] + input.view_dir[1] * 7.0,
                            input.view_origin[2] + input.view_dir[2] * 7.0,
                        ];
                        pending_events.push(NetEvent::Explosion { at, radius: 4.0 });
                        enqueue_server_explosion(
                            pending_chunk_edits,
                            at,
                            4,
                            vibecraft::net::protocol::BlockIdNet::Air,
                        );
                    }
                    if input.break_pressed {
                        if let Some(p) = input.break_cell {
                            let chunk = [
                                div_floor(p[0], vibecraft::config::CHUNK_SIZE as i32),
                                div_floor(p[2], vibecraft::config::CHUNK_SIZE as i32),
                            ];
                            pending_chunk_edits
                                .entry(chunk)
                                .or_default()
                                .insert((p[0], p[1], p[2]), vibecraft::net::protocol::BlockIdNet::Air);
                        }
                    }
                    if input.place_pressed {
                        if let Some(p) = input.place_cell {
                            let place = input
                                .place_block
                                .unwrap_or(vibecraft::net::protocol::BlockIdNet::Stone);
                            let chunk = [
                                div_floor(p[0], vibecraft::config::CHUNK_SIZE as i32),
                                div_floor(p[2], vibecraft::config::CHUNK_SIZE as i32),
                            ];
                            pending_chunk_edits
                                .entry(chunk)
                                .or_default()
                                .insert((p[0], p[1], p[2]), place);
                        }
                    }
                }
            }
            ClientMsg::AckSnapshot(_) => {
                if let Some(c) = clients.get_mut(&from) {
                    c.last_seen = Instant::now();
                }
            }
            ClientMsg::BlockEdits(blocks) => {
                if let Some(c) = clients.get_mut(&from) {
                    c.last_seen = Instant::now();
                    for e in blocks.edits {
                        let chunk = [
                            div_floor(e.x, vibecraft::config::CHUNK_SIZE as i32),
                            div_floor(e.z, vibecraft::config::CHUNK_SIZE as i32),
                        ];
                        pending_chunk_edits
                            .entry(chunk)
                            .or_default()
                            .insert((e.x, e.y, e.z), e.block);
                    }
                }
            }
            ClientMsg::NpcSync(_) => {}
            ClientMsg::Chat(chat) => {
                println!("chat {}: {}", from, chat.text);
                if let Some(c) = clients.get_mut(&from) {
                    c.last_seen = Instant::now();
                }
            }
        }
    }

    let timeout = Duration::from_secs(10);
    clients.retain(|addr, c| {
        let alive = c.last_seen.elapsed() <= timeout;
        if !alive {
            println!("client timed out: {} ({})", c.id, addr);
        }
        alive
    });
}

fn broadcast_snapshot(
    socket: &UdpSocket,
    clients: &mut HashMap<SocketAddr, ClientConn>,
    seq: SnapshotSeq,
    tick: u64,
    server_time_s: f32,
    pending_events: &mut Vec<NetEvent>,
    pending_chunk_edits: &mut HashMap<[i32; 2], HashMap<(i32, i32, i32), vibecraft::net::protocol::BlockIdNet>>,
    server_npcs: &[ServerNpc],
) {
    if clients.is_empty() {
        return;
    }

    let players: Vec<PlayerStateNet> = clients
        .iter()
        .map(|(_addr, c)| PlayerStateNet {
            entity_id: c.id as u64,
            client_id: c.id,
            pos: c.pos,
            vel: [0.0, 0.0, 0.0],
            yaw: c.yaw,
            pitch: c.pitch,
            hp: c.hp,
            dead: c.dead,
        })
        .collect();

    let snap = ServerMsg::Snapshot(WorldSnapshotMsg {
        snapshot_seq: seq,
        server_tick: tick,
        server_time_s,
        players,
        npcs: server_npcs.iter().map(|n| n.to_net()).collect(),
        projectiles: Vec::<ProjectileStateNet>::new(),
    });

    let batch = if pending_events.is_empty() {
        None
    } else {
        Some(ServerMsg::EventBatch(vibecraft::net::protocol::EventBatchMsg {
            events: std::mem::take(pending_events),
        }))
    };

    let chunk_deltas: Vec<ServerMsg> = pending_chunk_edits
        .drain()
        .map(|(chunk, edits)| {
            let edits = edits
                .into_iter()
                .map(|((x, y, z), block)| BlockCell { x, y, z, block })
                .collect();
            ServerMsg::ChunkDelta(ChunkDeltaMsg { chunk, edits })
        })
        .collect();

    for addr in clients.keys().copied().collect::<Vec<_>>() {
        send_msg(socket, addr, &snap);
        if let Some(ref b) = batch {
            send_msg(socket, addr, b);
        }
        for delta in &chunk_deltas {
            send_msg(socket, addr, delta);
        }
    }
}

#[inline]
fn div_floor(a: i32, b: i32) -> i32 {
    let mut q = a / b;
    let r = a % b;
    if r != 0 && ((r > 0) != (b > 0)) {
        q -= 1;
    }
    q
}

#[derive(Clone, Debug)]
struct ServerNpc {
    id: u64,
    pos: [f32; 3],
    yaw: f32,
    speed: f32,
    turn_timer: f32,
    kind: vibecraft::net::protocol::NpcKindNet,
    dead: bool,
}

impl ServerNpc {
    fn to_net(&self) -> NpcStateNet {
        NpcStateNet {
            entity_id: self.id,
            pos: self.pos,
            yaw: self.yaw,
            kind: self.kind,
            hp: if self.dead { 0.0 } else { 100.0 },
            dead: self.dead,
        }
    }
}

fn init_server_npcs(seed: u32) -> Vec<ServerNpc> {
    let mut v = Vec::new();
    for i in 0..24u64 {
        let h = hash_u64((seed as u64) ^ (i * 0x9E37_79B97F4A7C15));
        let r = 18.0 + (h as f32 / u64::MAX as f32) * 120.0;
        let a = ((h.rotate_left(17) as f32) / u64::MAX as f32) * std::f32::consts::TAU;
        v.push(ServerNpc {
            id: i + 1,
            pos: [a.cos() * r, 82.0, a.sin() * r],
            yaw: a + std::f32::consts::PI,
            speed: if (h & 1) == 0 { 2.3 } else { 2.9 },
            turn_timer: 0.6 + ((h >> 12) as f32 / u64::MAX as f32) * 2.6,
            kind: if (h & 7) == 0 {
                vibecraft::net::protocol::NpcKindNet::Hostile
            } else {
                vibecraft::net::protocol::NpcKindNet::Friendly
            },
            dead: false,
        });
    }
    v
}

fn tick_server_npcs(npcs: &mut [ServerNpc], dt: f32) {
    for n in npcs {
        if n.dead {
            continue;
        }
        n.turn_timer -= dt;
        if n.turn_timer <= 0.0 {
            let h = hash_u64((n.id << 32) ^ ((n.pos[0].to_bits() as u64) << 1) ^ n.pos[2].to_bits() as u64);
            let r = ((h as f32 / u64::MAX as f32) - 0.5) * 1.2;
            n.yaw += r;
            n.turn_timer = 0.8 + (((h.rotate_left(9) as f32) / u64::MAX as f32) * 2.4);
        }
        let dir = [n.yaw.cos(), n.yaw.sin()];
        n.pos[0] += dir[0] * n.speed * dt;
        n.pos[2] += dir[1] * n.speed * dt;
    }
}

#[inline]
fn hash_u64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^ (x >> 33)
}

fn enqueue_server_explosion(
    pending_chunk_edits: &mut HashMap<[i32; 2], HashMap<(i32, i32, i32), vibecraft::net::protocol::BlockIdNet>>,
    at: [f32; 3],
    radius: i32,
    block: vibecraft::net::protocol::BlockIdNet,
) {
    let cx = at[0].floor() as i32;
    let cy = at[1].floor() as i32;
    let cz = at[2].floor() as i32;
    let r2 = radius * radius;
    for z in cz - radius..=cz + radius {
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let dx = x - cx;
                let dy = y - cy;
                let dz = z - cz;
                if dx * dx + dy * dy + dz * dz > r2 {
                    continue;
                }
                let chunk = [
                    div_floor(x, vibecraft::config::CHUNK_SIZE as i32),
                    div_floor(z, vibecraft::config::CHUNK_SIZE as i32),
                ];
                pending_chunk_edits
                    .entry(chunk)
                    .or_default()
                    .insert((x, y, z), block);
            }
        }
    }
}


fn send_msg(socket: &UdpSocket, addr: SocketAddr, msg: &ServerMsg) {
    let payload = match serde_json::to_vec(msg) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("serialize server msg failed: {e}");
            return;
        }
    };
    if let Err(e) = socket.send_to(&payload, addr) {
        eprintln!("send_to {addr} failed: {e}");
    }
}

fn parse_args() -> ServerConfig {
    let mut cfg = ServerConfig::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => {
                if let Some(v) = args.next()
                    && let Ok(bind_addr) = v.parse::<SocketAddr>()
                {
                    cfg.bind_addr = bind_addr;
                    cfg.port = bind_addr.port();
                }
            }
            "--port" => {
                if let Some(v) = args.next()
                    && let Ok(port) = v.parse::<u16>()
                {
                    cfg.port = port;
                    cfg.bind_addr = SocketAddr::from((cfg.bind_addr.ip(), port));
                }
            }
            "--seed" => {
                if let Some(v) = args.next()
                    && let Ok(seed) = v.parse::<u32>()
                {
                    cfg.seed = seed;
                }
            }
            "--tick-hz" => {
                if let Some(v) = args.next()
                    && let Ok(tick_hz) = v.parse::<u16>()
                {
                    cfg.tick_hz = tick_hz.max(1);
                }
            }
            _ => {}
        }
    }

    cfg
}
