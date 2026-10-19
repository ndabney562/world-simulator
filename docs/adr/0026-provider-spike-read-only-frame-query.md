# ADR 0026: provider spike read-only frame query and isolated Unity providers

- Status: Accepted
- Date: 2026-10-19
- Decided by: Noah, direct standing approval on 2026-10-19, recorded on #77 and PR #84; proposed by Codex
- Issue: #13 (TP-012 design phase)
- Supersedes: none; would extend ADRs 0018/0022 after acceptance

## Context

[TP-012 #13](https://github.com/ndabney562/world-simulator/issues/13) is assigned by Noah's full-backlog authorization. It needs geospatial alignment and an automated test that provider switches leave the core hash unchanged. ABI 3 exposes relative render coordinates but no absolute origin/basis or canonical state hash. A picture or hash of render bytes cannot prove simulation independence. Extending the FFI requires an accepted ADR first.

The skeleton currently forbids C# in `Providers/` through `ProjectSkeletonTests.ProvidersFolderContainsNoCode` and the static checker's `NO_CODE_FOLDERS`. It also forbids scoped package registries. These are deliberate pre-TP-012 checks; adding providers/Cesium would fail them. The replacement expectations below are **proposed for explicit Product Owner approval**, not already waived or implemented. Current tests and accepted ABI 3 remain unchanged by this docs-only PR.

## Options considered

1. Infer alignment/hash from Unity render matrices. Rejected: the seed-dependent snapshot origin and cell ENU basis are not present; render bytes omit canonical driving state.
2. Give provider objects a native world handle. Rejected: this creates an unnecessary simulation mutation path and coupling.
3. Add one coarse read-only frame/hash query, with a provider assembly receiving immutable presentation metadata only. Proposed. Keeps all simulation/provider separation rules and does not add per-entity calls.

## Proposed decision

### Read-only bridge contract

Reserve the next unused bridge ABI for this implementation: **4**, if still unused when implementation starts. If another accepted implementation consumes it first, reconcile the number in this ADR before implementation; do not create two incompatible meanings. VS-008's future render ABI must use the next unused number and its own ADR, rather than assume 4 remains free.

Bump native version preflight, config/control/snapshot ABI fields, generated header, C# binding/decoder and fixtures together. Config, control state, instance record and render-header field shapes remain ADR 0022's shapes; only the ABI field changes. Keep previous fixtures for explicit old-client refusal. Command envelope version 2/opcodes, saves and canonical event schemas remain unchanged. A stale ABI-3 native library is rejected before a new caller reaches a missing symbol.

Add `WsStatus ws_get_render_context(WsHandle*, WsRenderContext*)`, serialized under the same single-handle ownership rule as other calls. Caller initializes ABI version and exact size. On success write this 128-byte, 8-byte-aligned fixed-width C struct; on invalid input/version/size or poisoned handle write no output. Preserve existing status/diagnostic/panic rules and pointer safety obligations. All floating values must be finite.

| Offset | Type | Meaning |
|---:|---|---|
| 0 | u32 | ABI version |
| 4 | u32 | struct size = 128 |
| 8 | u64 | committed tick |
| 16 | u64 | current canonical state hash |
| 24 | u64 | snapshot origin cell ID |
| 32 | 3 × f64 | absolute WGS84 ECEF origin, metres |
| 56 | 3 × f64 | unit east basis in ECEF |
| 80 | 3 × f64 | unit north basis in ECEF |
| 104 | 3 × f64 | unit up basis in ECEF |

The origin is exactly the ground centre of the lowest-ID building and the basis is that building cell's ENU basis, matching ADR 0022's current render implementation. Compute it through existing `World::toy().buildings()`/position/cell-frame accessors in `ws-ffi`, whose only simulation dependency remains `ws-sim`. No new domain state or simulation-crate dependency. The current toy constructor always has buildings; an unavailable context fails explicitly rather than manufacturing an origin. If a later model permits empty worlds it needs an explicit context contract then.

The query never flushes pending commands, steps time, changes canonical state/history, or reads provider state. It is a coarse diagnostic/context query at creation/when frame identity changes and in tests, not an extra per-entity or mandatory per-frame crossing. It is coherent with itself; callers needing it paired with a render snapshot must hold the existing host gate across both calls and verify equal ticks. A hash includes time, so comparing snapshots at different ticks does not prove provider influence. Tests freeze simulation at a committed boundary without submitting commands between hash reads, and separately compare twin worlds stepped identically while switching providers.

### Provider isolation and coordinate mapping

All runtime provider code lives in `Assets/WorldSim/Providers/`, in a `WorldSim.Providers` assembly that references Cesium/Unity math as needed and **no `WorldSim.Bridge` or other WorldSim assembly**. It receives an immutable presentation context/camera/exclusion description; no world handle, command queue, state mutator or arbitrary callback to core state. The host/test owns query access. Providers expose identity/version/kind, coverage/scale range, availability, attribution, exclusion capability, lifecycle and quota status under architecture §23.3. Manager selection/fallback is deterministic given presentation inputs but never affects simulation. Provider geometry has no simulation picking collider or entity identity authority.

For a snapshot `(east, north, up)`, form absolute ECEF in double precision as `origin + east*east_basis + north*north_basis + up*up_basis`; use Cesium's ECEF-to-Unity transform. Do not treat snapshot ENU as Cesium's local axes or anchor it at a hard-coded latitude. Changing Cesium georeference/floating origin recomputes presentation transforms only. Tests compare multiple seed-dependent origins, adjacent-cell positions and basis orientation against core-side known values, including the first building at local zero.

Use a separate ProviderSpike scene/entry point. Preserve Bootstrap's existing minimal roots and existing rendering/input tests; no blanket loosening of scene assertions is necessary. A fake provider exercises loss of availability, exclusions, zero/tiny quotas and fallback. Open-data provider is the default, offline. Google is off and makes no requests without a separately approved account/key/quota path; documenting that optional path is allowed without claiming live Google/quota-network verification.

### Package, data and explicit validation changes

Propose pinning `com.cesium.unity` **1.26.0**, with only the exact official scoped registry `https://unity.pkg.cesium.com`, scope `com.cesium.unity`, following the [official installation instructions](https://cesium.com/learn/unity/unity-quickstart/). [The pinned release](https://github.com/CesiumGS/cesium-unity/releases/tag/v1.26.0) advertises iOS support. Verify package/transitive licences and editor/iOS compatibility during implementation; pin the resolved lockfile. The package is not yet installed by this docs PR. Cesium ion accounts, streamed sample terrain/Bing/OSM assets, tokens and Google credentials are not included or approved by this choice.

Use the already registered VS-001 NYC block-6049 extract for a minimal bundled display mesh; no additional data source is approved here. Register the derived asset's source/version/attribution in the existing data register. Full footprints/holes and height metadata remain source truth; any simplified/flat visual elevation must be labelled as a presentation approximation, not a conversion of NAVD88 into ellipsoidal height. Providers never feed this data back into core state. Coverage is restricted to the bundled extract and the manager falls back outside it; this does not claim a multi-block or terrain dataset.

Explicit approval requested before implementation for these **specific validation replacements**:

- Replace the obsolete `ProvidersFolderContainsNoCode`/static `NO_CODE_FOLDERS` ban on the provider folder with positive assembly-isolation and location checks, plus provider-switch native-state-hash, fallback, attribution, quota and coordinate tests. Providers elsewhere, references to Bridge, mutable core inputs, or missing metadata still fail. Retain every unrelated skeleton/scene/graphics check.
- Replace the blanket no-scoped-registry rule with an exact allowlist for the single official Cesium endpoint/scope and pinned package. Keep rejection of arbitrary registries, Git/file package sources, malformed/unpinned versions and unexpected dependencies, and add negative fixtures for each. Do not disable the checker or skip failed Editor tests.
- Update only ABI-specific expected versions/new shared fixtures as the accepted bridge migration requires; keep old fixtures and assert refusal. Existing Tier 1 ID/position goldens do not change.

This proposal is the concrete justification required by the test/CI and architecture approval gates. It does not itself grant acceptance. If Noah declines these replacements, leave current checks/ABI intact and #13 unimplemented; do not bypass them to get green.

## Required evidence after acceptance

- Native/C ABI layout, null/size/version/poison/error/unchanged-output tests, header freshness, read-only pending-command behavior, native state-hash equality and current tick/frame mapping.
- Shared ABI-4 fixtures decoded by Rust/C#, old-client refusal, canonical state/digest unaffected by queries, no new per-frame allocations or per-entity crossings.
- Editor/play-mode provider switching at a frozen boundary plus twin stepped worlds; fallback/exclusion/attribution/quota tests; actual default-view screenshot with real coordinates, no credentials and clear synthetic-toy/approximate-height labels.
- Static positive/negative registry and assembly-boundary tests, all existing unchanged checks/tests, package licence review and native/iOS archive builds.
- Product Owner device run using the existing signing/build pipeline, backdrop on/off memory and GPU measurements, recorded tested SHA and `docs/spikes/tp-012-geospatial-providers.md`. Do not call an Editor/static/iOS compile a device result. Google measurements remain absent unless separately authorized/enabled; its integration path is documented instead.

## Consequences

The spike gains an honest core-hash independence test and a precise seed-dependent geospatial transform without transferring simulation authority to a provider. It consumes a bridge ABI version, adds a pinned presentation dependency and replaces two temporary prohibitions with positive architectural checks. Hashing is a diagnostic/context operation outside the per-frame path; device overhead and Cesium memory cost are measured before any budget claim.

Noah accepted this docs-only contract and its named validation replacements under his 2026-10-19 standing approval, recorded on #77 and PR #84. No test/check/package/native layout/runtime changes ship in this ADR PR; separate implementation and evidence remain required. #13 remains open for its separate implementation and device report. No new credentials, paid service, Apple signing, device run or stage-exit declaration is authorized here. Revisit when real slice snapshots/floating origins land, package/platform support changes or measured backdrop cost exceeds the device budget.
