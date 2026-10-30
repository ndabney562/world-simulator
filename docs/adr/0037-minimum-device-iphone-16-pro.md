# ADR 0037: Minimum device is the iPhone 16 Pro

- Status: Proposed
- Date: 2026-10-29
- Decided by: Noah (Product Owner), option chosen in a local Claude session on 2026-10-29; proposal drafted by Claude
- Issue: [#120](https://github.com/ndabney562/world-simulator/issues/120)
- Supersedes: 0004

## Context
ADR 0004 made the iPhone 15 Pro (A17 Pro, 8 GB RAM) the minimum device, and every stage exit is measured on it (ADR 0009, [19-roadmap](../architecture/19-roadmap.md), [20-mvp](../architecture/20-mvp.md) §21.3). Noah's only test device is an iPhone 16 Pro (A18 Pro, 8 GB RAM). The TP-011 runs recorded as "Noah's iPhone 15 Pro" were made on that phone, and the remaining Vertical Slice device acceptance (#72, #73, #76) can only be run on it.

A faster phone passing does not show that a slower one passes, so criteria written for the 15 Pro cannot be met honestly with 16 Pro evidence.

## Options considered
1. Make the iPhone 16 Pro the minimum and reference device: matches the hardware actually used for every measurement; drops 15 Pro support, which was never measured.
2. Keep the iPhone 15 Pro minimum and accept 16 Pro runs as substitute evidence with a recorded waiver: fewer edits, but every stage exit carries an unverified-device caveat, and budgets stay calibrated against a device nobody measures.

## Decision
The minimum supported device is the iPhone 16 Pro (A18 Pro, 8 GB RAM), running the current or previous major iOS version. Every stage exit criterion and every performance and memory budget is measured on an iPhone 16 Pro in its sustained (thermally throttled) state.

References to the iPhone 15 Pro in earlier accepted ADRs (0005, 0009, 0013, 0031, 0032, 0035) read as the iPhone 16 Pro from this ADR on. Those ADRs are not edited.

## Consequences
- Vertical Slice device acceptance (#72, #73, #76) and exit criterion 6 are run on Noah's iPhone 16 Pro.
- The iPhone 15 Pro becomes unsupported. Nothing measured is lost: the TP-011 results already came from the 16 Pro and are corrected in place.
- Existing budget numbers stay as initial targets; their measured baselines (for example the ≈220 MB process memory) are already 16 Pro values.
- Every supported device can still run Apple's on-device foundation model.
- Docs updated: AGENTS.md, architecture 02, 06, 15, 19, 20, 21 and README, the ADR index, the Vertical Slice backlog, the Unity README, the #72 verification handoff and the TP-011 spike note. #76's issue body still names the 15 Pro and needs the same change.
- Revisit if a supported device below the 16 Pro becomes a target, which would need its own measured evidence.
