# ADR 0022: Toy-world render snapshot and ABI 3 (TP-011)

- Status: Accepted; toy-world parts superseded by [0040](0040-delete-toy-world-and-old-formats.md)
- Date: 2026-10-17
- Decided by: Claude, under Noah's standing merge authority for ADR PRs (2026-10-17); product scope is fixed by issue #12
- Issue: #12 (TP-011; the ADR PR lands first)
- Supersedes: ABI 2 snapshot and config layout of [ADR 0019](0019-command-journal-and-world-events.md) §8 (command batch format unchanged)

## Context
TP-011 draws the toy world on an iPhone from core snapshots and removes TP-003's debug markers. ABI 2's snapshot has one record kind (a marker in a synthetic frame) and its config carries a marker count. [ADR 0018](0018-rust-unity-bridge.md) requires a version bump and a decision record for any layout change. Constraints: one coarse snapshot fill per frame into caller storage, no per-entity calls, little-endian explicit fields, the core owns state and Unity only renders ([15 §16](../architecture/15-ios-performance-memory.md), [03 §4.2](../architecture/03-rendering-architecture.md)).

## Decision
**ABI version 3.** `ws_abi_version()` returns 3 and every versioned struct and header field is 3. The command batch format (version 2 envelope, opcodes 0 to 3) is unchanged. A v2 client is refused at creation (config) and at snapshot decode (header), as v1 was.

**Config (32 bytes, unchanged size).** `{ abi_version = 3, struct_size = 32, seed: u64, start_us: i64, residents_per_building: u32, reserved: u32 = 0 }`. `residents_per_building` is 1 to 2,000 and sets the toy world's minimum and maximum population per building to that value, so a world has exactly `buildings × residents_per_building` residents. The toy dataset has five buildings, so the three measurement scales are 10, 100 and 1,000 per building: 50, 500 and 5,000 agents. `debug_marker_count` and `ws_sim::debug_marker` are removed.

**Snapshot v3.** Header unchanged in shape: `{ abi_version = 3: u32, header_size = 24: u32, tick: u64, instance_count: u32, record_size = 32: u32 }`. Records are 32 bytes: `id: u64, kind: u32, east: f32, north: f32, up: f32, a: f32, b: f32`.

| kind | meaning | id | a | b |
|---|---|---|---|---|
| 1 | building (a box) | the building's raw `EntityId` | half extent, metres | height, metres |
| 2 | resident (a capsule) | the resident's raw `EntityId` | heading, radians from east towards north (0 when standing still) | 0 |

Positions are metres east, north and up of the **snapshot origin**, the ground centre of the lowest-ID building, in that building's cell ENU frame. The toy world is a few hundred metres across, so one frame is accurate; real geometry and a floating origin are TP-012 and later work. Buildings come first in ascending ID order, then L1 residents in ascending ID order. Residents at L2 have no position and are not drawn, so `instance_count = buildings + L1 residents`. All values are finite. Order and values are deterministic for a given world state.

**Fill contract (unchanged).** One `ws_fill_render_snapshot` call writes the whole frame into caller storage; a short buffer is untouched and `written` reports the required size, so a host sizes its buffer by probing with capacity zero after creation. Fill reads the world and changes nothing (the canonical hash is unchanged). No per-entity calls, callbacks or retained pointers.

**Threading (host rule, not an ABI change).** A handle is not thread-safe; callers serialize every call. The Unity host gives the handle to one worker thread that owns `ws_submit`, `ws_advance`, `ws_get_control_state` and `ws_fill_render_snapshot`. The main thread queues commands for the worker and decodes at most one published snapshot per frame from a buffer the worker is not writing. The spike note records whether this needs more than double buffering.

**Where the code goes.** `ws-sim` gains a read-only `World::render_instances(&self, f: impl FnMut(RenderInstance))` that yields the records above (so `ws-ffi` keeps `ws-sim` as its only dependency); the ENU conversion lives in `ws-world`. `ws-ffi` writes the bytes. The shared fixture `tests/fixtures/snapshot-v3.hex` is decoded by Rust and Unity tests. Unity gets decoders for kinds 1 and 2, the worker host, an instanced renderer, an orbit camera and a debug HUD.

## Consequences
- Easier: the iPhone path draws real world state with one fill per frame; the same records serve later entity kinds by adding kinds, not layouts.
- Harder: ABI 2 clients and the ABI 2 fixtures are refused (kept only to test refusal). The Unity plugin and the generated header must be rebuilt together; a stale plugin fails the version preflight.
- Not decided here: L2 agents in the snapshot, any per-entity query, floating origins, aggregate rendering, FFI exposure of interventions or saves, snapshot compression or delta frames. All wait for measurements.
- Revisit if: the fill exceeds the 2 ms budget on device, 32-byte records are too heavy for a 5,000-agent frame, or the worker/main exchange needs more than double buffering.
