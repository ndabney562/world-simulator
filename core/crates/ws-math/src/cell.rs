//! S2-style cube-sphere cell IDs.
//!
//! The sphere is projected onto the six faces of a cube; each face is a
//! quadtree 30 levels deep, and cells are numbered along a Hilbert curve so
//! that nearby cells have nearby IDs (07-coordinates-and-time.md §8.3).
//!
//! The bit layout, face orientation, quadratic projection, and Hilbert
//! curve are those of Google's S2 geometry library, so a [`CellId`] has the
//! same `u64` as the S2 `CellID` for the same point: 3 face bits, then 2
//! bits per level, then a single `1` marker bit, then zeros.
//!
//! # Determinism (Tier 1)
//!
//! Cell IDs and cell membership decide identity and structure, so they must
//! be identical on every platform (25-determinism-policy.md §25.2). The
//! path from latitude and longitude to a cell ID uses `libm` for `sin`,
//! `cos`, and `atan2`, and otherwise only IEEE 754 operations that are
//! correctly rounded on every target (`+ − × ÷ √`, comparisons, `floor`).
//! There is no fused multiply-add and no platform math library. Once a
//! point has a cell ID, every operation here (level, parent, children,
//! containment, ordering, Hilbert decoding) is pure integer arithmetic.
//! Edge neighbours across a face boundary use the same basic operations as
//! S2 to re-project onto the adjacent face. Golden tests check fixed IDs on
//! Linux x86-64 and macOS arm64 CI.
//!
//! Latitude and longitude are mapped onto the cube as directions on a unit
//! sphere (the S2 `LatLng` convention), so a cell is the set of geodetic
//! (latitude, longitude) pairs whose direction falls in it, at any height.

use core::fmt;

use crate::enu::EnuFrame;
use crate::error::CoordError;
use crate::frame::{Ecef, Position};
use crate::wgs84::{Geodetic, sin_cos};

/// The finest level: leaf cells, about 1 cm across on Earth.
pub const MAX_LEVEL: u8 = 30;

/// Number of cube faces.
pub const NUM_FACES: u8 = 6;

const FACE_SHIFT: u32 = 61;
/// Leaf cells per face edge (2^30).
const MAX_SIZE: i64 = 1 << MAX_LEVEL;
/// Bits that may hold the lowest set bit of a valid ID (even positions).
const LSB_MASK: u64 = 0x1555_5555_5555_5555;

const SWAP_MASK: u8 = 1;
const INVERT_MASK: u8 = 2;
/// For each orientation, the (i, j) quadrant `(i << 1) | j` visited at each
/// Hilbert position.
const POS_TO_IJ: [[u8; 4]; 4] = [[0, 1, 3, 2], [0, 2, 3, 1], [3, 2, 0, 1], [3, 1, 0, 2]];
/// Inverse of [`POS_TO_IJ`].
const IJ_TO_POS: [[u8; 4]; 4] = [[0, 1, 3, 2], [0, 3, 1, 2], [2, 3, 1, 0], [2, 1, 3, 0]];
/// How the orientation changes when descending into each Hilbert position.
const POS_TO_ORIENTATION: [u8; 4] = [SWAP_MASK, 0, 0, INVERT_MASK | SWAP_MASK];

/// A cell of the S2-style hierarchy, from a whole cube face (level 0) down
/// to a leaf (level 30).
///
/// Every value is valid: construction goes through validating functions.
/// The derived ordering is the Hilbert-curve order, and a cell's
/// descendants occupy a contiguous range of IDs around it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellId(u64);

impl CellId {
    /// Wraps a raw S2-layout ID.
    ///
    /// # Errors
    ///
    /// [`CoordError::InvalidCellId`] unless the face is below 6 and the
    /// lowest set bit is at an even position.
    pub const fn from_raw(raw: u64) -> Result<Self, CoordError> {
        if (raw >> FACE_SHIFT) < NUM_FACES as u64 && (raw & raw.wrapping_neg() & LSB_MASK) != 0 {
            Ok(Self(raw))
        } else {
            Err(CoordError::InvalidCellId)
        }
    }

