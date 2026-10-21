# VS-010 desktop verification

Issue #74; accepted ADR0034 merged separately in #102. Runtime branch `codex/74-illness`, base `0b7a19a062a2420eca50ffb6ed42d5653ec6fd9c`.

## What works

The production inspector action applies severity5/two model hours: health100→55→100, while life stays Alive. Existing travel finishes, the resident returns on the pinned routes, rests, and rejoins continuously. Actual missed duty, eligible L1 same-building neighbour check-in and recovery retain explicit parent links and confidence classes. Duplicate illness is a recorded no-op; invalid semantic commands are replayable rejections. The default person panel remains five fields.

The actual visible RealBlock action/recovery run captures [illness](illness.png) and [recovery](recovery.png), with corresponding sampled text. The normal two-hour action runs at100× for72wall seconds; the screenshots are actual Game-view output, not mockups. At100× the core correctly coarsens toL2 and does not invent an L1 neighbour visit. The separate actual native/managed L1 test proves the Socialize override and contributing link. Exact deadline/replay/rollback cases are core/native tests, independent of screenshot frame timing.

## Validation

- Full pinned Rust workspace:454 passed,0 failed,4 pre-existing nightly ignores unchanged (`rust.log.gz`). Later focused checks cover all11 illness integration cases,2 rollback/arbitration units,1 checked-health test and expanded legacy payload refusal. The latter diagnostic/error-classification/test additions are checked by final all-target clippy/build and targeted tests; the hosted final-head workspace run remains required before merge.
- Unity6000.3.26f1:68 distinct cases covered by passing results across the retained full and focused XML runs. The unmodified initial full XML records63 pass/5 fail: four synthetic test headers still used ABI7 and one new L1 test's1m interest radius excluded the entrance when centred on the footprint. Corrected only those test setups; all five pass in `unity-setup-fixes.xml`. Final decoder cases and the final visible action/recovery run exercise the current release plugin. `unity-combined-results.json` maps each latest result to its original XML; no failed XML is relabelled or suppressed.
- All61 Python tests, docs, static Unity, dependency direction, fmt, all-target clippy/build, generated header, real ABI8 C consumer, CLI record/replay/save/load and release iOS Rust archive passed. The archive exports `_ws_slice_consequences`; no Apple account/signing or device action is involved.
- Old ABI1–7 fixtures/consumer sources, healthy/toy hashes/goldens, all previous assertions/budgets/nightly checks remain. Current consumers migrate only version/schema-specific inputs and extend old-client refusal through7.

Source/data/test hashes are in `tested-files.sha256`; native plugin hashes distinguish the full initial and final focused/visible runs. The complete source verification is the union of the full suite and relevant focused reruns, not a claim that the initial full XML was green. Error/resource-path cleanup and the memory diagnostic do not change the normal rendered trajectory.

## Desktop cost

Criterion `cargo bench --locked -p ws-sim --bench slice -- slice_illness --quick`, release arm64 macOS, clone setup excluded by `iter_batched`; retained output is `bench.log.gz`. Quick-run intervals are diagnostic desktop measurements, not a device budget proof:

| Body | Interval |
| --- | --- |
| Healthy100 quiet ticks |79.52–79.85µs |
| Active illness100 quiet ticks |79.06–79.94µs |
| Complete illness command |3.84–3.98µs |
| Newest three-record group with1000 retained roots |58.74–59.25ns |

No measurable quiet-tick slowdown in this probe. Query work is a root binary search plus at most16 direct ID lookups. At1000roots/3001events, allocated consequence-index storage is188416bytes and event/index storage3245808bytes. Both grow with retained history: the bounded per-query output is not bounded total memory. No unbounded history clone/scan runs each tick; reconstruction explicitly scans history when requested. Screenshots show desktop frame/fill/native counters, not an iPhone performance claim.

## Limits

No contagion, deaths, balancing, other interventions or real-slice saves. Same-building check-ins model a five-minute social hold, not unit geometry or inter-building travel. A later independent intervention can interrupt that intended visit. Device acceptance for #72/#73 and the #76 stage run remain separate; no stage completion is claimed.
