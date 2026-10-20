# VS-006 camera, aggregates and conservation verification

Scope [#70](https://github.com/ndabney562/world-simulator/issues/70), accepted [ADR0029](../../adr/0029-slice-camera-interest-and-aggregates.md), branch `codex/70-camera-lod`, base `170e81e12afaec9a3b97d6b6cecbe3f6b61972d9`. No new crate/dependency, old fixture/test weakening, FFI/Unity API or save format change. Slice-only event tag9 does not extend legacy save vocabularies.

## Correctness

The typed camera operation stores one integer input; deterministic200-ms node policy uses distinct entry/exit thresholds,1s dwell and explicit overview/warp coarsening. Every physical tick reconciles sole L1/L2 membership and validates complete identities/count partitions. L3 building occupancy/activity rows derive from the same committed individual answers; home/governing ownership is explicitly separate. Common logical-answer caches avoid recomputing paths for count checks and are excluded from hashing.

Evidence includes1000 randomized zoom cycles per property case across arbitrary seeds/negative epochs, a maximum30-person seed71449 through1000 further cycles, and all6,048,000 ticks of a week compared with an independently written resident aggregation. Need/action/counter/schedule/position comparisons use continuously retained fine reference state. Tests cover integer thresholds/extreme epochs and coordinates, actual off-map gateway demotion variants, inclusive120s observation hits/expiry/cache dropping, typed rejections/idempotence, per-boundary separate-process replay, host/physical resource-stop prefixes, complete transactional rollback and producer/index ordering.

New event tag9 freezes explicit words and digest `e22cd2ff279b8e06`, independently calculated by the existing Python SipHash oracle over the committed24-word test vector. Toy tag2 and old fixtures retain their contracts. Legacy codec and failed-save sentinel tests reject the new slice fact before any destination/staging write.

Local final-tree checks with pinned Rust1.97.0:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --all-targets --locked`: passed.
- `cargo test --workspace --release --locked`:383 passed,0 failed,4 existing ignored unchanged. Includes1000 actual slice camera operations with full-history replay in addition to the domain properties and backward-quiet-boundary rejection regression.
- `python3 -m unittest discover -s scripts`:61 passed; docs/dependency/Unity static checks and `git diff --check` passed. No existing golden/fixture was changed.
- Pinned cbindgen0.29.2 header comparison and existing native ABI-v3 C consumer (`-std=c11 -Wall -Wextra -Werror`): passed.
- `cargo +1.97.0 build --release --locked -p ws-ffi --target aarch64-apple-ios`: passed. This is a Rust archive, not a signed Unity/device build.
- Existing CLI seed42 record/replay at10,000 ticks verified unchanged toy state `e8c3cc679db799de`; save/load at2,000 ticks and resume to2,500 also passed.

## Desktop measurement method

Pinned Rust1.97.0 release with overflow checks, Criterion1s warm-up/2s measurement/10 samples, after correctness suites finish. Seeds0/42/MAX/71449 produce18/20/24/30 residents. The maximum cohort seed was found by an offline read-only population probe; it changes no generator or old golden.

`cargo bench --locked -p ws-sim --bench camera -- --warm-up-time 1 --measurement-time 2 --sample-size 10` measures the complete four-node promote tick at1.0s and forced overview demote tick at1.2s, including typed operation bookkeeping, common/controller/invariant work, fine projection validation, observation planning and factual publication. Fresh input clones/output-world destruction are outside the timed batched tick closures. Bulk fine-view timing includes output construction, geometry and result disposal. No scene rendering, native transport, device frame time or long-history scaling is included.

Isolated Criterion point estimates in microseconds per iteration:

| Workload |18, seed0 |20, seed42 |24, seedMAX |30, seed71449 |
|---|---:|---:|---:|---:|
|All-node promote tick |5.756 |5.946 |6.775 |7.786 |
|All-node demote tick with observations |6.013 |6.346 |7.015 |7.866 |
|Bulk fine view |1.892 |1.956 |2.453 |3.190 |

At the maximum30-person cohort,95% intervals were7.591–7.983µs for promotion,7.758–7.951µs for demotion and3.178–3.200µs for a bulk view. The measured complete core transitions are below the1ms issue target on this desktop. They do not establish the final Unity/native frame budget or iPhone performance;#72/#76 must add that evidence. Measurements ran after correctness work, with no concurrent local builds/tests/probes.

## Remaining scope

The coarse Rust command/view contract is complete core scope here; actual Unity camera/native snapshot integration is#72 and must preserve the explicit navigation-plane conversion. Calendar/sun#71, inspection#73, illness#74, whole-slice persistence#53/#75 and real20-minute iPhone evidence/stage exit#76 remain separate. No signed app, device frame result or stage-exit declaration is claimed.
