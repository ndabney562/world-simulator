//! Frame-tagged positions and vectors.
//!
//! [`Position<F>`] and [`Vector<F>`] carry their reference frame `F` as a
//! type parameter, so mixing frames is a compile error rather than a silent
//! bug (07-coordinates-and-time.md §8.2). Moving a value between frames goes
//! through a named conversion, for example [`EnuFrame::to_enu`].
//!
//! The algebra is affine: position − position = vector, position ± vector =
//! position, and vectors add and scale. Adding two positions does not
//! compile.
//!
//! Adding an ECEF position to an ENU vector does not compile:
//!
//! ```compile_fail
//! use ws_math::{Ecef, Enu, Position, Vector};
//! let p = Position::<Ecef>::new(6_378_137.0, 0.0, 0.0);
//! let v = Vector::<Enu>::new(1.0, 0.0, 0.0);
//! let _ = p + v;
//! ```
//!
//! Neither does adding two positions, even in the same frame:
//!
//! ```compile_fail
//! use ws_math::{Ecef, Position};
//! let p = Position::<Ecef>::new(1.0, 2.0, 3.0);
//! let _ = p + p;
//! ```
//!
//! The same-frame version compiles, which shows the failures above come
//! from the frame tags and not from something else:
//!
//! ```
//! use ws_math::{Ecef, Position, Vector};
//! let p = Position::<Ecef>::new(6_378_137.0, 0.0, 0.0);
//! let v = Vector::<Ecef>::new(1.0, 0.0, 0.0);
//! assert_eq!((p + v).x, 6_378_138.0);
//! ```
//!
//! [`EnuFrame::to_enu`]: crate::enu::EnuFrame::to_enu

use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// A reference frame marker. Implemented only by the uninhabited marker
/// types in this module; it exists so frame parameters cannot be arbitrary
/// types.
pub trait Frame: Copy + Debug + PartialEq + 'static + private::Sealed {
    /// Short frame name used in `Debug` output.
    const NAME: &'static str;
}

mod private {
    pub trait Sealed {}
    impl Sealed for super::Ecef {}
    impl Sealed for super::Enu {}
}

/// Earth-centred, Earth-fixed frame on the WGS84 ellipsoid: origin at the
/// Earth's centre of mass, +Z through the North Pole, +X through latitude 0,
/// longitude 0, +Y completing a right-handed system. Metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecef {}

/// A local East-North-Up tangent frame: +X east, +Y north, +Z up along the
/// ellipsoid normal (right-handed). Metres.
///
/// Which ENU frame (its origin) is runtime data, held by an
/// [`EnuFrame`](crate::enu::EnuFrame). The type system separates ENU values
/// from ECEF values, but not one ENU frame from another: code that holds
/// values in two different local frames must keep them apart itself, which
/// is why surface positions carry their cell ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enu {}

impl Frame for Ecef {
    const NAME: &'static str = "Ecef";
}

impl Frame for Enu {
    const NAME: &'static str = "Enu";
}

/// A point in frame `F`, in metres (64-bit).
#[derive(Clone, Copy, PartialEq)]
pub struct Position<F: Frame> {
    /// First axis (ECEF X, or ENU east).
    pub x: f64,
    /// Second axis (ECEF Y, or ENU north).
    pub y: f64,
    /// Third axis (ECEF Z, or ENU up).
    pub z: f64,
    frame: PhantomData<F>,
}

/// A displacement or direction in frame `F`, in metres (64-bit).
#[derive(Clone, Copy, PartialEq)]
pub struct Vector<F: Frame> {
    /// First axis (ECEF X, or ENU east).
    pub x: f64,
    /// Second axis (ECEF Y, or ENU north).
    pub y: f64,
    /// Third axis (ECEF Z, or ENU up).
    pub z: f64,
    frame: PhantomData<F>,
}

impl<F: Frame> Position<F> {
    /// Creates a position from its components in metres.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            x,
            y,
            z,
            frame: PhantomData,
        }
    }

    /// The frame's origin.
    #[must_use]
    pub const fn origin() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// The displacement from the frame's origin to this position.
    #[must_use]
    pub const fn to_vector(self) -> Vector<F> {
        Vector::new(self.x, self.y, self.z)
    }

    /// Whether every component is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Straight-line distance to `other`, in metres.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        (self - other).length()
    }
}

