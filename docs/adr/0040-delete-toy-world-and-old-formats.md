# ADR 0040: Delete the toy world, old save formats and old ABI versions

- Status: Accepted
- Date: 2026-10-31
- Decided by: Noah, on 2026-10-31 after the efficiency audit (findings AR-1, AR-2, AR-3); proposed by Claude for issue #139
- Issue: #139
- Supersedes: the toy-world parts of ADRs [0016](0016-entity-identity-storage-and-randomness.md), [0017](0017-simulation-lod-l1-l2-round-trip.md), [0019](0019-command-journal-and-world-events.md), [0020](0020-save-format-and-versioning.md), [0021](0021-god-intervention-contract.md), [0022](0022-toy-world-render-snapshot-abi-3.md) and [0025](0025-baseline-plus-deltas-saves.md)

## Context

Two simulations are maintained end to end. The game is the Vertical Slice: the RealBlock scene, the `ws_slice_*` exports, iOS and web builds ([vertical-slice-exit.md](../spikes/vertical-slice-exit.md)). The Technical Prototype's toy world (five box buildings, the Bootstrap scene, toy `World`, toy saves and the toy FFI) is still built, tested and carried through every ABI and save-format bump. It is about 15k lines including tests and Unity code, and about 336 s of the 893 s CI test time measured before the optimised test profile.

Every older save format (1 to 5) and every older ABI (3 to 9) also keeps a reader, fixtures or a C smoke test. No player has a save outside the repository, and nothing ships before the first release.

## Options considered

1. **Keep the toy world as a test bed.** No work now, but every core change still pays the toy's cost, and the slice already exercises every spine on real data.
2. **Delete the toy world and keep old-format readers.** Removes most code, but readers for formats nobody holds still need maintaining.
3. **Delete the toy world, every reader for an older save format, and every older-ABI test.** Keep one refusal check for each: an older save-format number and an older ABI word are refused cleanly.

## Decision

Option 3.

- **Unity:** RealBlock is the only scene and the only build scene. The toy host, decoder, render frame, orbit camera, Bootstrap scene, their tests and the "Build iOS performance spike" menu are deleted. `NativeWorld` keeps only the ABI version check and UTC conversion.
- **FFI and CLI:** the toy exports (`ws_create`, `ws_submit`, `ws_advance`, `ws_fill_render_snapshot`, `ws_get_control_state`, `ws_get_calendar_sky`) and the older-ABI smoke sources are deleted. Removing exports bumps the ABI. `ws-cli` keeps only the slice path.
- **Crates:** the toy `World`, toy LOD, toy commands, toy restore and toy illness are deleted from `ws-world` and `ws-sim`. `ws-persist` loads only the current save format and refuses every older one.
- **Kept, because the slice uses it:** identities and environment in `ws-world`, the clock and the calendar-sky record in `ws-sim`, `ws_abi_version`, `ws_utc_to_sim` and the status constants.
- **Unchanged:** every slice golden hash stays byte-identical. The slice's simulation contracts in ADRs 0023 onwards are not changed by this ADR.

The work lands in three PRs, in this order: Unity, then FFI and CLI, then the crates. Each PR deletes only code that nothing still built depends on.

## Consequences

- The toy parts of the ADRs listed under Supersedes describe deleted code. Their slice-relevant rules stay in force: stable IDs and counter-based randomness (0016), the L1/L2 conservation idea as applied to the slice ([ADR 0029](0029-slice-camera-interest-and-aggregates.md)), event and causality records (0019), and the save container and header (0020).
- Saves written before the current format no longer load. Tests keep one fixture for each refusal.
- A save-format or ABI bump now changes one reader and one export set.
- Revisit if a second, synthetic test world is needed for scale testing beyond the slice's 30 residents. That would be a new ADR, not a restored toy world.
