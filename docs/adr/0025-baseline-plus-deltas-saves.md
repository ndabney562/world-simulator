# ADR 0025: version-pinned baseline plus canonical deltas in save format 3

- Status: Accepted; toy-world parts superseded by [0040](0040-delete-toy-world-and-old-formats.md)
- Date: 2026-10-19
- Decided by: Noah, direct standing approval on 2026-10-19, recorded on #77 and PR #83; proposed by Codex
- Issue: #53
- Supersedes: none; would extend ADR 0020 after acceptance

## Context

[Issue #53](https://github.com/ndabney562/world-simulator/issues/53) carries forward the persistence architecture's baseline-plus-deltas contract from TP-009. [ADR 0020](0020-save-format-and-versioning.md) and [ADR 0021](0021-god-intervention-contract.md) currently store full toy structural state in formats 1 and 2. VS-002 introduces the real block's fixed structural baseline; [VS-011 #75](https://github.com/ndabney562/world-simulator/issues/75) later integrates all slice state, background writing and Unity controls. This proposal defines the earlier storage contract without implementing #53 or prematurely claiming #75 is complete.

Loading must restore the same canonical state, event/journal digests, counters and future behavior. It cannot reproduce Tier 2 movement by replaying from a seed or assuming cross-platform replay. Geometry from an updated source extract must never silently replace the version used by a saved world. Baseline omission saves space only for values reconstructible exactly from pinned Tier 1 inputs.

## Options considered

1. **Keep storing every baseline row.** Simple and appropriate for legacy fixtures, but duplicates immutable data for every save and does not scale to large worlds.
2. **Store seed and replay commands.** Compact but cannot restore Tier 2 continuous state across builds/platforms and loses non-journaled driving state. Rejected by architecture §15.5.
3. **Pinned baseline plus typed component overrides, tombstones and complete notable/spawned records.** Proposed. More validation and versioned generator retention, but unchanged structure is reconstructible and canonical runtime state remains explicit.
4. **Generic field patches/compression.** Additional codec complexity without a measured need. Deferred; #53 excludes compression.

## Proposed decision

### 1. Baseline identity

Keep SQLite, signed-order-flipped integer keys, per-record schemas and ADR 0020's explicit little-endian codec. The first implementation reserves `application_id = WSAV` and format version **3**. Format-1/2 readers and committed fixtures remain supported. No runtime writer/version changes until this proposal is accepted and separately implemented.

Add one versioned `baseline` record to the header contract: world seed; dataset ID/version and full content SHA-256; generator IDs/versions for every baseline domain; identity-policy ID/version; baseline schema; and canonical manifest digest/count. A truncated dataset version alone is not sufficient to select source bytes. Pin all configuration that changes baseline generation (population configuration, epoch and generation parameters). Domain counts and a canonical structural digest detect missing output even if materialized cells are a subset.

The baseline is the immutable **initial structural state**, not a world stepped to save time. It comprises reviewed building source keys/geometry/metadata and Tier 1 procedural residents, households and workplaces as those domain issues land. Each domain specifies exactly which components it owns, their stable keys, and canonical ordering. Current time, LOD, position/leg counters, needs, intervention state and scheduled commands are not implicitly reconstructed by replay.

The initial identity policy is the existing ADR 0016 fail-on-collision policy. If #30's collision policy is later accepted and implemented, the baseline records that distinct policy version, its roster identity and canonical losing-address override table. Until then, format 3 rejects nonempty collision overrides or an unsupported policy; this proposal does not make the separate collision proposal active.

### 2. Component classification and tables

| Record | Stored content | Load behavior |
|---|---|---|
| `baseline` | Exact identity/configuration above; supported collision overrides if any | Resolve and regenerate trusted pinned Tier 1 data before merging |
| `structural_overrides` | `(cell, entity, component_tag)`, schema and complete replacement value | Replace an existing baseline component only; do not infer unspecified fields |
| `tombstones` | Typed baseline entity address/ID and explicit deletion state | Suppress regenerated live identity, retain historical reservation/reference validity |
| `notables` | ID, typed generation address and all canonical components | Promote matching baseline identity and replace its canonical components without duplicating it |
| `spawned` | Complete canonical records in the spawned namespace | Insert as non-baseline identities; preserve the checked next-spawned-ID counter |
| Runtime state tables | Current L1/L2 state, LOD/node aggregates, needs, schedules/counters, illnesses and provenance, as implemented | Restore directly; exactly one authoritative state representation per entity |
| Existing clock/journal/events/links/scheduled tables | All currently required canonical content and counters | Restore directly using versioned codecs and existing invariant checks |

No generic overwrite-by-row-order: duplicate/overlapping owners of an entity/component are corrupt. A notable supersedes baseline/structural override ownership through an explicit validated promotion rule, never SQL order. Reject simultaneous notable and ordinary component override rows for the same component. A tombstoned baseline entity cannot also have live notable, structural override or runtime rows. Spawned identities cannot masquerade as procedural baseline rows. This schema defines tombstones for future domain deletion; it does not add deletion gameplay in #53.

A component may be omitted only when byte-exact regeneration from the pinned immutable baseline gives the same canonical value at load. Compare fixed-point/integer fields exactly and any persisted floats by their defined raw-bit representation. Write complete typed replacement components rather than a field-level diff. Unchanged baseline structure and reconstructible identity rows are omitted; mutable state that differs from the initial baseline remains explicit, even for an anonymous resident. Do not call a necessary driving state an observation cache to omit it. Droppable presentation/observation caches have no authority and cannot affect the state hash or future decisions; their existing contract must prove that before omission.

Derived indices (actor/time query indices, L2 queues, navigation, lookup caches) and provider data are not saved. Runtime state is not compressed or synthesized from event text. Event history and all links remain intact; #53 introduces no compaction, retention or loss of intervened entities. New slice component/event tags belong to their implementing issues; no guessed schedules or ABI vocabulary is frozen here.

### 3. Atomic load and validation

1. Validate container/version/header schemas and bounded counts/lengths before allocating. Select exactly the baseline generator/dataset/policy recorded by the file; no network, latest-version substitution or wall clock in generation. The host supplies a trusted local baseline registry.
2. Resolve the full dataset digest and supported versions. If unavailable, return a typed `MissingBaseline` or `UnsupportedBaseline` before a world is published, identifying the missing public version/digest without leaking local paths or credentials. A save is not portable without its pinned baseline being available.
3. Regenerate the structural baseline using Tier 1 logic; validate its manifest/count/digest, duplicate/colliding keys and any supported canonical collision overrides before merging.
4. Apply tombstones and canonical replacements in sorted entity/component order, create spawned/notable records, and restore complete runtime driving state. Resolve homes, households, schedules, provenance and events against live identities or the domain's explicit historical/deleted-identity contract. A tombstone is not permission to accept arbitrary dangling event actors.
5. Rebuild all derived indices from authoritative records. Check every existing invariant, L1/L2 exclusivity, conservation, journal/event/clock counters and all newly implemented slice invariants. Restore the exact next spawned/event/command IDs, including allocated historical IDs.
6. Compare the reconstructed canonical state hash, journal digest and event digest to saved values. Also verify baseline identity and collision policy metadata. Return a complete world only if all checks pass; failure exposes no partial world and does not replace the current running world.

The runtime `state_hash` continues to hash the same fully resolved canonical world for the same model; changing whether a value is stored as a baseline omission or an override must not change it. Baseline identity and supported collision overrides receive a separately defined/versioned canonical digest in the format. Tier 2 digests remain corruption/continuation checks within their supported build/platform contract; schema compatibility and direct canonical restoration remain the basis of load compatibility. Do not claim that a digest makes arbitrary generator or rules changes compatible.

### 4. Writing and migration

At an eligible tick boundary with no pending commands, capture one coherent canonical snapshot as the current writer does. Classify baseline values by exact comparisons, serialize replacements/notables/runtime state and validate counters before atomically replacing the save through the existing `.saving` transaction/rename path. No incremental saving, compression, background-thread API or autosave policy is introduced by #53; #75 owns background writing and Unity save/load integration.

Formats 1 and 2 load through their existing full-state decoders with all existing fixture assertions. Reading does not rewrite the file. An explicit migration to format 3 may omit a legacy baseline row only after the exact legacy dataset/generator/configuration is available and its reconstructed value matches that row. Do not infer a baseline from current main or from approximate geometry. If the needed legacy baseline cannot be resolved, keep the old file/load path supported and refuse conversion explicitly; never produce a lossy format-3 save. Migrations are output-to-new-file operations, so failure preserves the old save. Old save and journal/event state must survive migration bit-exactly where the existing format defines it.

Updates do not replace a saved dataset or generator automatically. Keep compatible old generators/data available, or introduce a separately reviewed materialize-and-store migration with reference-remapping policy. Future slice components added by #75 require versioned schemas and a reviewed format amendment/version bump if they change this contract. One version number never denotes two incompatible table layouts.

## Required implementation evidence after acceptance

- New committed format-3 fixtures and exact table/byte/tag/limit documentation; old format-1/2 fixtures still load with unchanged expected values.
- Round trips and interrupted/continued runs at L1/L2/mixed, after intervention and after dematerialize/regenerate; all resolved canonical state, counters, journal/events/links and future checkpoints match an uninterrupted world.
- An unchanged baseline writes no structural override rows. Changing one building/component writes precisely its required complete replacement; promotion, tombstone and spawned records preserve unique ownership and stable references.
- A save at an arbitrary later tick retains every changed driving component; no tick-to-current replay occurs on load. Drop only caches already proven noncanonical.
- Reject missing/changed dataset bytes, unknown generator/policy, malformed/duplicate/overlapping overrides, out-of-baseline IDs, illegal deletion references, missing runtime rows, tampered digests, truncation and unsupported schemas without partial publication.
- Verify canonical ordering by perturbing source/SQL row order. Test unsupported/missing baselines and failed/atomic explicit migrations without altering the source save.
- Compare format-2 full-state storage and format-3 storage on the same resolved toy world and the real slice once available: file bytes, per-table rows, save/load medians, peak allocations, regenerated-vs-overridden counts and event counts. Separate fresh structural savings from history/runtime growth. Measure the architecture §15.6 tens-of-megabytes hypothesis on representative longer runs; no size/load claim is established by this proposal.

## Consequences

Immutable structure stops being duplicated in every new save, while mutable state and history remain directly restorable. Baseline version retention and full validation become required infrastructure. Most moving residents may still require runtime rows, so baseline omission does not guarantee a dramatic overall size reduction. Sparse collision overrides are conditional on a separately accepted policy, keeping #30 from silently blocking the small slice's existing fail-on-collision rule.

Noah accepted this docs-only contract under his 2026-10-19 standing approval, recorded on #77 and PR #83. Runtime format-1/2 behavior remains active until separate implementation. No crate, dependency, data source, service, FFI or runtime state changes. Acceptance and implementation remain separate gates. Revisit if generator retention becomes unaffordable, component schemas multiply, partial/lazy loading needs a new integrity model, or measurements require compression/compaction under separately scoped issues.
