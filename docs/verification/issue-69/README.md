# VS-005 behavior/navigation verification

Scope: [#69](https://github.com/ndabney562/world-simulator/issues/69), accepted [ADR 0028](../../adr/0028-slice-l1-needs-and-navigation.md). Branch `codex/69-l1-behavior`, base `88ba7e282bf471849caaebcb4e3066bdbe4cf082`. No old golden, fixture, event/save schema, FFI/Unity API, data source or dependency is changed. New behavior/navigation goldens pin rules version1.

## Correctness evidence

Focused domain and slice tests cover independent path/need goldens, positive graph edges and complete-path ties, every resident's fine local/off-map commute deadlines over a week, exact zero-time/mixed-level conservation, full-week coarse/minute catch-up, a separate physical-tick need reference, typed detail/replay and injected domain/history rollback.

Local results on the implementation tree, using pinned Rust1.97.0:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo build --workspace --all-targets --locked`: passed.
- `cargo test --workspace --release --locked`:363 passed,0 failed;4 pre-existing ignored tests unchanged.
- `python3 -m unittest discover -s scripts`:61 passed. New independent exhaustive-path and counter-RNG oracle regenerates the new fixture exactly; existing fixtures are unchanged.
- `python3 scripts/check_docs.py`, `python3 scripts/check_core_deps.py`, `python3 scripts/check_unity_project.py`, and `git diff --check`: passed. Unity verification here is static, not an Editor run.
- `PATH=/tmp/worldsim58-tools/bin:$PATH scripts/check-ffi-header.sh` (from repository root): passed with pinned cbindgen0.29.2. Existing ABI-v3 C consumer compiled with warnings as errors and ran successfully against the debug library.
- `cargo +1.97.0 build --release --locked -p ws-ffi --target aarch64-apple-ios`: passed. This is a Rust archive build, not a signed Unity/device build.
- Existing CLI record/replay at seed42,10,000 ticks: verified state `e8c3cc679db799de`; save/load at2,000 ticks and resume to2,500 ticks also passed.

## Measurement method

Desktop-only Criterion, pinned Rust1.97.0, release with overflow checks. Run after correctness suites finish: `cargo bench --locked -p ws-sim --bench active -- --warm-up-time 1 --measurement-time 2 --sample-size 10` from `core/`. Seeds0/42/MAX give18/20/24 residents. The graph is the same10-node topology, with seed-dependent semantic entity mappings.

- Cold all-pairs paths:100 allocation-free Dijkstra queries per iteration, excluding graph construction; this measures actual search, not cached lookup.
- Active ticks:100 physical ticks, each followed by a fine snapshot of every resident. Includes operation hashing, retained-state checks, need evaluation and actual local/off-map projection. Starts07:30 in model time, with a mixed stationary/commuting cohort.
- Minute decision tick: starts100ms before07:31; includes domain planning/allocation and all residents' next utility decisions, plus any schedule facts at that boundary.
- Coarse hour: one standalone behavior-domain drain18:00–19:00,60 minute boundaries; no physical tick loop, factual store publication or host clock cost.

Fresh clone setup and output destruction are outside timed step/drain closures. Canonical state hashes, scene rendering, L1 crowd avoidance and device frame costs are excluded. Isolated local results (Criterion point estimates, microseconds per iteration):

| Case |18 residents, seed0 |20 residents, seed42 |24 residents, seedMAX |
|---|---:|---:|---:|
|100 ticks plus all fine views |174.73 |225.36 |271.36 |
|Minute decision tick |3.396 |2.629 |2.883 |
|Coarse hour domain drain |126.08 |157.11 |197.36 |
|100 cold path queries |9.981 |9.968 |9.987 |

The unchanged `slice_reference_toy_5000_100_steps` benchmark, run separately with the same sampling options, measured24.705ms per100 steps (95% interval24.674–24.737ms). It has5,000 residents and different work, so it is a reference cost, not an L1 speedup comparison. The active100-tick intervals were173.92–175.33,224.68–225.79 and270.33–272.52µs respectively. These small desktop cohorts do not establish iPhone frame budgets, larger-population scaling or stage-exit acceptance;#76 must supply device evidence.

## Remaining scope

The graph is an explicitly synthetic padded-footprint perimeter with portal proxies, not surveyed sidewalks/entrances, terrain or ENU. Camera-driven levels/events are#70; real civil calendar/sun#71; rendering/inspection#72/#73; illness consequences#74; slice persistence#53/#75; actual20-minute iPhone verification and stage exit#76. Existing CLI/FFI/save paths remain the toy simulation. No device or stage-exit claim is made.
