# ADR 0010: Tiered reproducibility instead of strict cross-platform determinism

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (from ChatGPT's review)

## Context
The original proposal treated bit-identical determinism as a hard requirement. Achieving that across platforms with floating point is costly and brittle, and most needs do not require it.

## Decision
Strict cross-platform bit-identical determinism is **desirable, not a blocker**. Reproducibility is tiered:
1. **Exact everywhere:** identity and structural decisions in the regenerated baseline (IDs, households, discrete procedural choices, cell membership, event IDs and ordering), using integer or quantised operations.
2. **Exact on the same build and platform:** journal replay (debugging, undo, counterfactuals, tests).
3. **Bounded differences accepted** across platforms and builds for continuous values; tests use tolerances and invariants.
4. **Non-deterministic by design:** rendering, providers, audio, and LLM calls (an adopted LLM output becomes a journaled command).

## Consequences
- Saves never rely on cross-platform replay; they store canonical state and regenerate the baseline with Tier 1 exactness.
- Portable math is required only on Tier 1 paths.
- Details: [25-determinism-policy.md](../architecture/25-determinism-policy.md).
