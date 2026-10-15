//! The WGS84 ellipsoid, geodetic coordinates, and geodetic ↔ ECEF
//! conversions.
//!
//! Heights are ellipsoidal (above the WGS84 ellipsoid), never orthometric
//! (07-coordinates-and-time.md §8.4). Geoid models are data-pipeline work.
//!
//! All trigonometry uses the pure-Rust `libm` crate, so these conversions
//! give the same bits on every platform (ADR 0015). They feed cell
//! membership, which is Tier 1.

use core::f64::consts::{FRAC_PI_2, PI};

use crate::error::CoordError;
use crate::frame::{Ecef, Position};

/// Semi-major axis (equatorial radius) `a`, in metres. Defining constant.
pub const A: f64 = 6_378_137.0;

/// Inverse flattening `1/f`. Defining constant.
pub const INV_F: f64 = 298.257_223_563;

/// Flattening `f = (a − b) / a`.
pub const F: f64 = 1.0 / INV_F;

/// Semi-minor axis (polar radius) `b = a (1 − f)`, in metres
/// (≈ 6 356 752.314 245 m).
pub const B: f64 = A * (1.0 - F);

/// First eccentricity squared `e² = f (2 − f)`.
pub const E2: f64 = F * (2.0 - F);

/// Second eccentricity squared `e'² = e² / (1 − e²)`.
pub const EP2: f64 = E2 / (1.0 - E2);

/// Positions closer to the Earth's centre than this (in metres) are
/// rejected by [`Geodetic::from_ecef`]. Near the centre the geodetic
/// latitude stops being well defined (inside the ellipsoid's evolute, about
/// 43 km across, a point has several normals), and nothing the simulation
/// places is more than a few kilometres below the surface. The value leaves
/// a wide margin while staying far below any real use.
pub const MIN_GEODETIC_RADIUS: f64 = 1_000_000.0;

/// Fixed number of Bowring iterations in [`Geodetic::from_ecef`]. A fixed
/// count (rather than a convergence test) keeps the operation sequence
/// identical for every input. Two iterations already reach well under a
/// micrometre for terrestrial points; the property tests check the
/// round trip over the documented height range.
const BOWRING_ITERATIONS: usize = 3;

/// A geodetic position on the WGS84 ellipsoid.
///
/// Latitude and longitude are in radians; the height is the ellipsoidal
/// height in metres. Constructed only through validating constructors, so
/// every value is finite with latitude in [-π/2, π/2] and longitude in
/// [-π, π].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geodetic {
    lat: f64,
    lon: f64,
    h: f64,
}

impl Geodetic {
    /// Creates a geodetic position from latitude and longitude in radians
    /// and ellipsoidal height in metres.
    ///
    /// # Errors
    ///
    /// [`CoordError::NonFinite`] if any component is NaN or infinite,
    /// [`CoordError::LatitudeOutOfRange`] if `lat` is outside [-π/2, π/2],
    /// and [`CoordError::LongitudeOutOfRange`] if `lon` is outside [-π, π].
    /// Longitudes are not wrapped silently: the caller decides how.
    pub const fn new(lat: f64, lon: f64, h: f64) -> Result<Self, CoordError> {
        if !(lat.is_finite() && lon.is_finite() && h.is_finite()) {
            return Err(CoordError::NonFinite);
        }
        if lat < -FRAC_PI_2 || lat > FRAC_PI_2 {
            return Err(CoordError::LatitudeOutOfRange);
        }
        if lon < -PI || lon > PI {
            return Err(CoordError::LongitudeOutOfRange);
        }
        Ok(Self { lat, lon, h })
    }

    /// Creates a geodetic position from latitude and longitude in degrees
    /// and ellipsoidal height in metres.
    ///
    /// # Errors
    ///
    /// As [`Geodetic::new`], after conversion to radians.
    pub const fn from_degrees(lat_deg: f64, lon_deg: f64, h: f64) -> Result<Self, CoordError> {
        Self::new(lat_deg.to_radians(), lon_deg.to_radians(), h)
    }

