//! Deterministic randomness for the World Simulator ([ADR 0016] §3,
//! 25-determinism-policy.md).
//!
//! There is no generator object with hidden state. Every random value is a
//! pure function of explicit inputs:
//!
//! - [`hash`]: SipHash-2-4 under a 128-bit key over `u64` words, the one
//!   primitive. [`siphash24`] is the byte form, checked against the
//!   published test vectors.
//! - [`Purpose`]: a domain-separation constant computed from a namespaced
//!   label such as `"ws-world/agent/next-target"`.
//! - [`derive`]: `derive(parent, purpose, address) = hash(parent, purpose,
//!   address)`. Seeds and procedural IDs come from it; seeds nest by
//!   deriving from a parent seed (11-procedural-generation.md §12.5).
//! - [`Stream`]: a key plus an explicit counter held by the caller. Draw
//!   `n` depends only on `(parent seed, purpose, address, n)`, so other
//!   draws, call order, and iteration order cannot change it.
//! - [`below`] and [`unit_f64`]: turning a draw into a value. Tier 1
//!   decisions use [`below`] only.
//!
//! Everything here is integer arithmetic (wrapping add, rotate, xor, and a
//! 128-bit multiply), so results are bit-identical on every platform (Tier
//! 1). No global state, no OS randomness, no wall clock.
//!
//! [ADR 0016]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0016-entity-identity-storage-and-randomness.md

mod sip;

use core::num::NonZeroU64;

pub use sip::{SipHasher24, hash, siphash24};

/// A domain-separation label for derivations and streams.
///
/// The value is the 64-bit FNV-1a hash of the label, computed at compile
/// time. Labels are namespaced by crate (`"<crate>/<what>"`), and each crate
/// tests that its labels are distinct. A label is part of every result
/// derived under it, so changing one is a generator or rules version change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Purpose {
    value: u64,
    label: &'static str,
}

impl Purpose {
    /// The purpose with this label.
    #[must_use]
    pub const fn new(label: &'static str) -> Self {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let bytes = label.as_bytes();
        let mut h = OFFSET;
        let mut i = 0;
        while i < bytes.len() {
            h ^= bytes[i] as u64;
            h = h.wrapping_mul(PRIME);
            i += 1;
        }
        Self { value: h, label }
    }

    /// The 64-bit value used as the second key half.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }

    /// The label this purpose was made from.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.label
    }
}

/// Derives a value from a parent seed, a purpose, and an address:
/// SipHash-2-4 keyed by `(parent, purpose)` over the address words. Tier 1.
///
/// Used for child seeds (`seed(child) = derive(seed(parent), purpose,
/// child_address)`), procedural entity IDs, and Tier 1 generator decisions.
#[must_use]
pub fn derive(parent: u64, purpose: Purpose, address: &[u64]) -> u64 {
    hash(parent, purpose.value, address)
}

/// A counter-based random stream: a key derived once, plus draws indexed by
/// an explicit counter that the caller stores.
///
/// `Stream::new(world_seed, purpose, &[entity_id])` gives the stream of one
/// entity for one purpose, and [`Stream::draw`]`(n)` its `n`-th value. The
/// stream holds no position; which draws have been used is canonical state
/// of the caller (ADR 0016 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream {
    key: u64,
    purpose: Purpose,
}

impl Stream {
    /// The stream with key `derive(parent, purpose, address)`.
    #[must_use]
    pub fn new(parent: u64, purpose: Purpose, address: &[u64]) -> Self {
        Self {
            key: derive(parent, purpose, address),
            purpose,
        }
    }

    /// Draw number `n`: `hash(key, purpose, [n])`. Tier 1.
    #[must_use]
    pub fn draw(&self, n: u64) -> u64 {
        hash(self.key, self.purpose.value, &[n])
    }

    /// The derived key.
    #[must_use]
    pub const fn key(&self) -> u64 {
        self.key
    }
}

/// Maps a draw to an integer in `[0, n)` as `(x × n) >> 64`, with no
/// rejection loop. The bias is at most `n / 2^64`. Integer-only, so it is
/// the form Tier 1 decisions use.
#[must_use]
pub const fn below(x: u64, n: NonZeroU64) -> u64 {
    ((x as u128 * n.get() as u128) >> 64) as u64
}

/// Maps a draw to a float in `[0, 1)` from its top 53 bits, an exact
/// conversion. For Tier 2 code only: Tier 1 decisions never use floats.
#[must_use]
pub fn unit_f64(x: u64) -> f64 {
    // 2^-53, exact.
    const SCALE: f64 = 1.0 / 9_007_199_254_740_992.0;
    (x >> 11) as f64 * SCALE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_value_is_fnv1a_of_the_label() {
        // Published FNV-1a 64 values.
        assert_eq!(Purpose::new("").value(), 0xcbf2_9ce4_8422_2325);
        assert_eq!(Purpose::new("a").value(), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(Purpose::new("foobar").value(), 0x8594_4171_f739_67e8);
        assert_eq!(Purpose::new("x/y").label(), "x/y");
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let n = NonZeroU64::new(7).unwrap();
        assert_eq!(below(0, n), 0);
        assert_eq!(below(u64::MAX, n), 6);
        let one = NonZeroU64::new(1).unwrap();
        assert_eq!(below(u64::MAX, one), 0);
        let mut seen = [false; 7];
        let s = Stream::new(1, Purpose::new("ws-rng/test/below"), &[]);
        for i in 0..1000 {
            let v = below(s.draw(i), n);
            assert!(v < 7);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&b| b));
    }

    #[test]
    fn unit_f64_is_in_the_half_open_unit_interval() {
        assert_eq!(unit_f64(0), 0.0);
        assert!(unit_f64(u64::MAX) < 1.0);
        assert_eq!(unit_f64(1 << 63), 0.5);
    }

    #[test]
    fn stream_key_is_the_derivation() {
        let p = Purpose::new("ws-rng/test/key");
        let s = Stream::new(9, p, &[1, 2]);
        assert_eq!(s.key(), derive(9, p, &[1, 2]));
        assert_eq!(s.draw(3), hash(s.key(), p.value(), &[3]));
    }
}
