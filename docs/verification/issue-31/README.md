# Issue #31: movement frame reuse

## Change and exactness

The selected option from #31 is frame caching. `ToyWorld::plan_movement` creates a step-local ordered map from occupied cell ID to its exact `CellFrame`. All L1 agents in a cell share that frame for goal projection and `SurfacePos::reexpress_with`. Waiting agents do not populate the cache; an empty L1 store returns before creating it. Memory grows with distinct cells visited in the step, not with a new per-agent component, and disappears after planning. The cache adds map allocation/lookup overhead, measured in the step benchmarks.

`reexpress_with` converts the same f32 offsets through the exact same ENU frame operations as the original `reexpress`, then calls the unchanged Tier 1 `CellId::from_ecef` lookup and unchanged ADR 0016 `settle`. Private `CellFrame` fields guarantee that a frame cannot be constructed with a mismatched origin for its ID; a caller passing a different cell gets `FrameMismatch`. There is no approximate geometric inside predicate or skipped membership decision to justify. The new global fixed-seed property compares exact output bits against the original path; the existing fixed/global ±200 µm edge sweeps now additionally compare cached and original results and assert that an inside result agrees with the full lookup. Existing assertions and goldens remain intact.

The original `reexpress` remains an independent frame-building reference path. No coordinate representation, canonical iteration, dependency, unsafe code, threads, LOD policy or save/FFI layout changes.

## Reproduce the measurements

Baseline source: `de1a59eea49f2febfe65eec97213fa29ffab8526`. Same arm64 Mac, Rust 1.97.0, release profile, baseline then candidate sequentially. Criterion confidence intervals are reported below; absolute numbers are desktop evidence, not iPhone measurements. The existing LOD benchmark overrides sample sizes to 20 for transitions/steps and 10 for the hour. Each large case first warms a 100,000-agent world for 600 ticks outside the timed region, so the suite takes several minutes.

From `core/`, baseline:

```sh
cargo bench -p ws-sim --bench step -- world_step --save-baseline main31 --warm-up-time 1 --measurement-time 3 --sample-size 20
cargo bench -p ws-sim --bench lod -- --save-baseline main31 --warm-up-time 1 --measurement-time 2 --sample-size 10
```

After applying this branch in the same checkout (preserving Criterion data under `core/target/criterion`):

```sh
cargo bench -p ws-sim --bench step -- world_step --baseline main31 --warm-up-time 1 --measurement-time 3 --sample-size 20
cargo bench -p ws-sim --bench lod -- --baseline main31 --warm-up-time 1 --measurement-time 2 --sample-size 10
cargo bench -p ws-math --bench conversions
```

The conversion benchmark runs original and cached re-expression over identical inputs in the same executable (best of 50 passes of 10,000 positions). It includes the full lookup in both paths. It is a microbenchmark, not a simulation frame budget.

## Independent baseline/candidate replay probe

[replay-probe.rs](replay-probe.rs) and [l2-regression-bench.rs](l2-regression-bench.rs) are historical verification artifacts for baseline `de1a59eea49f2febfe65eec97213fa29ffab8526` and candidate `1f612862dd2447b30f3525f8b4bb15e5ad40dc10`. They are not compiled by CI or maintained as current-API tools. Compile it against each revision's release libraries, run the two resulting executables and compare their outputs. It prints canonical state hash plus a digest of every derived L1 velocity's f32 bits after each tick. Four seeds, 100 agents, 10,000 ticks, with all L1 and with alternating mixed LOD requests every 137 ticks, give 80,000 comparisons. Compare within the same platform/build settings; it is Tier 2 evidence, not a new cross-platform golden.

From `core/`, after building each revision separately:

```sh
cargo build --release --locked -p ws-sim
# Use the rlib from this build; use a separate target directory per revision
# to avoid selecting artifacts from other feature/build configurations.
rustc --edition=2024 -O ../docs/verification/issue-31/replay-probe.rs \
  -L dependency=target/release/deps \
  --extern ws_sim="$(find target/release/deps -name 'libws_sim-*.rlib' -print -quit)" \
  -o /tmp/worldsim31-replay
/tmp/worldsim31-replay > /tmp/worldsim31-replay-output.txt
```

The probe file is added by this branch; copy it outside the candidate checkout to compile against main. Preserve before/after executables and output under distinct names, then use `cmp` on the outputs. The unit/property/golden/replay/LOD suites remain the regression checks in CI; this probe additionally compares directly to the unchanged main implementation.

## Initial sequential benchmark results

95% timing intervals, with the point estimate in the middle. Criterion uses a separate change distribution, so its percentage can differ from the ratio of displayed estimates.