    /// Latitude in radians, in [-π/2, π/2].
    #[must_use]
    pub const fn lat(&self) -> f64 {
        self.lat
    }

    /// Longitude in radians, in [-π, π].
    #[must_use]
    pub const fn lon(&self) -> f64 {
        self.lon
    }

    /// Ellipsoidal height in metres.
    #[must_use]
    pub const fn h(&self) -> f64 {
        self.h
    }

    /// The same latitude and longitude at a different ellipsoidal height.
    ///
    /// # Errors
    ///
    /// [`CoordError::NonFinite`] if `h` is NaN or infinite.
    pub fn with_height(self, h: f64) -> Result<Self, CoordError> {
        Self::new(self.lat, self.lon, h)
    }

    /// Converts to ECEF (closed form).
    #[must_use]
    pub fn to_ecef(&self) -> Position<Ecef> {
        let (sin_lat, cos_lat) = sin_cos(self.lat);
        let (sin_lon, cos_lon) = sin_cos(self.lon);
        let n = prime_vertical_radius(sin_lat);
        Position::new(
            (n + self.h) * cos_lat * cos_lon,
            (n + self.h) * cos_lat * sin_lon,
            (n * (1.0 - E2) + self.h) * sin_lat,
        )
    }

    /// Converts an ECEF position to geodetic coordinates.
    ///
    /// Uses Bowring's method with a fixed number of iterations and a height
    /// formula that is well conditioned at every latitude, including the
    /// poles. The round trip `Geodetic → ECEF → Geodetic` stays well inside
    /// 1 mm for heights from −100 km to +1000 km.
    ///
    /// # Errors
    ///
    /// [`CoordError::NonFinite`] if a component is NaN, infinite, or too
    /// large to square, and
    /// [`CoordError::NearEarthCentre`] if the position is closer to the
    /// Earth's centre than [`MIN_GEODETIC_RADIUS`].
    pub fn from_ecef(p: Position<Ecef>) -> Result<Self, CoordError> {
        if !p.is_finite() {
            return Err(CoordError::NonFinite);
        }
        let r2 = p.x * p.x + p.y * p.y + p.z * p.z;
        if !r2.is_finite() {
            // Components above about 1e154 m overflow when squared.
            return Err(CoordError::NonFinite);
        }
        if r2 < MIN_GEODETIC_RADIUS * MIN_GEODETIC_RADIUS {
            return Err(CoordError::NearEarthCentre);
        }

        let rho = (p.x * p.x + p.y * p.y).sqrt();
        let lon = libm::atan2(p.y, p.x);

        // Bowring: iterate on the parametric (reduced) latitude `beta`.
        let mut beta = libm::atan2(p.z, (1.0 - F) * rho);
        let mut lat = beta;
        for i in 0..BOWRING_ITERATIONS {
            let (sin_beta, cos_beta) = sin_cos(beta);
            lat = libm::atan2(
                p.z + EP2 * B * sin_beta * sin_beta * sin_beta,
                rho - E2 * A * cos_beta * cos_beta * cos_beta,
            );
            if i + 1 < BOWRING_ITERATIONS {
                let (sin_lat, cos_lat) = sin_cos(lat);
                beta = libm::atan2((1.0 - F) * sin_lat, cos_lat);
            }
        }

        let (sin_lat, cos_lat) = sin_cos(lat);
        // Well conditioned everywhere: no division by cos(lat) near the poles.
        let h = rho * cos_lat + p.z * sin_lat - A * (1.0 - E2 * sin_lat * sin_lat).sqrt();

        Self::new(lat.clamp(-FRAC_PI_2, FRAC_PI_2), lon, h)
    }
}

/// Prime-vertical radius of curvature `N(φ) = a / √(1 − e² sin²φ)`.
fn prime_vertical_radius(sin_lat: f64) -> f64 {
    A / (1.0 - E2 * sin_lat * sin_lat).sqrt()
}

