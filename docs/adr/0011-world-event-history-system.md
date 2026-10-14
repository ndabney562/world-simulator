# ADR 0011: A first-class world event and history system

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (from ChatGPT's review)

## Decision
Meaningful simulation outcomes are recorded as structured **world events**, each with: event ID, simulation timestamp, location, actors and roles, event type (a versioned taxonomy covering life, social, work, built environment, disasters, economy, politics, ecology, climate, celestial, and player interventions), cause and provenance, effects, importance, related events, LOD level, and whether it is an individual or aggregate event.
- The **command journal** (inputs) and the **event log** (outputs) are separate records.
- NPC memories, history summaries, statistics, causal explanations, inspector timelines, LLM context, and tests all consume the same event system.
- Events are stored in the save with indices by actor, location, type, time, and cause, and retained by importance with deterministic compaction into aggregate events.

## Consequences
- A new `ws-events` crate in the core.
- The LOD contract extends to events (individual events roll up into aggregate events).
- Details: [24-world-events-and-causality.md](../architecture/24-world-events-and-causality.md).
