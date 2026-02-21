# VibeCraft

VibeCraft is a high-performance voxel sandbox prototype built in Rust with Bevy.

The project focuses on:
- large streamed worlds with high FPS
- chunk-based generation and meshing
- responsive block editing and combat tools
- modular systems that can be split into client/server

## Current Gameplay Features

- Procedural terrain with Perlin-based elevation, cave carving, and biome selection
- Chunk streaming and remeshing around the player camera
- Texture-atlas terrain material with directional lighting, fog, and weather tinting
- Day/night cycle with weather presets
- Clouds and water rendering
- Landmarks and authored world features (castle/city/settlement style content)
- NPC simulation with friendly and hostile behaviors
- Gun and grenade gameplay (projectiles, explosion work queue, terrain destruction)
- Inventory pickup loop:
  - breaking or shooting a block collects that block type
  - placement consumes inventory count
  - bottom hotbar UI shows selected slot + counts
  - mouse wheel cycles selected slot

## Requirements

- Rust stable toolchain
- Cargo
- GPU backend supported by Bevy (Metal, Vulkan, or DX12 depending on platform)

## Running

This workspace has multiple binaries (`vibecraft`, `server`, `net_client`).
Use explicit `--bin` selection.

Run the main game client:

```bash
cargo run --bin vibecraft
```

Run optimized for smoother frame pacing:

```bash
cargo run --release --bin vibecraft
```

## Controls

### Movement and Camera
- `W A S D`: move
- `Mouse`: look
- `Space`: jump
- `Ctrl` (hold): sprint
- `F`: toggle fly mode
- `Esc`: release cursor
- `Left Mouse` (when cursor is free): re-lock cursor

### Build and Combat
- `Left Mouse`: break targeted block (adds to inventory)
- `Right Mouse`: place selected block (consumes inventory)
- `Mouse Wheel`: cycle hotbar selection
- `E`: fire gun (also used for NPC interaction when applicable)
- `Q`: throw grenade

### World and Environment
- `R` or `F5`: reseed/regenerate world
- `F6`: toggle terrain mode (Procedural/Flat)
- `F7`: cycle weather preset
- `F8`: pause/resume day-night cycle

### Generation and Debug
- `P`: toggle prompt editing mode
- `Enter`: submit prompt
- `L` / `T`: trigger live LLM generation
- `J`: load generation request JSON from assets
- `G`: trigger demo generation
- `F3`: toggle debug overlay

## Multiplayer Draft Status

The repo includes a draft dedicated-server path and protocol scaffold.

Relevant files:
- `docs/MULTIPLAYER_DRAFT.md`
- `src/net/protocol.rs`
- `src/bin/server.rs`
- `src/bin/net_client.rs`
- `src/net_client.rs`

Run server scaffold:

```bash
cargo run --bin server -- --bind 0.0.0.0:40000 --seed 1337 --tick-hz 20
```

Run lightweight protocol test client:

```bash
cargo run --bin net_client -- --server 127.0.0.1:40000 --name tester1
```

Run game client connected to server:

```bash
cargo run --bin vibecraft -- --connect 127.0.0.1:40000 --name player1
```

If `--connect` is omitted, game runs in local mode.

## Code Layout

### Core

- `src/main.rs`: app bootstrap, resource insertion, system schedule wiring
- `src/config.rs`: central gameplay/world tuning constants
- `src/block_edit.rs`: unified block mutation pipeline (event-driven dedup + batched remeshing)
- `src/physics.rs`: shared AABB collision queries for player and NPC systems
- `src/perception.rs`: shared line-of-sight and FOV checks

### World

- `src/world/mod.rs`: module re-exports
- `src/world/voxel.rs`: Block/Chunk types, VoxelWorld resource, block read/write helpers
- `src/world/generation.rs`: Perlin-based terrain generation with biome classification
- `src/world/landmarks.rs`: procedural structures (villages, monuments, cities, mazes, pyramids, observatories)
- `src/world/meshing.rs`: chunk mesh building with LOD and ambient occlusion

### NPC

- `src/npc/mod.rs`: module re-exports and NPC constants
- `src/npc/components.rs`: NPC data types, resources (vitals, stimulus, UI state), asset setup
- `src/npc/ai.rs`: NPC tick simulation, player interaction, AI decision logic
- `src/npc/spawn.rs`: camera-driven NPC streaming and entity spawning
- `src/npc/movement.rs`: pathfinding, collision helpers, step-climbing, heading selection
- `src/npc/debug.rs`: debug gizmo visualization for NPC state

### Environment

- `src/streaming.rs`: camera-driven chunk load/generate/mesh orchestration
- `src/materials.rs`: terrain material/shader bindings
- `src/weather.rs`: weather + day/night blending into terrain/cloud/water materials
- `src/sky.rs`: sky dome/discs/stars setup and animation
- `src/water.rs`: water surface rendering and optional flow simulation
- `src/clouds.rs`: cloud chunk spawning, mesh generation, animation
- `src/foliage.rs`: tree and vegetation rendering

### Gameplay

- `src/player.rs`: first-person movement, collision, fly mode
- `src/interact.rs`: raycast targeting, break/place, hotbar palette, inventory resource
- `src/weapons.rs`: gun, bullets, grenades, explosion processing and visual effects
- `src/ui.rs`: crosshair, HUD, hotbar, debug overlay

### Generation

- `src/generation/mod.rs`: module re-exports and config resources
- `src/generation/schema.rs`: serde-based generation request schema
- `src/generation/planner.rs`: converts user intent to structured operation sequences
- `src/generation/compiler.rs`: compiles generation ops (boxes, cylinders, spheres) into block edits
- `src/generation/validator.rs`: bounds and safety validation rules
- `src/generation/executor.rs`: generation queue execution
- `src/generation/live.rs`: LLM prompt editing and HTTP polling

### Networking

- `src/net/protocol.rs`: shared networking protocol and types
- `src/net_client.rs`: in-game network client state sync and remote visuals
- `src/core_sim/net_map.rs`: block enum network conversion helpers
- `src/bin/server.rs`: dedicated server scaffold
- `src/bin/net_client.rs`: lightweight protocol test client

## Notes

- This is an actively iterated prototype, so systems are optimized for clarity and modular refactors.
- Most simulation/rendering modules are intentionally isolated so features can be tuned or replaced without full-engine rewrites.