impl<F: Frame> Vector<F> {
    /// Creates a vector from its components in metres.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            x,
            y,
            z,
            frame: PhantomData,
        }
    }

    /// The zero vector.
    #[must_use]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product (right-handed).
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Euclidean length.
    #[must_use]
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// Whether every component is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl<F: Frame> Debug for Position<F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Position<{}>({}, {}, {})",
            F::NAME,
            self.x,
            self.y,
            self.z
        )
    }
}

impl<F: Frame> Debug for Vector<F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Vector<{}>({}, {}, {})", F::NAME, self.x, self.y, self.z)
    }
}

impl<F: Frame> Sub for Position<F> {
    type Output = Vector<F>;
    fn sub(self, rhs: Self) -> Vector<F> {
        Vector::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl<F: Frame> Add<Vector<F>> for Position<F> {
    type Output = Self;
    fn add(self, rhs: Vector<F>) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl<F: Frame> Sub<Vector<F>> for Position<F> {
    type Output = Self;
    fn sub(self, rhs: Vector<F>) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl<F: Frame> AddAssign<Vector<F>> for Position<F> {
    fn add_assign(&mut self, rhs: Vector<F>) {
        *self = *self + rhs;
    }
}

impl<F: Frame> SubAssign<Vector<F>> for Position<F> {
    fn sub_assign(&mut self, rhs: Vector<F>) {
        *self = *self - rhs;
    }
}

impl<F: Frame> Add for Vector<F> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl<F: Frame> Sub for Vector<F> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl<F: Frame> AddAssign for Vector<F> {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl<F: Frame> SubAssign for Vector<F> {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl<F: Frame> Neg for Vector<F> {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl<F: Frame> Mul<f64> for Vector<F> {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl<F: Frame> Mul<Vector<F>> for f64 {
    type Output = Vector<F>;
    fn mul(self, rhs: Vector<F>) -> Vector<F> {
        rhs * self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_minus_position_is_vector_in_same_frame() {
        let a = Position::<Ecef>::new(3.0, 5.0, 7.0);
        let b = Position::<Ecef>::new(1.0, 1.0, 1.0);
        let v: Vector<Ecef> = a - b;
        assert_eq!(v, Vector::new(2.0, 4.0, 6.0));
        assert_eq!(b + v, a);
        assert_eq!(a - v, b);
    }

    #[test]
    fn vector_algebra() {
        let e = Vector::<Enu>::new(1.0, 0.0, 0.0);
        let n = Vector::<Enu>::new(0.0, 1.0, 0.0);
        assert_eq!(
            e.cross(n),
            Vector::new(0.0, 0.0, 1.0),
            "east x north = up (right-handed)"
        );
        assert_eq!(e.dot(n), 0.0);
        assert_eq!((e * 3.0 + n * 4.0).length(), 5.0);
        assert_eq!(2.0 * e, e + e);
        assert_eq!(-e, Vector::new(-1.0, 0.0, 0.0));
        let mut p = Position::<Enu>::origin();
        p += e;
        p -= n;
        assert_eq!(p, Position::new(1.0, -1.0, 0.0));
        assert_eq!(p.distance(Position::origin()), 2.0_f64.sqrt());
    }

    #[test]
    fn finiteness_checks() {
        assert!(Position::<Ecef>::new(1.0, 2.0, 3.0).is_finite());
        assert!(!Position::<Ecef>::new(f64::NAN, 2.0, 3.0).is_finite());
        assert!(!Vector::<Enu>::new(1.0, f64::INFINITY, 3.0).is_finite());
    }

    #[test]
    fn debug_output_names_the_frame() {
        assert_eq!(
            format!("{:?}", Position::<Ecef>::new(1.0, 2.0, 3.0)),
            "Position<Ecef>(1, 2, 3)"
        );
        assert_eq!(
            format!("{:?}", Vector::<Enu>::new(1.0, 2.0, 3.0)),
            "Vector<Enu>(1, 2, 3)"
        );
    }
}
