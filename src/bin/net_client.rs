use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use vibecraft::net::protocol::{
    AckSnapshotMsg, ClientMsg, HelloMsg, InputFrameMsg, PROTOCOL_VERSION, ServerMsg,
};

fn main() {
    let mut server: SocketAddr = "127.0.0.1:40000".parse().expect("valid default server addr");
    let mut name = "tester".to_string();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--server" => {
                if let Some(v) = args.next()
                    && let Ok(addr) = v.parse::<SocketAddr>()
                {
                    server = addr;
                }
            }
            "--name" => {
                if let Some(v) = args.next() {
                    name = v;
                }
            }
            _ => {}
        }
    }

    let sock = UdpSocket::bind("0.0.0.0:0").expect("bind client udp");
    sock.set_nonblocking(true).expect("set nonblocking");
    println!("net_client local={} server={}", sock.local_addr().unwrap(), server);

    send_client_msg(
        &sock,
        server,
        &ClientMsg::Hello(HelloMsg {
            protocol_version: PROTOCOL_VERSION,
            player_name: name.clone(),
        }),
    );

    let start = Instant::now();
    let mut last_input = Instant::now();
    let mut input_seq = 1u32;
    let mut buf = [0u8; 8192];

    loop {
        loop {
            let (n, from) = match sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("recv error: {e}");
                    break;
                }
            };

            let msg: ServerMsg = match serde_json::from_slice(&buf[..n]) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("invalid server msg from {from}: {e}");
                    continue;
                }
            };

            match msg {
                ServerMsg::Welcome(w) => {
                    println!(
                        "welcome client_id={} seed={} tick_hz={}",
                        w.your_client_id, w.world_seed, w.server_tick_hz
                    );
                }
                ServerMsg::Snapshot(s) => {
                    println!(
                        "snapshot seq={} tick={} players={}",
                        s.snapshot_seq,
                        s.server_tick,
                        s.players.len()
                    );
                    send_client_msg(
                        &sock,
                        server,
                        &ClientMsg::AckSnapshot(AckSnapshotMsg {
                            snapshot_seq: s.snapshot_seq,
                        }),
                    );
                }
                ServerMsg::ServerNotice(n) => {
                    println!("notice {:?}: {}", n.code, n.message);
                }
                _ => {}
            }
        }

        if last_input.elapsed() >= Duration::from_millis(100) {
            send_client_msg(
                &sock,
                server,
                &ClientMsg::InputFrame(InputFrameMsg {
                    input_seq,
                    client_time_s: start.elapsed().as_secs_f32(),
                    move_x: 0.0,
                    move_z: 0.0,
                    jump_pressed: false,
                    sprint_pressed: false,
                    fire_pressed: false,
                    grenade_pressed: false,
                    break_pressed: false,
                    place_pressed: false,
                    place_block: None,
                    fire_cell: None,
                    break_cell: None,
                    place_cell: None,
                    look_yaw: 0.0,
                    look_pitch: 0.0,
                    view_origin: [0.0, 0.0, 0.0],
                    view_dir: [0.0, 0.0, -1.0],
                }),
            );
            input_seq = input_seq.wrapping_add(1);
            last_input = Instant::now();
        }

        thread::sleep(Duration::from_millis(5));
    }
}

fn send_client_msg(sock: &UdpSocket, server: SocketAddr, msg: &ClientMsg) {
    match serde_json::to_vec(msg) {
        Ok(payload) => {
            if let Err(e) = sock.send_to(&payload, server) {
                eprintln!("send error: {e}");
            }
        }
        Err(e) => eprintln!("serialize client msg error: {e}"),
    }
}
