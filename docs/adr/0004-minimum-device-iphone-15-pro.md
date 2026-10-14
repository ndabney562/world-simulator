# ADR 0004: Minimum device is the iPhone 15 Pro

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah

## Decision
The minimum supported device is the iPhone 15 Pro (A17 Pro, 8 GB RAM), running the current or previous major iOS version.

## Consequences
- All performance and memory budgets are calibrated on an iPhone 15 Pro in its sustained (thermally throttled) state.
- Every supported device can run Apple's on-device foundation model, which matters for optional LLM features later.