    /// The raw `u64`, identical to the S2 `CellID` value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// The level-0 cell covering a whole cube face.
    ///
    /// # Errors
    ///
    /// [`CoordError::InvalidCellId`] if `face` is 6 or more.
    pub const fn from_face(face: u8) -> Result<Self, CoordError> {
        if face < NUM_FACES {
            Ok(Self(
                ((face as u64) << FACE_SHIFT) | (1 << (FACE_SHIFT - 1)),
            ))
        } else {
            Err(CoordError::InvalidCellId)
        }
    }

    /// The leaf cell containing a geodetic position's latitude and
    /// longitude. Height is ignored. Tier 1.
    #[must_use]
    pub fn from_geodetic(g: &Geodetic) -> Self {
        let (sin_lat, cos_lat) = sin_cos(g.lat());
        let (sin_lon, cos_lon) = sin_cos(g.lon());
        let xyz = [cos_lon * cos_lat, sin_lon * cos_lat, sin_lat];
        let face = face_of(xyz);
        let (u, v) = face_xyz_to_uv(face, xyz);
        Self::from_face_ij(face, st_to_ij(uv_to_st(u)), st_to_ij(uv_to_st(v)))
    }

    /// The cell at `level` containing a geodetic position. Tier 1.
    ///
    /// # Errors
    ///
    /// [`CoordError::InvalidLevel`] if `level` exceeds [`MAX_LEVEL`].
    pub fn from_geodetic_at_level(g: &Geodetic, level: u8) -> Result<Self, CoordError> {
        Self::from_geodetic(g).parent(level)
    }

    /// The leaf cell containing an ECEF position, through its geodetic
    /// latitude and longitude. Tier 1 (the ECEF → geodetic step also uses
    /// only `libm` and correctly rounded operations).
    ///
    /// # Errors
    ///
    /// As [`Geodetic::from_ecef`].
    pub fn from_ecef(p: Position<Ecef>) -> Result<Self, CoordError> {
        Geodetic::from_ecef(p).map(|g| Self::from_geodetic(&g))
    }

    /// The cube face, 0 to 5.
    #[must_use]
    pub const fn face(self) -> u8 {
        (self.0 >> FACE_SHIFT) as u8
    }

    /// The level, 0 (face) to 30 (leaf).
    #[must_use]
    pub const fn level(self) -> u8 {
        MAX_LEVEL - (self.0.trailing_zeros() / 2) as u8
    }

    /// Whether this is a leaf cell (level 30).
    #[must_use]
    pub const fn is_leaf(self) -> bool {
        self.0 & 1 == 1
    }

    /// The ancestor at `level` (or the cell itself at its own level).
    ///
    /// # Errors
    ///
    /// [`CoordError::InvalidLevel`] if `level` is finer than this cell's.
    pub const fn parent(self, level: u8) -> Result<Self, CoordError> {
        if level > self.level() {
            return Err(CoordError::InvalidLevel);
        }
        let lsb = lsb_for_level(level);
        Ok(Self((self.0 & lsb.wrapping_neg()) | lsb))
    }

    /// The four children in Hilbert order, or `None` for a leaf.
    #[must_use]
    pub const fn children(self) -> Option<[Self; 4]> {
        if self.is_leaf() {
            return None;
        }
        let lsb = self.lsb();
        let first = self.0 - lsb + (lsb >> 2);
        let step = lsb >> 1;
        Some([
            Self(first),
            Self(first + step),
            Self(first + 2 * step),
            Self(first + 3 * step),
        ])
    }

