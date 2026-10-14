# 19. Testing Strategy

Covers point 19.

## 19.1 Principles

- The simulation is tested **headless in Linux CI**, independent of the renderer and of iOS. That is where almost all correctness risk lives, and where the AI collaborators can run tests themselves.
- Tests target **invariants and properties**, not just examples, because emergent systems fail in ways nobody writes example tests for.
- Determinism is a tested property, not a hope.
- Performance is a tested property with budgets, not a final-phase activity.

## 19.2 Test layers

| Layer | What | Tools | Where it runs |
|---|---|---|---|
| Unit | Functions and small modules: transforms, cell-ID math, time arithmetic, utility scoring, effect application | `cargo test` | Linux + macOS CI, every PR |
| Property-based | Invariants over random inputs: frame round-trips within tolerance; cell-ID neighbour and containment rules; generator locality (generating a cell alone equals generating it with neighbours); aggregation/disaggregation conservation | `proptest` | Every PR |
| Golden (snapshot) | Hashes of generator outputs for fixed seeds and addresses; hashes of world state after N ticks of fixed scenarios | Custom golden harness in `ws-testkit` | Every PR; any change requires an intentional version bump and golden update |
| Determinism | Run scenario twice in-process, in separate processes, and with different thread counts; compare per-tick state hashes. Cross-architecture (Linux x86-64 vs. macOS arm64) comparison on a schedule | `ws-cli` + CI matrix | Every PR (same-arch); nightly (cross-arch) |
| Scenario / integration | Scripted headless stories: "materialize district, run a day, dematerialize, run a week, re-materialize; check conservation and notable persistence"; "apply plague at L5, zoom into a district, check infected share matches aggregate" | Scenario DSL in `ws-testkit`, files in `tests/scenarios/` | Every PR (short), nightly (long) |
| Soak | Simulate years or decades of game time headless; check invariants, memory growth, save size, event-log growth, no NaNs, no runaway populations | `ws-cli` | Nightly |
| Statistical equivalence | Compare fine-level-then-aggregate against coarse-level evolution over many seeds; assert drift within tolerance ([05](05-simulation-lod.md)) | `ws-cli` + analysis scripts | Nightly / weekly |
| Save compatibility | Load every fixture save from `tests/saves/`, migrate, run, re-save | `cargo test` | Every PR touching persistence; nightly |
| Replay regression | Recorded sessions (journal + start snapshot) replayed; final state hash must match | `ws-cli` | Nightly |
| Benchmarks | Per-system CPU cost, materialization cost, generation cost with budgets; regressions beyond a threshold fail | `criterion` | Nightly with recorded baselines |
| iOS unit / bridge | FFI wrapper, snapshot decoding, lifecycle (background/foreground save) | XCTest on simulator | macOS CI, every PR touching `apps/ios` or `ws-ffi` |
| On-device performance | Fixed "benchmark flights" (scripted camera paths from street to orbit), sustained 20-minute runs, memory high-water marks, thermal state over time | XCTest performance metrics, Instruments | Per milestone and before each TestFlight build |
| Visual | Screenshot comparison of fixed camera positions with tolerance; mostly to catch broken shaders and LOD seams | XCTest screenshots | Later phases |

## 19.3 Debugging infrastructure that doubles as testing

- **Invariant checkers** run in debug builds after every LOD transition and periodically (counts, conservation, ID uniqueness, frame consistency).
- **State hashing** per tick, per node, to locate the first divergence when determinism breaks.
- **Replay from bug reports:** a TestFlight build can export the last snapshot plus journal, which reproduces the session in `ws-cli`.
- **Causal tags** on events (which intervention or event caused this) make emergent failures explainable.

## 19.4 CI pipeline (proposed)

1. Linux: format, lint (`clippy`), unit, property, golden, short determinism and scenario tests.
2. macOS (arm64): same core tests (catches architecture-specific float issues), build the XCFramework, build the iOS app, run XCTest on the simulator.
3. Nightly: soak, statistical equivalence, cross-architecture determinism, benchmarks, replay regression.
4. Manual/per-milestone: device performance runs on Noah's devices.
