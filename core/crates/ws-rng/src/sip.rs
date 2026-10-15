//! SipHash-2-4 (Aumasson and Bernstein, 2012), implemented here so its
//! output is fixed forever. `std`'s `Hasher` implementations are explicitly
//! not stable between Rust releases, so they are never used for results.

const C0: u64 = 0x736f_6d65_7073_6575;
const C1: u64 = 0x646f_7261_6e64_6f6d;
const C2: u64 = 0x6c79_6765_6e65_7261;
const C3: u64 = 0x7465_6462_7974_6573;

/// SipHash state: four 64-bit words.
#[derive(Debug, Clone, Copy)]
struct State {
    v: [u64; 4],
}

impl State {
    const fn new(k0: u64, k1: u64) -> Self {
        Self {
            v: [k0 ^ C0, k1 ^ C1, k0 ^ C2, k1 ^ C3],
        }
    }

    fn round(&mut self) {
        let [v0, v1, v2, v3] = &mut self.v;
        *v0 = v0.wrapping_add(*v1);
        *v1 = v1.rotate_left(13);
        *v1 ^= *v0;
        *v0 = v0.rotate_left(32);
        *v2 = v2.wrapping_add(*v3);
        *v3 = v3.rotate_left(16);
        *v3 ^= *v2;
        *v0 = v0.wrapping_add(*v3);
        *v3 = v3.rotate_left(21);
        *v3 ^= *v0;
        *v2 = v2.wrapping_add(*v1);
        *v1 = v1.rotate_left(17);
        *v1 ^= *v2;
        *v2 = v2.rotate_left(32);
    }

    /// Absorbs one 8-byte block with two compression rounds.
    fn compress(&mut self, m: u64) {
        self.v[3] ^= m;
        self.round();
        self.round();
        self.v[0] ^= m;
    }

    /// Absorbs the final block (`b`, which carries the length byte) and
    /// runs the four finalization rounds.
    fn finish(mut self, b: u64) -> u64 {
        self.compress(b);
        self.v[2] ^= 0xff;
        for _ in 0..4 {
            self.round();
        }
        self.v[0] ^ self.v[1] ^ self.v[2] ^ self.v[3]
    }
}

/// SipHash-2-4 of `msg` under the 128-bit key `k0 ‖ k1` (each half read as
/// little-endian, as in the reference implementation).
///
/// This byte-oriented form exists to check the implementation against the
/// published test vectors; simulation code uses [`hash`] or [`SipHasher24`].
#[must_use]
pub fn siphash24(k0: u64, k1: u64, msg: &[u8]) -> u64 {
    let mut s = State::new(k0, k1);
    let mut blocks = msg.chunks_exact(8);
    for block in &mut blocks {
        let mut b = [0u8; 8];
        b.copy_from_slice(block);
        s.compress(u64::from_le_bytes(b));
    }
    let tail = blocks.remainder();
    let mut last = [0u8; 8];
    last[..tail.len()].copy_from_slice(tail);
    // The length byte is the message length modulo 256 (SipHash spec).
    last[7] = (msg.len() & 0xff) as u8;
    s.finish(u64::from_le_bytes(last))
}

/// Incremental SipHash-2-4 over a sequence of `u64` words, each absorbed as
/// its 8 little-endian bytes.
///
/// `SipHasher24::new(k0, k1)` followed by `write_u64` for each word and
/// [`SipHasher24::finish`] gives exactly `siphash24(k0, k1, bytes)` where
/// `bytes` is the concatenated little-endian encoding of the words. It does
/// not allocate.
#[derive(Debug, Clone, Copy)]
pub struct SipHasher24 {
    state: State,
    words: u64,
}

impl SipHasher24 {
    /// Starts a hash under the key `k0 ‖ k1`.
    #[must_use]
    pub const fn new(k0: u64, k1: u64) -> Self {
        Self {
            state: State::new(k0, k1),
            words: 0,
        }
    }

    /// Absorbs one word.
    pub fn write_u64(&mut self, word: u64) {
        self.state.compress(word);
        self.words = self.words.wrapping_add(1);
    }

    /// Absorbs several words in order.
    pub fn write_words(&mut self, words: &[u64]) {
        for &w in words {
            self.write_u64(w);
        }
    }

    /// The hash of everything absorbed so far. The hasher is consumed, so a
    /// finished hash cannot be extended by mistake.
    #[must_use]
    pub fn finish(self) -> u64 {
        // A whole number of 8-byte words leaves an empty tail, so the final
        // block holds only the length byte: (8 × words) mod 256.
        let len_byte = self.words.wrapping_mul(8) & 0xff;
        self.state.finish(len_byte << 56)
    }
}

/// SipHash-2-4 of the words `words` (as little-endian bytes) under the key
/// `k0 ‖ k1`. The one primitive behind every derivation, random draw, and
/// state hash in the core (ADR 0016 §3).
#[must_use]
pub fn hash(k0: u64, k1: u64, words: &[u64]) -> u64 {
    let mut h = SipHasher24::new(k0, k1);
    h.write_words(words);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_bytes(words: &[u64]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn word_hash_equals_byte_hash_of_little_endian_words() {
        let mut x = 0x0123_4567_89ab_cdef_u64;
        for len in 0..40 {
            let words: Vec<u64> = (0..len)
                .map(|i| {
                    x = x.rotate_left(7) ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                    x
                })
                .collect();
            let (k0, k1) = (x, !x);
            assert_eq!(hash(k0, k1, &words), siphash24(k0, k1, &le_bytes(&words)));
        }
    }

    #[test]
    fn incremental_writes_equal_one_slice() {
        let words = [1, 2, 3, u64::MAX, 0];
        let mut h = SipHasher24::new(5, 6);
        h.write_u64(1);
        h.write_words(&[2, 3]);
        h.write_u64(u64::MAX);
        h.write_u64(0);
        assert_eq!(h.finish(), hash(5, 6, &words));
    }

    #[test]
    fn the_length_is_part_of_the_hash() {
        assert_ne!(hash(0, 0, &[]), hash(0, 0, &[0]));
        assert_ne!(hash(0, 0, &[0]), hash(0, 0, &[0, 0]));
    }
}
