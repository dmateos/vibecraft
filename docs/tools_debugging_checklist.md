# VibeCraft Tools & Debugging Checklist

Focus area: **Track 7 - Tools and Debugging**

How to use:
- Check `[x]` when complete.
- Keep notes short and specific.
- If scope changes, update acceptance criteria before implementation.

## 7.1 In-Game Debug Overlay
- [x] Add toggleable debug HUD (suggested key: `F3`)
- [x] Show FPS + frame time (avg + 1% low proxy)
- [x] Show chunk stats (loaded, meshed this frame, queued)
- [x] Show generation stats (queue length, ops applied/frame)
- [x] Show player/camera position and current chunk

Acceptance criteria:
- Overlay can be toggled without hitching.
- Values update live and are readable on all backgrounds.

Notes:
- Implemented via `ui::DebugOverlayState`, `ui::FrameStats`, `streaming::StreamingRuntimeStats`, and `generation::GenerationRuntimeStats`.
- Controls: `F3` toggles overlay visibility.

## 7.2 CPU/GPU Timing Instrumentation
- [ ] Add CPU stage timers for update systems (streaming, meshing, render prep)
- [ ] Capture moving averages over 1s / 10s windows
- [ ] Add optional GPU timing path (if backend supports it)
- [ ] Emit periodic summary logs (every N seconds)

Acceptance criteria:
- We can identify top 3 frame-time contributors from logs/overlay.

Notes:
- 

## 7.3 Stutter Capture / Spike Diagnostics
- [ ] Detect frame spikes above threshold (e.g. > 25 ms)
- [ ] Capture spike context (chunk ops, queue sizes, camera speed, mode)
- [ ] Persist rolling spike records to file (`debug/spikes.jsonl`)
- [ ] Add command/key to clear spike history

Acceptance criteria:
- After a stutter session, we can inspect a timestamped spike log.

Notes:
- 

## 7.4 Deterministic Benchmark Path
- [ ] Add benchmark camera path playback mode
- [ ] Make seed + terrain mode + path deterministic
- [ ] Record summary metrics (avg fps, p95 frame ms, max frame ms)
- [ ] Save run outputs to `debug/benchmarks/<timestamp>.json`

Acceptance criteria:
- Re-running benchmark with same config gives comparable metrics.

Notes:
- 

## 7.5 Runtime Config + Hot Reload
- [ ] Add config file for perf/debug tuning (`config/debug.toml`)
- [ ] Support live reload for key values (overlay verbosity, thresholds)
- [ ] Display current active config version/hash in overlay
- [ ] Fail safely with defaults if config is invalid

Acceptance criteria:
- Changing config values updates runtime behavior without restart (where supported).

Notes:
- 

## Progress Tracker
- Overall completion: **1 / 5 sections**
- Last updated: 2026-02-18
- Current focus: **7.2 CPU/GPU Timing Instrumentation**