    /// Whether `other` is this cell or one of its descendants.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        let half = self.lsb() - 1;
        other.0 >= self.0 - half && other.0 <= self.0 + half
    }

    /// The four cells at the same level that share an edge with this one,
    /// in S2's order: towards −j, +i, +j, −i in the face's (i, j)
    /// coordinates. Across a face boundary the neighbour lies on the
    /// adjacent face.
    #[must_use]
    pub fn edge_neighbors(self) -> [Self; 4] {
        let level = self.level();
        let size = 1_i64 << (MAX_LEVEL - level);
        let (face, i, j) = self.face_ij_min();
        let (i, j) = (i64::from(i), i64::from(j));
        let neighbour = |ni: i64, nj: i64| {
            let leaf = if (0..MAX_SIZE).contains(&ni) && (0..MAX_SIZE).contains(&nj) {
                // In range on this face: the conversion is lossless.
                Self::from_face_ij(face, ni as u32, nj as u32)
            } else {
                Self::from_face_ij_wrap(face, ni, nj)
            };
            match leaf.parent(level) {
                Ok(cell) => cell,
                Err(_) => unreachable!("a leaf has every level as an ancestor"),
            }
        };
        [
            neighbour(i, j - size),
            neighbour(i + size, j),
            neighbour(i, j + size),
            neighbour(i - size, j),
        ]
    }

    /// The latitude and longitude of the cell's centre, at height 0 on the
    /// ellipsoid. This is the origin of the cell's ENU frame.
    #[must_use]
    pub fn center_geodetic(self) -> Geodetic {
        let size = 1_u64 << (MAX_LEVEL - self.level());
        let (face, i, j) = self.face_ij_min();
        // Exact in f64: the numerators are below 2^31.
        let s = (u64::from(i) as f64 + size as f64 * 0.5) / MAX_SIZE as f64;
        let t = (u64::from(j) as f64 + size as f64 * 0.5) / MAX_SIZE as f64;
        let [x, y, z] = face_uv_to_xyz(face, st_to_uv(s), st_to_uv(t));
        let lat = libm::atan2(z, (x * x + y * y).sqrt());
        let lon = libm::atan2(y, x);
        match Geodetic::new(lat, lon, 0.0) {
            Ok(g) => g,
            Err(_) => unreachable!("atan2 results are finite and in range"),
        }
    }

    /// The cell's local ENU frame: anchored on the WGS84 ellipsoid (height
    /// 0) at the cell's centre.
    #[must_use]
    pub fn enu_frame(self) -> EnuFrame {
        EnuFrame::new(self.center_geodetic())
    }

    const fn lsb(self) -> u64 {
        self.0 & self.0.wrapping_neg()
    }

    /// Encodes the leaf cell at (i, j) on `face` along the Hilbert curve.
    fn from_face_ij(face: u8, i: u32, j: u32) -> Self {
        let mut pos: u64 = 0;
        let mut orientation = face & SWAP_MASK;
        for k in (0..u32::from(MAX_LEVEL)).rev() {
            let ij = (((i >> k) & 1) << 1) | ((j >> k) & 1);
            let p = IJ_TO_POS[usize::from(orientation)][ij as usize];
            pos = (pos << 2) | u64::from(p);
            orientation ^= POS_TO_ORIENTATION[usize::from(p)];
        }
        Self((u64::from(face) << FACE_SHIFT) | (pos << 1) | 1)
    }

    /// Decodes the face and the (i, j) leaf coordinates of the cell's
    /// minimum corner.
    fn face_ij_min(self) -> (u8, u32, u32) {
        let face = self.face();
        let level = u32::from(self.level());
        let mut orientation = face & SWAP_MASK;
        let (mut i, mut j) = (0_u32, 0_u32);
        for k in 0..level {
            let p = ((self.0 >> (59 - 2 * k)) & 3) as usize;
            let ij = POS_TO_IJ[usize::from(orientation)][p];
            i = (i << 1) | u32::from(ij >> 1);
            j = (j << 1) | u32::from(ij & 1);
            orientation ^= POS_TO_ORIENTATION[p];
        }
        let shift = u32::from(MAX_LEVEL) - level;
        (face, i << shift, j << shift)
    }

    /// The leaf cell at (i, j) when (i, j) may lie just outside `face`,
    /// re-projected onto the adjacent face. Follows S2's `FromFaceIJWrap`.
    fn from_face_ij_wrap(face: u8, i: i64, j: i64) -> Self {
        const SCALE: f64 = 1.0 / MAX_SIZE as f64;
        const LIMIT: f64 = 1.0 + f64::EPSILON;
        let i = i.clamp(-1, MAX_SIZE);
        let j = j.clamp(-1, MAX_SIZE);
        // Exact conversions: the values are below 2^31 in magnitude.
        let u = (SCALE * (2 * (i - MAX_SIZE / 2) + 1) as f64).clamp(-LIMIT, LIMIT);
        let v = (SCALE * (2 * (j - MAX_SIZE / 2) + 1) as f64).clamp(-LIMIT, LIMIT);
        let xyz = face_uv_to_xyz(face, u, v);
        let new_face = face_of(xyz);
        let (u, v) = face_xyz_to_uv(new_face, xyz);
        Self::from_face_ij(
            new_face,
            st_to_ij(0.5 * (u + 1.0)),
            st_to_ij(0.5 * (v + 1.0)),
        )
    }
}

