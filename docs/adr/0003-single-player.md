# ADR 0003: Single-player only

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah

## Decision
World Simulator is single-player. The simulation runs on device and does not need a server. A network connection is used only for streamed visuals and data tiles.

## Consequences
- No networking, authority, or anti-cheat design is needed.
- The deterministic core and command journal still allow replays, undo, and sharing worlds as files.
