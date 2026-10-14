# ADR 0009: Stage progression starts with a Technical Prototype and a Vertical Slice

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah (from ChatGPT's review)

## Decision
Development proceeds through:

**Technical Prototype → Vertical Slice → Neighbourhood MVP → City → Planet → Solar System**

- **Technical Prototype (MVP-0):** proves the foundational architecture only.
- **Vertical Slice:** about one real Bay Ridge block, about 10–30 synthetic people, 3–5 representative buildings, basic camera and zoom, Rust ↔ Unity communication, simulation time, at least one simulation-LOD transition, save/load, entity inspection, and one god intervention. Visual polish is not a requirement.
- **Neighbourhood MVP:** the 6–12-block Bay Ridge concept, preserved as the first playable version rather than the first implementation target.

## Consequences
- Each stage has exit criteria measured on an iPhone 15 Pro.
- Details: [19-roadmap.md](../architecture/19-roadmap.md), [20-mvp.md](../architecture/20-mvp.md).