| Benchmark | Main interval | Candidate interval |
|---|---|---|
| `world_step/50` | 34.223 µs 35.443 µs 37.905 µs | 23.852 µs 23.904 µs 23.981 µs |
| `world_step/5000` | 3.3269 ms 3.3330 ms 3.3409 ms | 2.2826 ms 2.2909 ms 2.3001 ms |
| `lod_transition/demote/30` | 8.1310 µs 8.1759 µs 8.2552 µs | 8.0522 µs 8.0777 µs 8.1023 µs |
| `lod_transition/promote/30` | 8.5702 µs 8.9493 µs 9.7881 µs | 8.3234 µs 8.3406 µs 8.3556 µs |
| `lod_transition/demote/3000` | 878.07 µs 889.25 µs 900.49 µs | 864.79 µs 866.32 µs 868.07 µs |
| `lod_transition/promote/3000` | 904.31 µs 909.29 µs 914.12 µs | 867.43 µs 869.56 µs 872.32 µs |
| `lod_transition/demote/100000` | 32.125 ms 32.212 ms 32.302 ms | 31.698 ms 31.789 ms 31.890 ms |
| `lod_transition/promote/100000` | 32.487 ms 32.893 ms 33.451 ms | 31.120 ms 31.262 ms 31.421 ms |
| `lod_step/l1/3000` | 2.2698 ms 2.3078 ms 2.3753 ms | 1.3669 ms 1.4013 ms 1.4272 ms |
| `lod_step/l2/3000` | 611.17 ns 632.86 ns 649.96 ns | 559.24 ns 573.24 ns 589.60 ns |
| `lod_step/l1/100000` | 65.820 ms 66.132 ms 66.454 ms | 46.022 ms 48.351 ms 52.519 ms |
| `lod_step/l2/100000` | 14.167 µs 15.573 µs 16.522 µs | 13.988 µs 15.591 µs 17.888 µs |
| `lod_l2_hour/3000` | 17.675 ms 18.541 ms 20.802 ms | 18.007 ms 18.602 ms 19.794 ms |
| `lod_l2_hour/100000` | 665.98 ms 672.66 ms 679.38 ms | 689.37 ms 715.48 ms 752.39 ms |

The 5,000-agent L1 result meets the issue's approximate 2.4 ms target on the M1 Pro, but not on the independent Xeon reference below. All transition and L1 results improved or were within the noise threshold. Smaller L2 changes are not attributed to frame reuse: that path does not enter the cache. The initial 100,000-agent L2-hour run flagged +6.37% (change interval +2.29% to +12.04%, p=0.01), so it required a focused investigation rather than a blanket no-regression claim.

The [focused benchmark](l2-regression-bench.rs) repeats exactly the same seed, population, 600 L1 warm-up ticks, all-L2 transition and 36,000-tick measured span, with 30 samples and a 20-second target (Criterion extends it to fit the samples). It excludes unrelated benchmark setup. Compile it with `rustc --edition=2024 -O`, `-L dependency=target/release/deps`, and `--extern ws_sim=... --extern criterion=...` pointing to that revision's release rlibs (Criterion is the existing dev dependency). Run the main executable with `--bench --save-baseline main31-isolated`, then the candidate with `--bench --baseline main31-isolated` in the same directory. Preserve distinct executable names; rebuild main in a detached checkout, not by editing the candidate source.

An initial focused pair found no significant change (694.99 → 709.61 ms, change interval −2.82% to +7.88%, p=0.45), but overlapped the debug soak test and is excluded from the final isolated comparison. The isolated pair ran only after the workspace tests finished. No tests or performance budgets were changed to address the original flag.

The isolated 30-sample comparison, after all test work finished, measured main 681.78 ms (95% interval 680.14–683.48) and candidate 669.64 ms (668.22–671.01), a change interval of −2.10% to −1.47% (point −1.78%, p<0.05). The original slowdown was not reproduced; no L2 regression was observed in this controlled repeat. This is desktop timing evidence rather than proof of identical performance in every environment.

## Independent Linux review (2026-10-17)

Claude reported these measurements in its [PR #45 review](https://github.com/ndabney562/world-simulator/pull/45), using an Intel Xeon at 2.1 GHz, Rust 1.97.0, alternating baseline/candidate twice with 30 samples. These are reviewer measurements, not a new local benchmark run.

| Benchmark | Baseline runs | Candidate runs |
|---|---|---|
| `world_step/5000` | 3.93 / 3.83 ms | 2.65 / 2.62 ms |
| `world_step/50` | 38.7 / 38.4 µs | 28.2 / 27.3 µs |
| Re-expression microbenchmark | 559 ns | 434 ns |

The optimization improves both machines, but the Xeon result misses the approximate 2.4 ms target. At roughly 2.63 ms per tick and 3.33 ticks per frame, the average ×10 cost is about 8.8 ms, above the 8 ms high-priority budget before other work. Cached re-expression still accounts for roughly 434 ns of 525 ns per agent: the unchanged full cell lookup remains the main cost. Further membership optimization is future assigned work, not part of this change; desktop results do not establish the iPhone budget.

For independent baseline/candidate builds, use a distinct `CARGO_TARGET_DIR` per revision and point each probe's library paths at that directory. Preserve distinct executables and compare outputs on the same platform. The sequential Criterion measurements above retain their original methodology.

## Correctness verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --all-targets --locked`: pass.
- `cargo test --workspace --locked`: **216 passed, 0 failed, 2 existing ignored nightly LOD tests**. Includes unchanged Tier 1 math/world goldens, same-process/separate-process replay, LOD invariants/conservation and the debug short soak. The added global property and extended edge sweeps compare cached results to full lookup without changing any tolerance or existing assertion.
- The independent main/candidate probe outputs are byte-identical: 80,000 rows each; SHA-256 `7a0d6e46d5801bb0d73604713760a55c0320afa0044b9a47d901bb9062edce7b` for both.
- `python3 scripts/check_docs.py`, `python3 scripts/check_unity_project.py`, all 36 Python script tests, dependency direction and `git diff --check`: pass locally. Final PR CI must also run unchanged checks on Linux and macOS; local results above are macOS only.
- No Unity asset, binding or plugin changed; Unity Editor/device testing is not claimed for this core-only optimization.
