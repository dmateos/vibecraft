# VibeCraft

VibeCraft is a high-performance Minecraft-like voxel sandbox built in Rust with Bevy.

It focuses on:
- chunked world streaming
- procedural terrain + biomes
- high FPS rendering
- editable world interaction
- live LLM-assisted structure generation

## Features

- Procedural terrain with Perlin-based height, caves, and biome classification
- Chunk meshing and runtime chunk streaming around the camera
- Texture-atlas voxel material with lighting/fog/weather response
- Water and cloud systems
- Day/night cycle and weather blending
- Block break/place interaction with palette cycling
- Basic NPC system:
  - friendly NPCs (can follow with interaction)
  - hostile NPCs (chase/attack)
- Village generation pass (plains/forest-biased, deterministic per seed)

## Requirements

- Rust (stable)
- Cargo
- GPU/API supported by Bevy (Metal/Vulkan/DX12 depending on platform)

## Run

```bash
cargo run
```

## Multiplayer Draft Scaffold

This repo now includes an initial dedicated-server scaffold and protocol draft:
- Design doc: `docs/MULTIPLAYER_DRAFT.md`
- Shared protocol types: `src/net/protocol.rs`
- Shared sim/net mapping helpers: `src/core_sim/net_map.rs`
- Draft server binary: `src/bin/server.rs`

Run the draft server scaffold:

```bash
cargo run --bin server -- --bind 0.0.0.0:40000 --seed 1337 --tick-hz 20
```

Run the draft test client (separate terminal):

```bash
cargo run --bin net_client -- --server 127.0.0.1:40000 --name tester1
```

Current status:
- UDP transport wired
- `Hello` / `Welcome` handshake wired
- periodic snapshot broadcast wired
- test client prints received snapshots

Use the main game binary as a networked client:

```bash
cargo run -- --connect 127.0.0.1:40000 --name player1
```

If `--connect` is omitted, the game runs in normal local mode.

## Controls

### Movement / camera
- `W A S D`: move
- `Mouse`: look
- `Space`: jump
- `Ctrl` (hold): sprint
- `F`: toggle fly mode
- `Esc`: release cursor
- `Left Mouse`: lock cursor

### World / gameplay
- `Left Mouse`: break targeted block
- `Right Mouse`: place selected block
- `Mouse Wheel`: cycle block palette
- `R` or `F5`: reseed/regenerate world
- `F6`: toggle terrain mode (Procedural/Flat)

### NPC / environment
- `E`: interact with friendly NPC (toggle follow)
- `F7`: cycle weather preset
- `F8`: pause/resume day-night cycle

### Generation / debug
- `P`: open/close LLM prompt editing
- `Enter`: submit prompt
- `L`/`T`: trigger live LLM generation
- `J`: load generation request JSON
- `G`: trigger demo generation
- `F3`: toggle debug overlay

## Project structure

- `src/main.rs`: app setup, schedule wiring, global resources
- `src/world/mod.rs`: terrain generation, biome logic, meshing, villages/features
- `src/streaming.rs`: chunk generation + render streaming
- `src/materials.rs`: voxel material definitions
- `src/water.rs`: water mesh/material streaming
- `src/clouds.rs`: cloud streaming/animation
- `src/weather.rs`: weather + day/night lighting integration
- `src/interact.rs`: block targeting/break/place
- `src/player.rs`: player movement/collision/fly mode
- `src/npc.rs`: NPC spawning, AI, interaction, combat
- `src/generation/*`: LLM and manual generation pipeline
- `src/ui.rs`: HUD/debug overlays

## Notes

- After major generation or mapping changes, regenerate with `R`/`F5`.
- Assets are organized under `assets/` with project-owned terrain atlas paths.
- This is currently an engine prototype; systems are intentionally modular for iteration.
