# ADR 0012: Practical causal provenance

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (from ChatGPT's review)

## Decision
Consequences keep causal relationships to god interventions and other major events where practical, through four mechanisms:
1. **Direct causal links** recorded by the system that creates an event.
2. **Bounded provenance tags** on state changed by interventions or major events, propagated with decay into events produced while reading that state.
3. **Aggregate attribution** in coarse models where practical.
4. **Optional counterfactual comparison** (a re-run without the intervention, on the same build) for summary views.

Every causal claim carries a confidence class (`direct`, `contributing`, `statistical`, `counterfactual`). This is explicitly **not** a universal causal-inference engine; the model is designed to evolve.

## Consequences
- Every intervention effect type declares the events it emits, the state it tags, and how (or whether) its aggregate share is tracked.
- The system can answer "What happened here?", "Why did this city decline?", and "What consequences resulted from my intervention?" from structured data, with honest confidence.
- Details: [24-world-events-and-causality.md §24.6](../architecture/24-world-events-and-causality.md) and [13-god-interventions.md §14.8](../architecture/13-god-interventions.md).
