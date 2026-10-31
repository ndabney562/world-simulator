# VS-012: Vertical Slice exit report

Status: **Vertical Slice exit criteria met or waived.** Issue: [#76](https://github.com/ndabney562/world-simulator/issues/76). Criteria: [20-mvp.md](../architecture/20-mvp.md) §21.3.

## Device run

One run of the RealBlock scene on Noah's iPhone 16 Pro on 2026-10-31, built with **WorldSim → Build iOS real block** and free personal-team signing. Log: [`vs008-real-71449-20261031-225411.csv`](../verification/issue-76/vs008-real-71449-20261031-225411.csv) (one row per second).

| Measure | Result |
|---|---|
| Duration | 1,112 s (18.5 min), no gaps in the log (largest step 1.02 s) |
| Scene | Seed 71449, 30 residents, 58 buildings, 2,245 vertices |
| Time scale | ×1 for 238 s, ×10 for 870 s |
| Frame rate | Mean 59.9 fps; 1st percentile of per-second fps 59.9; 3 slow seconds below 55 fps (lowest 38.7 fps) |
| Frame time | 99th percentile of per-second worst frame 19.5 ms; 6 seconds had a frame over 33 ms (worst 184 ms) |
| Core | Native apply 0.03 ms per tick on average (worst single tick 5.4 ms); snapshot fill 0.25 ms per frame |
| Memory | Process 239 MB at start, 255 MB at end, peak 323 MB; Unity 83–84 MB; managed 4–6 MB |
| Thermal | *Nominal* in every sample |

The first five seconds (startup, 6.6 fps in the first sample) are excluded from the frame figures.

**Not recorded:** iOS version, start battery and room temperature. The phone was **charging** for the whole run, so battery drain is unknown. Charging adds heat, so I infer the thermal result is if anything pessimistic; this was not measured.

**What the run included:** the `l1` column varies between 0 and 15, so the camera moved in and out and LOD transitions happened. Whether an intervention (bless, storm or gathering) was cast is not in the log; Noah was asked to cast them.

The three slow seconds (at 234 s, 272 s and 566 s) are single long frames of 167–184 ms with normal frames around them. Their cause is not in the log. They are not a frame-rate problem, but a hitch of that size is visible.

## Exit criteria

| # | Criterion | Verdict | Evidence |
|---|---|---|---|
| 1 | Zooming never loses or duplicates a person; re-materialized people match their schedules | **Met** | `a_thousand_actual_camera_operations_conserve_the_bulk_population_and_replay_full_history` (`core/crates/ws-sim/src/slice/camera_tests.rs`): 1,000 random camera moves, every resident present once and equal to an independent reference after each; `detail_round_trip_keeps_state_hash_need_progress_positions_and_history` (`active_tests.rs`) |
| 2 | Block aggregates equal the aggregation of individuals at all times (automated) | **Met** | `Controller::aggregate` (`core/crates/ws-life/src/lod.rs`, private; reached through the public `summary` and `view`) rebuilds every building row from the individuals on each summary and view, and fails with `LodError::Invariant` unless location, detail and governing counts each sum to the population and each building's activities sum to its occupancy. The camera test above exercises it 1,000 times; the device log's `local_stationary + in_transit + off_map` equals 30 in every row |
| 3 | Save, quit and load reproduce the same state hash | **Met** | `core/crates/ws-persist/tests/slice_roundtrip.rs`, including `slice_advanced_with_illness_and_lod_round_trips_and_steps_identically` and `format6_round_trips_partial_goal_and_faith` |
| 4 | Intervention consequences listed by a query, with confidence classes | **Met** | `SliceSimulation::consequences` (`core/crates/ws-sim/src/slice/illness.rs`); `core/crates/ws-sim/tests/slice_illness.rs` checks `CausedBy`/`Direct` and `ContributedTo`/`Contributing` links and bounded queries |
| 5 | Toggling the Google backdrop does not change the state hash | **Waived** | No backdrop exists: Google data is excluded ([ADR 0008](../adr/0008-visual-backdrop-providers.md), [ADR 0023](../adr/0023-vertical-slice-block.md)) and the provider spike TP-012 ([#13](https://github.com/ndabney562/world-simulator/issues/13)) is still open. Waived as at the Technical Prototype; it is re-checked when a backdrop is added |
| 6 | Stable frame rate on the iPhone 16 Pro for 20 minutes | **Waived (18.5 min)** | The run above held about 60 fps apart from 3 slow seconds (lowest 38.7 fps), with nominal thermal state, for 18.5 minutes. Noah (Product Owner) accepted it on 2026-10-31 instead of repeating the run |

## S-5 frame-rate decision

The TP-011 note deferred S-5 (30 fps with opportunistic 60, or 60 sustained) to this load. On the real block the game held **about 60 fps** apart from 3 single-frame hitches, with nominal thermal state, and the core plus fill use under 0.5 ms of a 16.7 ms frame. Recommendation: target **60 sustained** for the slice. No ADR is proposed: 30 residents is far below the Neighbourhood MVP's population, so the policy should be fixed when that load exists, not on this one.

## Not verified here

Unity cannot run in the cloud environment; the device numbers are read from Noah's CSV and I did not observe the phone. The run is 1.5 minutes short of the criterion, mostly at ×10 rather than ×1, and started while charging.
