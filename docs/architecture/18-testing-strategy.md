# 19. Testing Strategy

Covers point 19.

## 19.1 Principles

- The simulation is tested **headless in Linux CI**, independent of Unity and iOS. That is where most correctness risk lives, and where the AI collaborators can run tests themselves.
- Tests target **invariants and properties**, not only examples.
- Reproducibility is tested **by tier** ([25](25-determinism-policy.md)): exact where exactness is promised, with tolerances where it is not.
- The event and causality system is tested like any other simulation output.
- Performance is tested against budgets throughout, not at the end.

## 19.2 Test layers

| Layer | What | Tools | Where it runs |
|---|---|---|---|
| Unit | Transforms (ECEF, ENU, geoid conversion), cell-ID math, time and calendar arithmetic, utility scoring, effect application, event construction | `cargo test` | Linux and macOS CI, every PR |
| Property-based | Frame round-trips within tolerance; cell containment and neighbour rules; generator locality; aggregation/disaggregation conservation; causal links always point backwards in time | `proptest` | Every PR |
| **Tier 1 golden** | Hashes of identity and structural outputs (IDs, households, names, gap-filled attributes) for fixed seeds, addresses, and dataset versions; **identical on Linux and macOS** | `ws-testkit` | Every PR; changes require a deliberate version bump |
| **Tier 2 replay** | Run a scenario twice, in separate processes and with different thread counts; per-tick state hashes must match on the same build and platform | `ws-cli` + `ws-testkit` | Every PR (Linux and macOS separately) |
| **Tier 3 drift** | The same scenario on Linux x86-64 and macOS arm64; report divergence metrics and check invariants and statistical tolerances, not hashes | `ws-cli` | Nightly; drift is reported, invariant failures fail the build |
| Scenario / integration | Scripted headless stories, for example: materialize a Bay Ridge block, run a day, dematerialize, run a week, re-materialize, then check conservation, notables, and events. Or: apply the plague at L5, zoom in, and check that the infected share matches the aggregate and links to the intervention | Scenario DSL; `tests/scenarios/` | Every PR (short), nightly (long) |
| **Event and causality** | Expected events and links for each intervention ("curse with illness" yields a `life.illness` event caused by the intervention, plus at least one `contributed_to` consequence); event invariants ([24 §24.10](24-world-events-and-causality.md)); aggregate event counts equal compacted individual events; retention never drops intervention chains; query answers ("what happened here?") are stable for fixed scenarios | `ws-testkit` | Every PR |
| Data pipeline | Pipeline output for a fixed source snapshot matches manifest hashes; schema validation; heights converted correctly (known benchmark points); every source has a licence and attribution entry | `ws-datapipe` tests | Every PR touching the pipeline; nightly |
| Soak | Simulate years of game time headless; check invariants, memory growth, save size, event store growth, no NaNs, no runaway populations | `ws-cli` | Nightly |
| Statistical equivalence | Fine-then-aggregate versus coarse evolution over many seeds, with drift within tolerance ([05](05-simulation-lod.md)) | `ws-cli` + analysis scripts | Nightly or weekly |
| Save compatibility | Load every fixture save, migrate, run, re-save | `cargo test` | PRs touching persistence; nightly |
| Benchmarks | Per-system CPU cost, materialization cost, generation cost, event query cost, all against budgets | `criterion` | Nightly with recorded baselines |
| Unity edit-mode / play-mode | Bridge bindings, snapshot decoding, provider manager (fallback, quota caps, attribution), and **"switching providers leaves the core state hash unchanged"** | Unity Test Framework | On a machine with Unity (macOS CI or Noah's Mac), PRs touching `apps/unity` or `ws-ffi` |
| On-device performance | Benchmark flights, sustained 20-minute runs, memory high-water marks, thermal state over time, with the Google backdrop on and off | Unity Profiler, Instruments | Per stage, and before each TestFlight build |
| Visual | Screenshot comparison at fixed camera positions with tolerance (broken shaders, LOD seams, alignment between backdrop and simulation geometry) | Unity screenshot tests | From the Neighbourhood MVP on |

## 19.3 Debugging infrastructure that doubles as testing

- **Invariant checkers** run in debug builds after every LOD transition and periodically.
- **State hashing** per tick, subsystem, and region locates the first divergence.
- **Replay from bug reports:** a TestFlight build can export the last snapshot plus journal, replayed on an arm64 Mac with the same build (Tier 2).
- **Causal browser** in `ws-inspector`: navigate the event graph from any event or intervention.

## 19.4 CI pipeline

1. Linux: format, lint (`clippy`), unit, property, Tier 1 golden, Tier 2 replay, event and causality, short scenario tests.
2. macOS (arm64): the same core tests, Tier 1 golden checked against the same files as Linux, the iOS static library build.
3. Unity: an iOS build and the Unity tests (on a macOS runner if Unity licence activation in CI works on the free plan; otherwise run locally on Noah's Mac before merging front-end changes).
4. Nightly: soak, statistical equivalence, Tier 3 drift report, benchmarks, data pipeline checks.
5. Per stage: device performance runs on Noah's iPhone.