impl fmt::Debug for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CellId({:#018x}, face {}, level {})",
            self.0,
            self.face(),
            self.level()
        )
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

const fn lsb_for_level(level: u8) -> u64 {
    1 << (2 * (MAX_LEVEL - level) as u32)
}

/// The face whose axis is the largest component of `p` (S2's tie-breaking).
fn face_of(p: [f64; 3]) -> u8 {
    let [ax, ay, az] = p.map(f64::abs);
    let axis: u8 = if ax > ay {
        if ax > az { 0 } else { 2 }
    } else if ay > az {
        1
    } else {
        2
    };
    if p[usize::from(axis)] < 0.0 {
        axis + 3
    } else {
        axis
    }
}

/// Projects a direction onto `face`'s (u, v) plane.
fn face_xyz_to_uv(face: u8, [x, y, z]: [f64; 3]) -> (f64, f64) {
    match face {
        0 => (y / x, z / x),
        1 => (-x / y, z / y),
        2 => (-x / z, -y / z),
        3 => (z / x, y / x),
        4 => (z / y, -x / y),
        _ => (-y / z, -x / z),
    }
}

/// The (unnormalised) direction of (u, v) on `face`.
fn face_uv_to_xyz(face: u8, u: f64, v: f64) -> [f64; 3] {
    match face {
        0 => [1.0, u, v],
        1 => [-u, 1.0, v],
        2 => [-u, -v, 1.0],
        3 => [-1.0, -v, -u],
        4 => [v, -1.0, -u],
        _ => [v, u, -1.0],
    }
}

/// S2's quadratic projection from u in [-1, 1] to s in [0, 1].
fn uv_to_st(u: f64) -> f64 {
    if u >= 0.0 {
        0.5 * (1.0 + 3.0 * u).sqrt()
    } else {
        1.0 - 0.5 * (1.0 - 3.0 * u).sqrt()
    }
}

/// Inverse of [`uv_to_st`].
fn st_to_uv(s: f64) -> f64 {
    if s >= 0.5 {
        (1.0 / 3.0) * (4.0 * s * s - 1.0)
    } else {
        (1.0 / 3.0) * (1.0 - 4.0 * (1.0 - s) * (1.0 - s))
    }
}

/// The leaf coordinate containing s, clamped to the face.
fn st_to_ij(s: f64) -> u32 {
    let x = (s * MAX_SIZE as f64).floor();
    // Clamped to [0, 2^30 − 1] first, so the cast is exact.
    x.clamp(0.0, (MAX_SIZE - 1) as f64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_cells_have_s2_values() {
        let expected = [
            0x1000_0000_0000_0000,
            0x3000_0000_0000_0000,
            0x5000_0000_0000_0000,
            0x7000_0000_0000_0000,
            0x9000_0000_0000_0000,
            0xb000_0000_0000_0000,
        ];
        for (face, raw) in (0..NUM_FACES).zip(expected) {
            let c = CellId::from_face(face).unwrap();
            assert_eq!(c.raw(), raw);
            assert_eq!((c.face(), c.level()), (face, 0));
        }
        assert_eq!(CellId::from_face(6), Err(CoordError::InvalidCellId));
    }

    #[test]
    fn from_raw_validates() {
        assert_eq!(CellId::from_raw(0), Err(CoordError::InvalidCellId));
        assert_eq!(
            CellId::from_raw(0x2000_0000_0000_0000),
            Err(CoordError::InvalidCellId),
            "odd lsb"
        );
        assert_eq!(
            CellId::from_raw(0xd000_0000_0000_0000),
            Err(CoordError::InvalidCellId),
            "face 6"
        );
        assert_eq!(
            CellId::from_raw(u64::MAX),
            Err(CoordError::InvalidCellId),
            "face 7"
        );
        assert!(CellId::from_raw(1).is_ok(), "leaf on face 0");
        assert!(CellId::from_raw(0x1000_0000_0000_0000).is_ok());
    }

    #[test]
    fn levels_and_parents() {
        let leaf = CellId::from_raw(0x89c2_5a31_2345_6789).unwrap();
        assert_eq!(leaf.level(), 30);
        assert!(leaf.is_leaf());
        for level in 0..=MAX_LEVEL {
            let p = leaf.parent(level).unwrap();
            assert_eq!(p.level(), level);
            assert!(p.contains(leaf));
            assert_eq!(p.face(), leaf.face());
        }
        let l13 = leaf.parent(13).unwrap();
        assert_eq!(l13.parent(14), Err(CoordError::InvalidLevel));
        assert_eq!(l13.parent(13), Ok(l13));
        assert_eq!(leaf.parent(31), Err(CoordError::InvalidLevel));
    }

    #[test]
    fn children_are_ordered_and_contained() {
        let c = CellId::from_face(3).unwrap().children().unwrap()[2];
        let kids = c.children().unwrap();
        for w in kids.windows(2) {
            assert!(w[0] < w[1]);
        }
        for k in kids {
            assert_eq!(k.level(), c.level() + 1);
            assert_eq!(k.parent(c.level()), Ok(c));
            assert!(c.contains(k) && !k.contains(c));
        }
        assert!(CellId::from_raw(1).unwrap().children().is_none());
    }

    #[test]
    fn hilbert_encoding_round_trips_through_decoding() {
        for face in 0..NUM_FACES {
            for (i, j) in [
                (0, 0),
                (1, 0),
                (0, 1),
                (12_345, 678_901),
                ((1 << 30) - 1, (1 << 30) - 1),
            ] {
                let c = CellId::from_face_ij(face, i, j);
                assert_eq!(c.face_ij_min(), (face, i, j));
            }
        }
    }

    #[test]
    fn face_neighbours_match_s2() {
        // From S2's own test suite (s2cell_id_test.cc, `Neighbors`).
        let n = CellId::from_face(1).unwrap().edge_neighbors();
        assert_eq!(n.map(CellId::face), [5, 3, 2, 0]);
        assert!(n.iter().all(|c| c.level() == 0));
    }

    #[test]
    fn projection_round_trips() {
        for u in [-1.0, -0.5, 0.0, 0.25, 1.0] {
            assert!((st_to_uv(uv_to_st(u)) - u).abs() < 1e-15);
        }
        assert_eq!(st_to_ij(-0.1), 0);
        assert_eq!(st_to_ij(1.0), (1 << 30) - 1);
        assert_eq!(st_to_ij(0.5), 1 << 29);
    }
}
