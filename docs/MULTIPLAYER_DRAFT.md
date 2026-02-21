# VibeCraft Multiplayer Draft (Server Authoritative)

## Goals

- Two players can join the same world session.
- Both players can move, edit blocks, and shoot.
- Server is authoritative for world, combat, and NPC state.
- Keep it simple first; scale later.

## Non-goals (Phase 1)

- Matchmaking/lobbies
- Accounts/auth
- Voice/chat moderation
- Perfect anti-cheat

## Architecture

- `vibecraft-server`: headless Bevy app or fixed-tick simulation loop.
- `vibecraft-client`: current game + network sync layer.
- Transport:
  - Start: UDP with reliability channels (`renet` recommended).
  - Optional later: QUIC (`bevy_quinnet`) for simpler reliability.

Server owns:
- World chunks + block edits
- Player canonical transforms/health
- Bullets/hits/explosions
- NPC spawning/AI/death state

Client owns:
- Input capture
- Local prediction for own movement/shots
- Visual interpolation for remote entities

## Tick Model

- Server tick: `20 Hz` (`50 ms`)
- Client render: uncapped (or VSync)
- Snapshot send: `10-20 Hz` depending on bandwidth budget

## Interest Management (AOI)

- Per-player chunk subscriptions around current chunk.
- For each player:
  - `subscribe` chunks entering AOI
  - `unsubscribe` chunks leaving AOI
- Server sends:
  - chunk baseline on subscribe
  - chunk deltas for subsequent edits

## Data Ownership

- World generation:
  - Deterministic seed known by all peers.
  - Server sends only edits/events, not full world continuously.
- Canonical state:
  - Server state always wins.
  - Client prediction corrected with smoothing.

## Packet Schema (Draft)

See `src/net/protocol.rs` for Rust structs/enums.

Top-level:
- `ClientMsg`
  - `Hello`
  - `InputFrame`
  - `AckSnapshot`
  - `Chat` (optional)
- `ServerMsg`
  - `Welcome`
  - `Snapshot`
  - `ChunkBaseline`
  - `ChunkDelta`
  - `EventBatch`
  - `ServerNotice`

Events include:
- block set/break
- gun fired
- bullet hit block/NPC/player
- NPC died
- player damaged/dead
- explosion

## Channels (Recommended)

- Reliable ordered:
  - connect/hello/welcome
  - chunk subscribe baselines
  - block edits
  - damage/death events
- Unreliable sequenced:
  - snapshots (entity transforms)
  - frequent input acks

## Shooting Model

- Client sends fire intent (input frame contains `fire_pressed` + origin/look).
- Server performs authoritative raycast/hit.
- Server emits damage/death event.
- Client predicts muzzle flash immediately.
- Client reconciles when server event arrives.

## Block Editing Model

- Client sends edit intent: break/place at targeted cell.
- Server validates:
  - reach
  - rate-limit
  - occupancy rules
- Server applies edit and broadcasts chunk delta.

## NPC Model

- NPCs simulated server-side only.
- Client receives:
  - transform + animation state
  - dead/alive flag
  - optional target mode enum (for debug)

## Persistence (Phase 2)

- Save `world_seed + block_deltas + entity_state`.
- Incremental autosave every N seconds.

## Rollout Plan

1. Foundation
- Create shared `net::protocol`.
- Add `client_id`, `net_entity_id`, snapshot sequence numbers.

2. Join + movement sync
- Client connect/hello.
- Server welcome with seed/time/state.
- Replicate player transforms.

3. Chunk AOI + block replication
- Chunk baseline on subscribe.
- Delta replication for edits.

4. Shooting + damage replication
- Fire intent to server.
- Authoritative hit + damage events.

5. NPC replication
- Server-only AI.
- Client interpolate NPC transforms and death states.

## Risks / Tradeoffs

- Bevy ECS state split between client/server must remain explicit.
- Bandwidth can spike with chunk baselines; AOI throttling required.
- Prediction/reconciliation adds complexity but is required for good gun feel.

## Minimal Success Criteria

- Two clients connect to one server.
- Both see each other moving with acceptable smoothness.
- Block edits replicate both ways.
- Shots from either player can damage/kill NPCs and other players.

