# ws-rng

## Responsibility

Deterministic randomness for the core ([ADR 0016](../../../docs/adr/0016-entity-identity-storage-and-randomness.md) §3, [25-determinism-policy.md](../../../docs/architecture/25-determinism-policy.md), [11-procedural-generation.md](../../../docs/architecture/11-procedural-generation.md) §12.5). There is no generator object with hidden state: every value is a pure function of explicit inputs.

| Item | Provides |
|---|---|
| `hash(k0, k1, words)` | SipHash-2-4 under the 128-bit key `k0 ‖ k1` over `u64` words (little-endian bytes). The one primitive behind derivations, draws, and state hashes |
| `siphash24(k0, k1, bytes)` | The byte form, checked against the 64 reference test vectors |
| `SipHasher24` | The same hash, absorbing words incrementally (state hashes) |
| `Purpose` | A domain-separation constant: 64-bit FNV-1a of a namespaced label such as `"ws-world/agent/next-target"`, computed at compile time |
| `derive(parent, purpose, address)` | `hash(parent, purpose, address)`: child seeds, procedural IDs, Tier 1 generator decisions |
| `Stream` | Key `derive(parent, purpose, address)`; draw `n` is `hash(key, purpose, [n])`. The counter `n` is the caller's canonical state |
| `below(x, n)` | `(x × n) >> 64`: an integer in `[0, n)`, the only form Tier 1 decisions use |
| `unit_f64(x)` | `(x >> 11) × 2⁻⁵³`: a float in `[0, 1)`, for Tier 2 code only |

## Owned state

None. Pure functions and value types.

## Emitted event types

None.

## Invariants

- **Tier 1.** Every function is integer arithmetic (wrapping add, rotate, xor, a 128-bit multiply), so results are bit-identical on every platform. SipHash is implemented here, not taken from `std`, whose `Hasher` output may change between Rust releases.
- A draw depends only on `(parent seed, purpose, address, n)`: other draws, call order, and entity iteration order cannot change it. There is no global or shared generator, no OS randomness, and no wall clock.
- Changing the hash, a purpose label, or an address layout changes every value derived from it, so it is a generator or rules version change. The golden values in `tests/determinism.rs` (cross-checked against an independent Python implementation) and the reference vectors in `tests/vectors.rs` (from the reference implementation's `vectors.h`) fail if anything changes. Updating them needs Product Owner approval ([testing-policy.md](../../../docs/process/testing-policy.md) §2).
- No `unsafe`, no dependencies.
