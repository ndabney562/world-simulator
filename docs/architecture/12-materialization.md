# 13. Materialization and Dematerialization

Covers point 13: how detailed entities appear and disappear as the player changes scale.

## 13.1 Definitions

- **Materialization**: promoting a node (sector cell, building, district) from a coarse simulation level to a finer one, creating individual entities whose state is consistent with the coarse state, with everything persisted about them, and with the current time.
- **Dematerialization**: demoting a node, folding lasting changes into canonical state and aggregates, and discarding the rest.

Both are **transactions** applied at a tick boundary: either the whole node changes level, or none of it does. Both are triggered by the interest manager ([05](05-simulation-lod.md)), never directly by the renderer. The first implementation of this contract (L1 ↔ L2, its validation, atomicity, and place in the tick) is specified in [ADR 0017](../adr/0017-simulation-lod-l1-l2-round-trip.md).

## 13.2 Materialization pipeline

For a node going from coarse level C to fine level F:

1. **Load canonical state** for the node from memory or disk: deltas, notable entities currently located there, ongoing events, observation cache entries.
2. **Generate the baseline** for the node from its simulation tiles (real buildings, streets, terrain) and seeds (synthetic population, schedules, facades), unless cached.
3. **Apply deltas** (demolished buildings, moved-out residents, intervention effects).
4. **Disaggregate** coarse state into individuals: sample exactly the counts in the cohort tables, reusing deterministic procedural identities so the same building yields the same people. Persistent notables are placed first; procedural slots they have vacated are skipped.
5. **Project to the present**: each individual's location and activity is derived from their schedule at the current time (it is 08:40 on a weekday, so this person is commuting and that one is at school), not from replaying elapsed time. Ongoing happenings (a fire that started 20 minutes ago) are applied at their current state.
6. **Refine coarse facts**: where coarse events need specifics (which building burned, who was hurt), specifics are sampled deterministically and recorded as child world events linked to the coarse event, with `meta` provenance ([24](24-world-events-and-causality.md)). From then on they are fact.
7. **Attach finer components** (needs at plausible values seeded from traits and time of day, short-term plans, positions on the navigation graph).
8. **Validate invariants** (counts match aggregates, no duplicate notables, events consistent) in debug builds.
9. **Publish** to the simulation and the front end, which cross-fades from visual proxies to real entities ([06](06-rendering-lod.md)).

Steps 2–7 run as budgeted background jobs; step 9 is the only part on the tick boundary.

## 13.3 Dematerialization pipeline

1. **Classify each entity**:
   - Notable (see [09](09-entities-and-agents.md)): persist its state canonically and keep it at L2 (it does not vanish; it becomes a schedule).
   - Anonymous but changed in a lasting way (moved house, died, was born): record the change as a delta and update aggregates.
   - Anonymous and unchanged relative to its baseline projection: discard. It will be regenerated identically.
2. **Aggregate**: recompute the node's coarse state from the remaining fine state (counts, distributions, budgets, condition), carrying intervention attribution into the aggregates where tracked ([24 §24.6](24-world-events-and-causality.md)).
3. **Convert ongoing activity** to coarse equivalents: in-progress trips become discrete events; a fire becomes a building-level fire state.
4. **Write the observation cache** (see 13.4).
5. **Validate conservation** (debug builds): `aggregate(before) == aggregate(after)` for conserved quantities.
6. **Release** fine components and renderer resources.

## 13.4 The observation cache: "I just saw that person"

Tier 1 regeneration ([25](25-determinism-policy.md)) guarantees the same *people* reappear, but not necessarily the same exact positions and moment-to-moment states, because those depend on fine simulation that was discarded. If the player zooms out for five seconds and back in, the woman who was halfway across the street should still be halfway across the street.

The **observation cache** keeps the exact fine state of recently dematerialized nodes for a short time (tunable, for example minutes of wall-clock time and bounded by memory). If the node is re-materialized within that window, the cached state is restored and advanced by the elapsed time with fine simulation (which is short, so cheap) instead of being regenerated. Beyond the window, schedule projection takes over, which is plausible because enough time has passed.

## 13.5 Hysteresis, budgeting, and prioritisation

- Different thresholds for promotion and demotion, and a minimum dwell time at each level, prevent thrashing at band boundaries.
- Materialization work is prioritised by interest (screen centre, player focus, interventions) and spread across frames with a per-frame budget.
- During fast zooms, the system **predicts** the target band and starts materializing ahead, while the renderer shows visual proxies until real entities are ready. Some nodes may be skipped entirely when the camera passes through them quickly.

## 13.6 Edge cases that need explicit rules

| Situation | Rule |
|---|---|
| An agent walks out of a materialized node into a coarse one | Demoted to L2 with a scheduled arrival in the destination; identity preserved. |
| A coarse model "decides" a specific event happened (a crime in district B) and the player zooms in | The event is a coarse fact (count, location within tolerance); its specifics (who, exactly where) are decided at materialization by seeded sampling and then become canonical. |
| An intervention targets a non-materialized individual (picked from a list) | The individual is materialized at L2 on demand (it has a deterministic identity) and made notable. |
| Provenance tags on dematerialized individuals | Kept on notables; for anonymous individuals, tags fold into the node's aggregate attribution. |
| Two materialization requests overlap (camera plus an intervention on the same node) | Interest scores are combined; the node gets the finer level once. |
| Memory pressure | Lowest-interest nodes are demoted first; notables stay at L2 minimum. |
| Save during a transition | Saves happen at tick boundaries; transitions are atomic at those boundaries. |

## 13.7 Testing materialization

The core property, **round-trip conservation**, is tested by property-based tests over many seeds: materialize, run, dematerialize, materialize again, and check invariants (counts, money, notables, identities). See [18](18-testing-strategy.md).