/// `(sin x, cos x)` from `libm`, never the platform library.
pub(crate) fn sin_cos(x: f64) -> (f64, f64) {
    (libm::sin(x), libm::cos(x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_constants_match_published_values() {
        // NIMA TR8350.2, table 3.1/3.3.
        assert!((B - 6_356_752.314_245).abs() < 1e-6, "b = {B}");
        assert!((E2 - 6.694_379_990_14e-3).abs() < 1e-14, "e^2 = {E2}");
        assert!((EP2 - 6.739_496_742_28e-3).abs() < 1e-14, "e'^2 = {EP2}");
    }

    #[test]
    fn constructor_rejects_invalid_input() {
        assert_eq!(
            Geodetic::new(f64::NAN, 0.0, 0.0),
            Err(CoordError::NonFinite)
        );
        assert_eq!(
            Geodetic::new(0.0, f64::INFINITY, 0.0),
            Err(CoordError::NonFinite)
        );
        assert_eq!(
            Geodetic::new(0.0, 0.0, f64::NEG_INFINITY),
            Err(CoordError::NonFinite)
        );
        assert_eq!(
            Geodetic::new(1.6, 0.0, 0.0),
            Err(CoordError::LatitudeOutOfRange)
        );
        assert_eq!(
            Geodetic::new(-1.6, 0.0, 0.0),
            Err(CoordError::LatitudeOutOfRange)
        );
        assert_eq!(
            Geodetic::new(0.0, 3.2, 0.0),
            Err(CoordError::LongitudeOutOfRange)
        );
        assert_eq!(
            Geodetic::new(0.0, -3.2, 0.0),
            Err(CoordError::LongitudeOutOfRange)
        );
        assert_eq!(
            Geodetic::from_degrees(90.000_001, 0.0, 0.0),
            Err(CoordError::LatitudeOutOfRange)
        );
        assert!(Geodetic::new(FRAC_PI_2, PI, 0.0).is_ok());
        assert!(Geodetic::new(-FRAC_PI_2, -PI, 0.0).is_ok());
    }

    #[test]
    fn from_ecef_rejects_invalid_input() {
        assert_eq!(
            Geodetic::from_ecef(Position::new(f64::NAN, 0.0, 0.0)),
            Err(CoordError::NonFinite)
        );
        assert_eq!(
            Geodetic::from_ecef(Position::origin()),
            Err(CoordError::NearEarthCentre)
        );
        assert_eq!(
            Geodetic::from_ecef(Position::new(0.0, 0.0, MIN_GEODETIC_RADIUS * 0.999)),
            Err(CoordError::NearEarthCentre)
        );
        assert!(Geodetic::from_ecef(Position::new(0.0, 0.0, MIN_GEODETIC_RADIUS)).is_ok());
    }

    #[test]
    fn equator_prime_meridian() {
        let p = Geodetic::new(0.0, 0.0, 0.0).unwrap().to_ecef();
        assert_eq!((p.x, p.y, p.z), (A, 0.0, 0.0));
        let g = Geodetic::from_ecef(p).unwrap();
        assert_eq!((g.lat(), g.lon()), (0.0, 0.0));
        assert!(g.h().abs() < 1e-9);
    }

    #[test]
    fn poles_from_ecef_are_exact_in_latitude() {
        for (z, lat) in [(B, FRAC_PI_2), (-B, -FRAC_PI_2)] {
            let g = Geodetic::from_ecef(Position::new(0.0, 0.0, z)).unwrap();
            assert_eq!(g.lat(), lat);
            assert!(g.h().abs() < 1e-6, "h = {}", g.h());
        }
    }

    #[test]
    fn with_height_changes_only_height() {
        let g = Geodetic::from_degrees(10.0, 20.0, 5.0).unwrap();
        let g2 = g.with_height(-7.0).unwrap();
        assert_eq!((g2.lat(), g2.lon(), g2.h()), (g.lat(), g.lon(), -7.0));
        assert_eq!(g.with_height(f64::NAN), Err(CoordError::NonFinite));
    }
}
