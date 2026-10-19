# ADR 0024: procedural ID collision resolution over a frozen generation roster

- Status: Proposed
- Date: 2026-10-19
- Decided by: pending Noah approval; proposed by Codex
- Issue: #30
- Supersedes: none; would extend ADR 0016's deferred collision rule after acceptance

## Context

[Issue #30](https://github.com/ndabney562/world-simulator/issues/30) asks for a Tier 1 rule before City-scale populations approach ten million. [ADR 0016](0016-entity-identity-storage-and-randomness.md) currently detects a collision and fails. The Vertical Slice remains below that trigger. This docs-only proposal does not change that accepted behavior, entity layouts, existing generators, saves, or goldens.

A retry loop at materialization time is insufficient: which address wins would depend on camera or thread order, and adding a lower address later could rename an already observed entity. Saving only an order-dependent winner makes one saved history repeatable, but fails seed/dataset regeneration independence. A resolution rule must define the full competing set before publishing IDs.

## Options considered

1. **Retry on insertion.** Cheap and local, but allocation order changes identities. Rejected.
2. **Widen IDs to 128 bits.** Greatly lowers probability but changes event, save, command and FFI layouts and still needs an explicit residual-collision rule. Deferred until roster construction proves impractical.
3. **Frozen generation roster, primary reservation, canonical retries and sparse saved overrides.** Preserves current IDs for every address whose primary does not collide, keeps existing 64-bit layouts and makes streaming order irrelevant. Proposed here. Requires a complete roster and a measured initialization strategy.
4. **Fail/reseed on any collision.** Matches the current guardrail but a reseed changes unrelated entities and breaks stable references. Retained only for old policies and explicit validation/exhaustion failures, not the desired City policy.

## Proposed decision

### 1. Identity domain and versioning

A baseline pins seed, dataset version/full content digest, generator version and **ID policy version**. Its immutable roster is the complete set of possible procedural generation addresses for that baseline, including entities never materialized or later promoted to notable status. A typed address is `(kind_tag: u32, dataset_version: u64, words: [u64])`, compared lexicographically by kind tag, dataset version, then address words (a proper prefix sorts first). Kind tags and address schemas are versioned and append-only; do not sort purpose strings, memory layouts, hashes or resolved IDs. Initial tags are building=1 and resident=2, with their existing ADR 0016 address words. Future kinds must register a tag/schema before implementation.

The roster is generated from immutable source keys and counter-based generation decisions, never materialization order or resolved entity IDs. Cross-kind collisions participate in the same procedural namespace. Duplicate typed addresses are an input error, not a hash collision. The allocator consumes a validated complete roster and produces a complete mapping atomically; no partial IDs escape on allocation, overflow or input failure.

An entity outside this fixed roster cannot be silently added through the procedural allocator. Runtime births, player creations and other new identities use the existing spawned namespace until a separately accepted policy supports expanding procedural domains. Changing datasets/generator/roster creates a new baseline and needs explicit migration; never extend a saved roster according to camera requests. Roster completeness is checked against a pinned canonical manifest digest and count. A caller cannot declare a conveniently small local roster complete.

### 2. Two-pass canonical allocation

1. Compute every address's existing ADR 0016 primary ID, with its existing kind-specific purpose and address words. Group by primary ID.
2. Reserve **all** distinct primary IDs before retrying any loser. In each collision group, the smallest typed address keeps that primary. A retry must not steal another address's unique primary. This preserves every collision-free current ID.
3. Visit all losing typed addresses in canonical address order. For retry counter `r` from 1 through 1,048,576 inclusive, compute `EntityId::procedural(derive(seed, Purpose::new("ws-world/id/collision-v1"), [1, kind_tag, dataset_version, address_word_count, ...address_words, r]))`, using ADR 0016's exact integer derivation and top-62-bit procedural mapping. Select the first ID absent from the complete reserved set, then reserve it. Retry collision chains include other primaries and earlier resolved losers, across every kind.
4. Exhausting the checked counter bound returns an explicit construction error before publishing any mapping. Never drop, renumber or alias an entity. The bound permits forced-exhaustion tests and bounded failure; it is not a claim that every finite hash space can fit an arbitrary roster.

The retry purpose and integer framing above belong to ID policy 1. Existing generators keep their existing policy until a separately reviewed implementation adopts this policy. Adopting the policy records a new ID-policy field/version even when there are no collisions; it never rewrites old golden IDs without the normal Product Owner gate.

### 3. Streaming, regeneration and overrides

After the full allocation validates, store only losing-address overrides in canonical address order, along with the baseline/roster identity. The primary derivation handles every address absent from this table; the validated manifest proves it was covered. All generation, queries that predict identities, dematerialization/regeneration and baseline reconstruction must consult the same resolved mapping. `ws-ecs` continues rejecting duplicate IDs; it never owns hidden retry state.

Resolved identities survive dematerialization. Promotion to notable status keeps the resolved ID and typed address. Causal links, households and other references use resolved IDs. Deletion never frees a reservation for another procedural address in that baseline. Spawned IDs remain in their separate namespace.

The override table is canonical identity state, not a disposable cache. It must be versioned, persisted, included in the canonical baseline identity/hash contract and validated before load publishes a world. A loader rejects duplicate/out-of-roster addresses, IDs outside the procedural namespace, conflicting targets, bad baseline/roster identity and any table that differs from canonical allocation. Omitting a required override is invalid even if only the winner is currently materialized. The complete allocation can rebuild the table from a trusted pinned roster; a save table alone does not prove completeness. Save-format implementation belongs to [#53](https://github.com/ndabney562/world-simulator/issues/53), with its own accepted format ADR and migration.

### 4. Construction costs and limits

Do not keep the full roster or occupied-ID set as permanent mobile state. A preprocessing/build or bounded startup path may enumerate/sort the roster and emit a sparse collision manifest. Seeds change the map, so a dataset-only precomputed map cannot cover arbitrary seeds. The application must validate seed and every baseline input before using a manifest. If a seed-specific manifest is unavailable, construct it before exposing any world entity; do not infer that a locally generated tile has no remote collisions.

This policy chooses determinism over cheap lazy allocation. Before implementation is approved for City use, measure full roster enumeration, external/bounded sorting if required, temporary disk use, peak memory, startup time and sparse-manifest size at ten million addresses on the intended deployment path. The iPhone budgets still apply. No performance feasibility is asserted by this docs PR. If the complete roster cannot be enumerated within budget, revisit this proposal or widen IDs before City work; do not replace the rule with camera-order allocation.

## Required implementation evidence after acceptance

- Forced same-kind and cross-kind collisions, including retry collisions with another unique primary and with an earlier retry; non-colliding IDs remain the existing goldens.
- Fixed integer golden fixtures on Linux/macOS for address ordering, primary winners, retry framing, collision chains and sparse overrides. Permuting source order, worker partitioning or materialization order produces identical complete maps.
- Duplicate-address, malformed manifest, allocation failure, retry exhaustion and oversized-input cases fail atomically.
- Dematerialize/regenerate, promote, save/load and replay preserve resolved IDs and all references, including when every collision participant is dematerialized. Missing/forged overrides and altered roster count/digest are rejected.
- Existing saves remain on their recorded policy; migration never silently assigns new IDs. No accepted save/FFI schema is changed by this ADR alone.
- Ten-million-address construction and memory measurements before City adoption. Implementation issues and the format ADR must be separate from this docs-only decision PR.

## Consequences

The winner and every retry are deterministic functions of a complete frozen baseline, not its load order. Existing collision-free IDs and the 64-bit namespace layout survive. Sparse overrides should be small at the issue's population scale, but peak construction storage can be large and remains an explicit adoption risk. Unknown or expanding procedural universes are unsupported, rather than given a false order-independence guarantee.

No new service, credential, dependency, data source or implementation ships here. ADR 0016 remains the active rule until Noah accepts this proposal; acceptance does not itself implement it or authorize a silent save migration. Revisit if the roster is unbounded, streaming must expose entities before full allocation, new kinds depend on resolved IDs, measured initialization exceeds device budgets, or planetary identity scale makes a wider namespace preferable.
