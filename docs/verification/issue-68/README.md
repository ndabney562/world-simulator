# VS-004 schedule implementation verification

Scope: [#68](https://github.com/ndabney562/world-simulator/issues/68), accepted [ADR 0027](../../adr/0027-slice-schedules-and-events.md). Branch `codex/68-daily-schedules`, based on `6046f606f43f822cab98e25eb2074a2c10ec655f`. No old Tier-1 golden, save fixture, ABI, command envelope or Unity asset changed. New independent schedule goldens freeze rules version 1.

## Local correctness

- Rust workspace release suite: **345 passed, 0 failed**, four existing ignored tests retained; no new ignores. The existing long LOD tests retain their nightly path.
- Python script suite: **60 passed**. Docs, dependency direction, Unity static checks and whitespace checks pass.
- Core-directory pinned Rust1.97.0 formatting/Clippy all-target checks pass.
- Independent timetable intervals cover every generated resident for a week plus the preceding day; exact +/-1us boundaries, week wrap, negative/opposite/extreme epochs and pure off-map identity conservation are checked.
- A real simulated week publishes all expected typed facts with physical-tick IDs/ordinals; queries/index reconstruction preserve history and do not feed state. Single-step and varied batching agree on state, facts and normalized operation digests.
- Structured scripts replay in another test process, comparing state/event/operation checkpoints and per-tick checkpoints across an actual activity boundary. Resource fault injection verifies failing-tick rollback, committed prefixes, stopped replay/digest verification and explicit retry; clock overflow and host cap/remainder/pause semantics are covered.
- Legacy encoders reject new payloads with typed errors; their readers retain unknown-tag rejection. A saved-file regression constructs otherwise valid external history containing a new slice payload and proves rejection leaves both target and staging sentinel untouched. Existing v1/v2 fixtures and continuation tests pass.

## Isolated desktop measurements

Apple M1 Pro, pinned Rust1.97.0 release, overflow checks retained. Run after all suites finished; Criterion10 samples, 1s warm-up, 2s requested measurement. `core/`: `cargo bench --locked -p ws-sim --bench slice -- --warm-up-time 1 --measurement-time 2 --sample-size 10`.

| Measurement | Seed0,18 people | Seed42,20 people | SeedMAX,24 people |
|---|---|---|---|
| 1,000 quiet explicit ticks, 95% interval | 7.3192–7.3474 µs | 7.3419–7.4421 µs | 7.3215–7.3567 µs |
| First event-bearing tick, 95% interval | 2.4015–2.4480 µs | 2.4829–2.5198 µs | 2.6889–2.7498 µs |
| Plan every boundary/fact in one day, 95% interval | 124.84–125.33 µs | 130.71–131.69 µs | 176.07–176.42 µs |

Current toy 5,000-resident reference in the same executable/build: **24.685–24.757ms per100 ticks**, estimate24.718ms, or247.18µs/tick. It starts after600 warm-up ticks. Both step benchmarks exclude fresh-clone setup and world destruction; step/operation/factual publication work is included. Planning includes temporary fact-buffer allocation/destruction but no store publication. Transition measurements cover the first household boundary for each seed, not the maximum coincident cohort or an established long history. State-hash/query work is separate diagnostic work and excluded. Quiet runs include1,000 explicit tick/log operations, divided by1,000 only for amortized per-tick descriptions.

These are distinct workloads: L2 timetable slice versus the moving L1 toy reference. Historical [TP-011](../../spikes/tp-011-iphone-performance.md) reports1.06ms mean core step at5,000 residents on the iPhone, and earlier host implementations report about4ms. Neither is a same-hardware before/after comparison. The slice measurements are far below those references on this Mac, but do not establish device, frame, L1, provider or full-scene performance.

## Remaining scope

Civil New York calendar/sun is #71; L1 needs/pathfinding #69; camera LOD #70; real rendering/inspection #72/#73; illness consequences #74; slice save formats #53/#75; final on-device evidence #76. The existing CLI/FFI/save path remains the toy simulation. This issue supplies no new device claim or stage-exit declaration.
