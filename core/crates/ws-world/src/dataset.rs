//! The fixed, versioned toy "dataset": a few box buildings at fixed
//! offsets from the Bay Ridge reference point (#7). Not real data.

use ws_math::reference::BAY_RIDGE;
use ws_math::{CoordError, EnuFrame, Position, SurfacePos};

/// Stable dataset identifier of the built-in toy dataset (ADR 0025 §1).
pub const TOY_DATASET_ID: u32 = 1;

/// Version of the built-in toy dataset. It is an input to every procedural
/// ID and decision, so changing [`TOY_BUILDINGS`] means bumping it and
/// updating the Tier 1 golden values with Product Owner approval.
pub const TOY_DATASET_VERSION: u64 = 1;

/// Version of the toy building structural generator (ADR 0025 §1).
pub const BUILDING_GENERATOR_VERSION: u32 = 1;

/// Version of the toy resident generator (ADR 0025 §1).
pub const RESIDENT_GENERATOR_VERSION: u32 = 1;

/// Identity policy version 1: fail-on-collision (ADR 0016, ADR 0025 §1).
pub const IDENTITY_POLICY_FAIL_ON_COLLISION: u32 = 1;

/// One building of the toy dataset, in metres in the ENU frame at
/// [`BAY_RIDGE`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToyBuildingSpec {
    /// Metres east of the reference point.
    pub east_m: f64,
    /// Metres north of the reference point.
    pub north_m: f64,
    /// Half the side of the square footprint, in metres.
    pub half_extent_m: f32,
    /// Height of the box, in metres.
    pub height_m: f32,
}

/// The toy dataset: five boxes spread over about 1 km, far enough apart
/// that walks between them cross sector-cell edges.
pub const TOY_BUILDINGS: [ToyBuildingSpec; 5] = [
    ToyBuildingSpec {
        east_m: 0.0,
        north_m: 0.0,
        half_extent_m: 10.0,
        height_m: 12.0,
    },
    ToyBuildingSpec {
        east_m: 460.0,
        north_m: 140.0,
        half_extent_m: 8.0,
        height_m: 9.0,
    },
    ToyBuildingSpec {
        east_m: -380.0,
        north_m: 310.0,
        half_extent_m: 12.0,
        height_m: 15.0,
    },
    ToyBuildingSpec {
        east_m: 150.0,
        north_m: -520.0,
        half_extent_m: 9.0,
        height_m: 10.0,
    },
    ToyBuildingSpec {
        east_m: -270.0,
        north_m: -340.0,
        half_extent_m: 11.0,
        height_m: 21.0,
    },
];

/// Where one building of a dataset stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingSite {
    /// Ground-level centre of the footprint.
    pub pos: SurfacePos,
    /// Half the side of the square footprint, in metres.
    pub half_extent_m: f32,
    /// Height of the box, in metres.
    pub height_m: f32,
}

/// A versioned set of building sites. A site's index in `sites` is its
/// building index, an input to procedural IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    /// Dataset version, an input to every procedural ID and decision.
    pub version: u64,
    /// Building sites, by building index.
    pub sites: Vec<BuildingSite>,
}

impl Dataset {
    /// The built-in toy dataset ([`TOY_BUILDINGS`], version
    /// [`TOY_DATASET_VERSION`]).
    ///
    /// # Errors
    ///
    /// Only as [`SurfacePos::from_ecef`]; never for the built-in values.
    pub fn toy() -> Result<Self, CoordError> {
        let frame = EnuFrame::new(BAY_RIDGE);
        let sites = TOY_BUILDINGS
            .iter()
            .map(|b| {
                let p = frame.to_ecef(Position::new(b.east_m, b.north_m, 0.0));
                Ok(BuildingSite {
                    pos: SurfacePos::from_ecef(p)?,
                    half_extent_m: b.half_extent_m,
                    height_m: b.height_m,
                })
            })
            .collect::<Result<_, CoordError>>()?;
        Ok(Self {
            version: TOY_DATASET_VERSION,
            sites,
        })
    }

    /// Canonical little-endian byte representation of this dataset for
    /// content hashing (ADR 0025 §1).
    ///
    /// Layout: `version` (`u64` LE), `sites.len()` (`u32` LE), then per site
    /// `pos.cell().raw()` (`u64` LE), `pos.offset()` (`3 × u32` raw `f32`
    /// bits LE), `half_extent_m` (`u32` raw `f32` bits LE), and `height_m`
    /// (`u32` raw `f32` bits LE).
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let count = u32::try_from(self.sites.len()).expect("fewer than 2^32 sites");
        let mut out = Vec::with_capacity(12 + self.sites.len() * 28);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        for site in &self.sites {
            out.extend_from_slice(&site.pos.cell().raw().to_le_bytes());
            for c in site.pos.offset() {
                out.extend_from_slice(&c.to_bits().to_le_bytes());
            }
            out.extend_from_slice(&site.half_extent_m.to_bits().to_le_bytes());
            out.extend_from_slice(&site.height_m.to_bits().to_le_bytes());
        }
        out
    }

    /// Full content SHA-256 of [`Self::canonical_bytes`] (ADR 0025 §1).
    #[must_use]
    pub fn content_sha256(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }
}

/// FIPS 180-4 SHA-256 digest of `bytes`.
///
/// Pure integer implementation with zero external dependencies.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let compress = |state: &mut [u32; 8], block: &[u8; 64]| {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    };

    let mut chunks = bytes.chunks_exact(64);
    for chunk in &mut chunks {
        let block: &[u8; 64] = chunk.try_into().expect("64-byte chunk");
        compress(&mut state, block);
    }
    let rem = chunks.remainder();
    let bit_len = (bytes.len() as u64) * 8;
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;
    if rem.len() < 56 {
        tail[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let b0: &[u8; 64] = tail[..64].try_into().expect("first tail block");
        compress(&mut state, b0);
    } else {
        tail[120..128].copy_from_slice(&bit_len.to_be_bytes());
        let b0: &[u8; 64] = tail[..64].try_into().expect("first tail block");
        let b1: &[u8; 64] = tail[64..128].try_into().expect("second tail block");
        compress(&mut state, b0);
        compress(&mut state, b1);
    }

    let mut out = [0u8; 32];
    for (i, word) in state.into_iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8; 32]) -> String {
        let mut s = String::with_capacity(64);
        for b in bytes {
            use core::fmt::Write;
            write!(&mut s, "{b:02x}").unwrap();
        }
        s
    }

    #[test]
    fn sha256_matches_nist_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn toy_dataset_canonical_bytes_and_sha256_are_deterministic_and_bit_sensitive() {
        let ds = Dataset::toy().unwrap();
        let bytes = ds.canonical_bytes();
        assert_eq!(bytes.len(), 12 + 5 * 28);
        let digest = ds.content_sha256();
        assert_eq!(Dataset::toy().unwrap().content_sha256(), digest);

        let mut altered = ds;
        altered.sites[2].height_m = f32::from_bits(altered.sites[2].height_m.to_bits() ^ 1);
        assert_ne!(altered.content_sha256(), digest);
    }
}
