# ADR 0016: Entity identity, component storage, and counter-based randomness (D-7, S-1)

- Status: Accepted (2026-10-15, by merging PR #24); toy-world parts superseded by [0040](0040-delete-toy-world-and-old-formats.md)
- Date: 2026-10-15
- Decided by: Noah (Product Owner), on Claude's proposal for TP-006; reviewed by ChatGPT
- Issue: #7 (TP-006)
- Supersedes: none. Fixes D-7 of [21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md) in detail, closes S-1 for the Technical Prototype, and settles the cell-edge re-expression rule that [ADR 0015](0015-coordinate-representation.md) left to TP-006 (its Tier 1 requirements are unchanged).

## Context

TP-006 (#7) gives the core its first entities: a toy world of 3–5 box buildings at Bay Ridge with a few dozen agents walking between them. Four things must be fixed before any of it is written, because saves, events, golden tests, and the FFI will depend on them:

1. **Entity identity (D-7).** [09-entities-and-agents.md](../architecture/09-entities-and-agents.md) §10.2 asks for 64-bit IDs with namespace bits: procedural IDs derived from a generation address (Tier 1), persistent promotion that keeps the ID, and a separate namespace for spawned entities. The ID must survive materialization and dematerialization, LOD changes, save and load, and replay, and it must be usable in events, causal links, relationships (person → household → building), lookups, serialization, and the FFI. It must not depend on memory addresses, iteration order, the wall clock, or unseeded randomness.
2. **Component storage (S-1).** §10.1 asks for an ECS evaluated against deterministic iteration order, no implicit parallel reordering, cheap component add and remove, and serialization, hidden behind `ws-ecs`.
3. **Counter-based randomness.** [25-determinism-policy.md](../architecture/25-determinism-policy.md) requires integer hashing of `(seed, address, purpose)` for Tier 1 decisions and counter-based streams keyed by stable IDs, never a shared sequential RNG ([11-procedural-generation.md](../architecture/11-procedural-generation.md) §12.5).
4. **Cell-edge re-expression.** ADR 0015 and PR #21 recorded that `SurfacePos::reexpress` is not idempotent within a fraction of a millimetre of a cell edge, and assigned the remedy to TP-006, where agents first move.

### Measured behaviour at cell edges

Before choosing the remedy, a throwaway probe (not committed) measured `SurfacePos::reexpress` on `main` at `914fc07`, in four level-13 cells (Bay Ridge, the equator, 60° N, and next to a cube vertex). For 12,000 rays from cell centres it located the edge by 64-bit bisection and re-expressed every distinct `f32` offset within ±200 µm of it (218,108 positions):

| Result | Count |
|---|---|
| Re-expression result lies outside its own cell (`cell() != occupied_cell()`) | 2,392 (1.1%) |
| of which a second `reexpress` sends it back to the original cell | 2,392 (all) |
| of which it never settles: it alternates between the two cells on every call, indefinitely | 1,454 |
| Rays with at least one such position | 1,474 of 12,000 (12.3%) |
| Distance from the edge of the unstable positions | 0 to 61.5 µm (one `f32` spacing at offsets of 512–1024 m) |
| Largest position change caused by `reexpress` | 34 µm |

The cause is that the target cell is chosen from the 64-bit point, but the new offset is then rounded to `f32`, and the rounded point can lie on the other side of the edge in both cells' frames. A concrete input that alternates forever: cell `89c2455c00000000`, offset bits `[c29b4918, c405dcb4, 00000000]` (−77.64276, −535.4485, 0 m), which flips between `89c2455c00000000` and `89c2456400000000` on every call.

In a movement test where 3,000 agents walk to a point on a cell edge and stand there for 200 ticks of 0.1 s, re-expressing once per tick, 278 agents changed cell more than once (up to 179 times) without moving.

The results are deterministic, so this is not a Tier 1 violation, but cell ownership is meant to be a stable integer fact: spurious flips would move entities between cells' lists, emit cell-change events that did not happen, and break the "agents are re-expressed correctly" criterion of #7.

## Options considered

### Entity identity

1. **Storage handles as identity** (a slot index or an ECS library's generational `Entity`). Rejected: their values depend on allocation history and insertion order, so the same resident gets a different ID after dematerialization, after a load, or in another run.
2. **Address-encoded IDs** (the bits of the ID are the generation address: kind, building index, slot). Collision-free by construction and invertible. Rejected for now: the field widths would have to be fixed for planet scale today (buildings per cell, residents per building, kinds), and they tie identity to the still-open sector level (S-4) and to how a dataset numbers buildings. Hashing keeps the same derivation inputs without those commitments.
3. **Hashed procedural IDs with namespace bits, plus a counter namespace for spawned entities** (chosen). Matches §10.2: Tier 1, order-free, no storage needed to regenerate an ID, and the generation address is kept beside the ID wherever it is needed to re-derive the entity.

### Component storage

| Option | Canonical order | Dependencies | Notes |
|---|---|---|---|
| Plain `Vec` of structs in insertion order | No: order is insertion order | None | Needs sorting before every hash or system pass; lookup by ID needs an index |
| `BTreeMap<EntityId, C>` per component | Yes | None (std) | Simple and canonical; pointer-chasing iteration, more memory per entry |
| Generational or sparse storage (`slotmap`, a custom sparse set) | No: slot order follows allocation and free history | `slotmap` 1.1, or none | O(1) handles, but handles are not identities; a 62-bit hashed ID cannot index a sparse array directly |
| `hecs` 0.11 | No: archetype order, changed by swap-remove on despawn and by component add or remove | `hashbrown`, `foldhash`, `spin`; `serde` optional | Small, single-threaded, fast archetype queries. Canonical order costs a sort per system pass or a separate sorted index; its `Entity` is generational, so a separate ID component and map are still needed |
| `bevy_ecs` 0.20 standalone | No: table and archetype order | 19 direct dependencies, including the `bevy_tasks` thread pool | Most features. A breaking release every three to five months (0.17 in October 2025 to 0.20 in October 2026). Its scheduler runs systems in parallel by default, so it must be bypassed. `scripts/check_core_deps.py` forbids `bevy.*` crates in `core/` as rendering-engine code, so adopting it would also change a CI guardrail |
| Custom full ECS (archetypes, queries, scheduler) | Could be designed so | None | Most code to write and test, for features TP-006 does not use |
| **Minimal sorted-column store** (chosen) | Yes, by construction | None | Small; contiguous iteration; trivially serialisable; insertion O(n) per entity, O(n + m) per batch |

The deciding facts: every library option iterates in an order that depends on history, so each would need sorting or a sorted index added on top to meet the canonical-order and insertion-order-independence criteria of #7, and none of them provides the persistent ID, which is the hard part. TP-006 has one agent shape and a few dozen to 5,000 agents, where a sorted column is as fast to iterate as a `Vec`.

### Cell-edge re-expression

1. **Hysteresis**: change cell only once the position is more than a margin (for example 1 mm) outside the current cell. In the probe this also removed every switch-back. Not chosen: it leaves an entity up to the margin outside its own cell indefinitely, so its stored cell is no longer a function of its position but of its history; spatial queries, per-cell counts, and aggregates then disagree with geometry inside the band; the margin is a tuning constant; and `reexpress` itself stays non-idempotent.
2. **Wider offsets** (`f64`, or fixed-point `i32` at finer resolution). Not chosen: changes ADR 0015's accepted type and the snapshot layout, costs memory, and does not remove the problem: any finite representation has points whose rounding crosses the edge.
3. **Re-express only on demand, never as a fixed tick step.** Not chosen: 07 §8.3 requires re-expression at a deterministic point in the tick, and stale cells would break per-cell ownership.
4. **Self-consistent re-expression** (chosen): when rounding puts the new offset back across the edge, choose the nearest representable offset that does lie inside the target cell. In the probe this removed every non-idempotent case, with a largest position change of 59.7 µm, and no agent standing on an edge changed cell more than once.

## Decision

### 1. Entity identity

**Type.** `EntityId` is a 64-bit value (`NonZeroU64` inside, so `Option<EntityId>` is also 8 bytes). It is the only persistent simulation identity: components, relationships, events, causal links, the journal, saves, and FFI snapshots refer to entities only by `EntityId`. It is serialised as its raw `u64`, little-endian, and Unity treats it as opaque.

**Layout.** The top two bits are the namespace:

| Bits 63–62 | Namespace | Low 62 bits |
|---|---|---|
| `01` | Procedural | The top 62 bits of `derive(world_seed, purpose, address)` (§3) |
| `10` | Spawned | A counter in canonical state, starting at 0, never reused |
| `00`, `11` | Reserved | Never issued; `0` is never a valid ID |

**Procedural IDs** are Tier 1: integer-only functions of the world seed, the dataset version, and a typed generation address. The entity kind is part of the purpose, so a building and a resident can never share derivation inputs. For TP-006:

| Entity | Purpose label | Address words |
|---|---|---|
| Toy building | `"ws-world/id/building"` | `[dataset_version, building_index]` |
| Toy resident | `"ws-world/id/resident"` | `[dataset_version, building_index, slot]` |

Later stages add kinds (household, unit, workplace) in the same way, with real-data building keys from the simulation tiles in place of the toy index. The generation address is stored in the entity's identity component, because a hashed ID cannot be inverted: it is what lets a dematerialized or notable entity be regenerated.

**Persistent promotion** changes only where an entity's state is stored (the canonical entity table), never its ID ([09](../architecture/09-entities-and-agents.md) §10.6).

**Spawned IDs** (a birth, a god spawning a person) come from `next_spawned_id`, a `u64` in canonical world state that is saved and is allocated only while commands and systems run in canonical order (Tier 2). TP-006 spawns nothing; it defines the namespace only.

**Collisions.** Procedural IDs have 62 random bits, so among `n` entities that coexist in one world's canonical state or store, the chance of any collision is about n² / 2⁶³: 1 × 10⁻⁷ for a million, 1 × 10⁻⁵ for ten million. A collision is never silent: inserting an ID that is already present is an error in `ws-ecs`, and TP-006's collision sweep checks 10,000 seeds × the toy world. A deterministic resolution rule is deferred until a stage where coexisting populations make it plausible (City, Revisit below).

**Transient handles.** `ws-ecs` may use dense indices internally. None leaves the crate in TP-006. If one is exposed later for performance, it is generational, valid only until the next structural change, and never stored in a component, event, save, snapshot, or FFI buffer.

### 2. Component storage

`ws-ecs` is a **minimal sorted-column store** with no dependencies:

- **Columns.** Each component type has one column holding entity IDs in ascending order and the components in a parallel `Vec`. Lookup is a binary search; insert and remove are a binary search and a move; batch insert (materialization) sorts the batch and merges in O(n + m); batch remove likewise. Adding or removing a component (a LOD change) is an insert into or removal from that component's column, with no archetype moves.
- **Canonical order.** Every iteration is in ascending `EntityId` order. Systems read several columns by a merge-join over IDs. A column's contents, and so every iteration and the state hash, depend only on the set of `(EntityId, component)` pairs, never on insertion order.
- **No scheduler, no threads.** Systems are plain functions that `ws-sim` calls in a fixed order. A system writes only the components of the entity it is visiting; effects on other entities wait for commands and events (TP-008). Deterministic parallelism ([25](../architecture/25-determinism-policy.md), Tier 2) is a later decision.
- **Errors, not overwrites.** Inserting an existing ID returns an error; nothing is replaced silently.
- **Serialisation.** A column is a sorted list of `(u64, component)` pairs, which any format chosen under S-2 (TP-009) can write directly.
- **Encapsulation.** No other crate names the storage types' internals or any ECS library ([#7](https://github.com/ndabney562/world-simulator/issues/7) requirement). The contract that later code may rely on is: `EntityId`-keyed access, ascending-ID iteration, explicit insert and remove with errors on duplicates, and no handle escaping the crate.

**Crate placement.** `ws-ecs` defines `EntityId` and the store and depends on no workspace crate, so it moves from the Domain layer to the **Foundation** layer of [17-repository-structure.md](../architecture/17-repository-structure.md) §18.2, beside `ws-math`, `ws-time`, and `ws-rng`. Without the move, `ws-world` (layer 1), which owns the toy world's entities, could not use it, and neither could `ws-events` for entity references. The `LAYERS` table in `scripts/check_core_deps.py` and its copy in [ci.md](../process/ci.md) change together in the TP-006 implementation PR that creates the crate. Deriving a procedural ID combines `ws-rng` (the hash) and `ws-ecs` (the type) in `ws-world`, so the two foundation crates do not depend on each other.

### 3. Counter-based randomness

`ws-rng` has one primitive and no shared generator state:

- **Hash.** `hash(k0, k1, words) -> u64` is **SipHash-2-4** with the 128-bit key `k0 ‖ k1` over the little-endian bytes of `words`. It is integer-only, it is specified with published test vectors (Aumasson and Bernstein, 2012), which the crate's tests check, and it is implemented in the crate (about 60 lines) rather than taken from `std`, whose `Hasher` output is explicitly not stable between Rust releases. Speed is not a concern at TP-006 scale (tens of nanoseconds per call).
- **Purposes.** A purpose is a `u64` computed at compile time from a namespaced label, such as `"ws-world/agent/next-target"`, with 64-bit FNV-1a. Each crate declares its own labels; a test in each crate checks that they are distinct. Changing a label changes results, so it is a generator or rules version change.
- **Derivation.** `derive(parent, purpose, address) = hash(parent, purpose, address)`. Seeds nest as in 11 §12.5: `seed(child) = derive(seed(parent), purpose, child_address)`. The world seed is the root.
- **Streams.** A stream is a key plus an explicit counter: `key = derive(world_seed, purpose, [entity_id])`, and draw `n` is `hash(key, purpose, [n])`. A draw depends only on `(world seed, entity ID, purpose, n)`, so adding a draw elsewhere, reordering entities, or changing call order cannot change it.
- **Counters** are explicit state. For TP-006's target choice, each agent stores the number of targets it has drawn (`u32`), so its k-th target is the same whatever tick it arrives on, which keeps Tier 2 float differences in arrival time from changing which building it picks. A per-tick form, `[entity_id, tick_index, k]`, is allowed for systems that draw every tick. Counters of notable entities are canonical and saved; those of anonymous materialized entities are transient, like the rest of their materialized state ([08](../architecture/08-world-state.md) §9.1).
- **Turning draws into values.** An integer in `[0, n)` is `(x as u128 * n as u128) >> 64`, integer-only with no rejection loop (bias at most n / 2⁶⁴), and is the only form allowed in Tier 1 decisions such as residents per building. A float in `[0, 1)` is `(x >> 11) as f64 × 2⁻⁵³`, an exact conversion, for Tier 2 code only.

The TP-006 generator then decides, all in Tier 1 integers: residents per building from `derive(seed, "ws-world/gen/resident-count", [dataset_version, building_index])`, and each resident's ID from the table in §1.

`state_hash(&World)` (#7) feeds the same SipHash-2-4, under fixed keys, with canonical words in ascending ID order: IDs, integer fields, and `f32`/`f64` fields by their bits. It is Tier 2: stable within one build and platform.

### 4. Self-consistent re-expression at cell edges

**Rule.** A surface position produced by `SurfacePos::reexpress` or `SurfacePos::from_ecef` always lies in its own cell: `p.cell() == p.occupied_cell()`. Re-expression is therefore idempotent, and a position that does not move never changes cell.

`reexpress(p)`:

1. Convert `p` to the 64-bit ECEF point `P` and find the sector cell `T` that contains it (unchanged, Tier 1).
2. If `T == p.cell()`, return `p` unchanged, bit for bit (unchanged).
3. Otherwise express `P` in `T`'s frame with the offset rounded to nearest `f32` (unchanged). If that position lies in `T`, return it.
4. Otherwise (the rounding put it back across the edge), consider the 3 × 3 neighbours of that offset on the `f32` grid: east and north each stepped by −1, 0, or +1 representable values (bit-level steps, exact), up unchanged. Among those lying in `T`, return the one whose ECEF point is nearest to `P` by squared distance, with ties going to the first in a fixed order (east step, then north step, from −1 to +1). If none lies in `T`, repeat with the 5 × 5 neighbourhood, and if still none, return a new `CoordError` variant. The probe never needed more than the 3 × 3 step.

`SurfacePos::from_ecef` applies steps 3 and 4 to the cell containing its input. `SurfacePos::new` and `from_ecef_in` keep accepting offsets outside the cell, which is how movement writes positions between re-expressions.

**Why this keeps ADR 0015's Tier 1 requirements.** The target cell is still decided by the existing Tier 1 lookup of the 64-bit point. Step 4 uses only bit-level `f32` steps, the same cell lookup, and correctly rounded `+ − ×` and comparisons, so the result is bit-identical on every platform. Every input whose result was already self-consistent gets exactly today's result, so the committed golden values in `core/crates/ws-math/tests/golden/cell_ids.txt` are unchanged (the probe checked every `reexpress` row), and no golden update is needed. The 0.1 mm position bound in the `ws-math` README still holds (largest change 59.7 µm).

**Where it runs.** The TP-006 movement system integrates offsets in the agent's current cell frame and calls `reexpress` once per agent at a fixed point at the end of the movement step. Agents arrive when within an arrival radius of their target and then stop integrating (and draw a new target), so a parked agent is never stepped back and forth across an edge by sub-millimetre corrections, which the probe showed is a second, movement-caused source of flips.

**Tests** (in the TP-006 implementation PR):

- `ws-math` regression: the input above (cell `89c2455c00000000`, offset bits `[c29b4918, c405dcb4, 00000000]`) re-expresses to cell `89c2456400000000` and then stays there. On `main` the first result lies outside its own cell and the next call returns to `89c2455c00000000`, so the test fails before the fix. The case is also added as a Tier 1 golden `reexpress` row, checked on Linux and macOS (a new row, not a changed one).
- `ws-math` property test: for a random sector cell anywhere on the globe and a random direction, locate the edge by 64-bit bisection, then re-express every distinct `f32` offset within ±200 µm of it, asserting `cell() == occupied_cell()`, idempotence, a position change under 0.1 mm, and equality with today's result wherever today's result was self-consistent. On `main`, 12% of rays contain a failing offset, so a run of the usual 256 cases fails with near certainty.
- `ws-world` invariant: after every tick, every agent satisfies `cell() == occupied_cell()`, and agents parked on a cell edge keep their cell for the rest of the run.

## Consequences

- IDs, residents per building, and every random outcome are pure functions of explicit inputs, so materialization, LOD changes, save and load, and replay reproduce them without storing anything, and insertion order cannot change the state hash.
- Saves and the event log store entities as raw `u64` IDs. Changing the layout, the hash, a purpose label, or the address words of a kind changes every affected ID: that needs a save-format or generator version change with migration, and Product Owner approval for any Tier 1 golden update ([testing-policy.md](../process/testing-policy.md) §2).
- Procedural IDs include the dataset version, so moving a save to a new dataset version will need an ID mapping (old address → new address). This is part of dataset migration, decided when the first real dataset ships (Vertical Slice), not here.
- `ws-ecs` is a few hundred lines with no dependencies. Iteration is as fast as a `Vec`; single inserts and removes cost O(n) moves, which is acceptable for TP-006 and batched for materialization. TP-006's `criterion` benchmark records per-agent step cost and memory per agent at 50 and 5,000 agents against [15-ios-performance-memory.md](../architecture/15-ios-performance-memory.md) §16.3–16.4. The estimate is about 150 bytes per L1 agent including the per-column ID copies, against the 0.5–1 KB L1 target in 09 §10.8.
- Migration path: because only `ws-ecs` knows the storage, moving to paged sparse sets, archetypes, or a library later keeps `EntityId` as the identity, adds a map from `EntityId` to the library's handle, and either sorts per pass or keeps a sorted index, inside one crate and checked by the same canonical-order and insertion-order tests.
- [17-repository-structure.md](../architecture/17-repository-structure.md) §18.1–18.2 now lists `ws-ecs` as a foundation crate; the dependency-check table follows in the TP-006 implementation PR. 09 §10.1–10.2, 21 (D-7, S-1), and 07 §8.3 point here.
- `SurfacePos::reexpress` gains one rarely taken branch: only positions within about 60 µm of an edge reach it (1.1% of the positions within ±200 µm in the probe), and it costs at most nine extra cell lookups of about 0.6 µs each. The `ws-math` README's known-limitation note is replaced by the invariant in the implementation PR.
- Revisit if: the TP-006 benchmark or later materialization shows the store's insert, remove, or merge-join cost dominating the L1 budget; L0 needs many optional component combinations or parallel systems; coexisting procedural populations approach ten million (decide a deterministic collision-resolution rule before then); SipHash-2-4 becomes a measured bottleneck in bulk generation (a faster counter-based function would be a versioned generator change); or physics-level jitter along cell edges makes hysteresis worth adding on top of the rule in §4, as a policy for when movement calls `reexpress`, not a change to it.
