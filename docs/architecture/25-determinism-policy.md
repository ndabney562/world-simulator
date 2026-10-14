# Determinism and Reproducibility Policy

Revision 4 addition (ChatGPT review item 4). This document supersedes the stricter wording in earlier revisions of [04](04-simulation-architecture.md) and [01](01-hard-problems.md). See ADR 0010.

## 25.1 Position

Strict, bit-identical determinism across all platforms is **desirable but not an absolute blocker**. What the project needs is reproducibility that is strong enough for:
- Debugging: reproducing a session from a journal.
- Testing: stable results, and golden tests where they make sense.
- Materialization: the same people reappear when you zoom back in.
- Persistence: saves stay compact because the baseline is regenerated rather than stored.
- Causal consistency: events and their links do not contradict each other.

So the policy is **tiered**: exactness is required where those needs depend on it, and bounded numerical differences are accepted elsewhere.

## 25.2 Tiers

### Tier 1: exact everywhere (all builds, all platforms)

Anything that **decides identity or structure in the regenerated baseline** must be exact across platforms and builds of the same data and generator versions, because saves store only deltas against that baseline and may be loaded on a different device or a newer build.

| What | How exactness is achieved |
|---|---|
| Procedural entity IDs and identities (names, traits, household composition) | Integer hashing of (seed, address, purpose); no floating point in the decision path |
| Discrete procedural decisions (number of residents per unit, which unit a household gets, building use where data is silent) | Integer arithmetic, or fixed-point arithmetic, or floats quantised before any comparison |
| Dataset processing outputs (simulation tiles) | Produced once by the pipeline and **stored and versioned**, so they are not recomputed on device |
| Cell IDs and cell membership | Integer operations on cell IDs; positions are snapped deterministically at cell boundaries |
| Event IDs, command ordering, entity iteration order | Defined orderings; no hash-map iteration order, no thread-scheduling dependence |
| Simulation time | 64-bit integers |

### Tier 2: exact on the same build and platform (replay-exact)

Replaying the journal from a snapshot **on the same build and platform** reproduces the same state hash. This is the debugging and testing guarantee, and it is what undo, the "what if" counterfactuals ([24](24-world-events-and-causality.md)), and bug replays rely on.

It requires:
- Counter-based RNG streams keyed by stable IDs, never a shared sequential RNG.
- Deterministic parallelism: work is partitioned, and results are merged in canonical order.
- No wall-clock time in simulation logic.
- A fixed step for physics, with deterministic body insertion order.

Platform math libraries are **allowed** in Tier 2 code. Soft-float or portable math is required only for Tier 1 paths.

### Tier 3: bounded differences accepted (across platforms and builds)

Continuous simulation quantities may differ slightly between platforms (Linux x86-64 vs. iPhone arm64) or builds: positions, velocities, need values, physics trajectories, and orbital positions at the millimetre level. These differences may, over time, change discrete outcomes (an agent picks a different café). That is **accepted** because:
- Saves store canonical state (snapshots plus deltas), not only inputs, so a save loaded elsewhere starts from the same state and only its future evolution may differ.
- Cross-platform replay is a diagnostic, not a feature.

Tests that compare across platforms use **tolerances and invariants** (conservation, statistical equivalence), not hashes.

### Tier 4: non-deterministic by design

Rendering, animation blending, audio, visual providers ([23](23-visual-providers.md)), the frame rate, and LLM calls. None of these feed simulation state, except an LLM output adopted as a **journaled command**, which makes it an input like any other ([16](16-ai-llm-npcs.md)).

## 25.3 Where exactness matters, and where it does not

| Concern | Needed tier | Why |
|---|---|---|
| Same person reappears on zoom-in | Tier 1 | Identity comes from the baseline |
| Save on Mac, load on iPhone | Tier 1 + stored snapshot | Baseline must match; state is loaded, not replayed |
| Bug reproduction from a TestFlight report | Tier 2 | Replayed on the same build in a test harness on the same architecture (arm64 macOS for an iPhone report) |
| Undo of an intervention | Tier 2 | Replay without the command, on device |
| Counterfactual "what if" | Tier 2 | Same device, same build |
| Golden tests for generators | Tier 1 | Must pass identically on Linux and macOS CI |
| Golden tests for multi-tick simulation | Tier 2 | Per platform: Linux CI and macOS CI keep separate golden files where needed |
| Cross-platform simulation comparison | Tier 3 | Tolerances and invariants only |
| Rendering / provider output | Tier 4 | Never compared |

## 25.4 Detecting divergence

- Per-tick state hashes, split by subsystem and region, are recorded in debug builds and tests. When a Tier 2 replay diverges, the first differing tick, subsystem, and region pinpoint the cause.
- A nightly cross-platform job runs the same scenario on Linux and macOS arm64 and reports how far the two drift (Tier 3 metrics). The report is reviewed for trends; unexpected large drift is investigated, but drift alone does not fail the build. Tier 1 differences **do** fail the build.

## 25.5 What this changes from the original proposal

- Software transcendental math, FMA control, and similar measures are now **required only on Tier 1 paths**, which keeps most simulation code simple and fast.
- Rapier's cross-platform determinism mode becomes optional; the same-platform guarantee (Tier 2) is enough.
- Cross-platform bit-identical replay is a **nice-to-have** measured nightly, not an acceptance criterion.
