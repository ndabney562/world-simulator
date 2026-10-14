# ADR 0008: Visual backdrops are optional, swappable providers

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (from ChatGPT's review)

## Context
Google Photorealistic 3D Tiles give photoreal visuals but are display-only, metered, non-cacheable, and changeable by Google. Other visual sources (our open-data tiles, NASA imagery, future sources) have different properties.

## Decision
- Every external visual source is a **visual backdrop provider** implementing one front-end contract: coverage, scale range, availability, cost and quota with a hard local cap, attribution, exclusion-zone support, appearance hints, and lifecycle.
- Providers are **never part of simulation state**. The core never reads them; they never write to it; switching them never changes saves, events, or test results.
- Simulation data sources (ingested offline, stored, versioned) and visual providers (runtime, display-only) are separate concepts.
- The open-data provider is the default. Google is opt-in and capped. NASA imagery covers orbit and space.

## Consequences
- The game works fully offline and for free with Google switched off.
- A front-end test asserts that switching providers leaves the core state hash unchanged.
- Details: [23-visual-providers.md](../architecture/23-visual-providers.md).
