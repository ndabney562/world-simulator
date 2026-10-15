//! Local East-North-Up tangent frames and ECEF ↔ ENU transforms.

use crate::frame::{Ecef, Enu, Position, Vector};
use crate::wgs84::{Geodetic, sin_cos};

/// A local East-North-Up frame anchored at a geodetic origin.
///
/// The axes are the ellipsoid's local east, north, and up (the ellipsoid
/// normal at the origin), forming a right-handed orthonormal basis. The
/// origin and rotation are held in 64-bit, so the frame is exact enough to
/// anchor 32-bit offsets anywhere on Earth (07-coordinates-and-time.md §8.3).
///
/// At the poles east and north follow the origin's longitude, which stays
/// well defined because the longitude is part of the origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnuFrame {
    origin: Geodetic,
    origin_ecef: Position<Ecef>,
    /// Rows are the east, north, and up unit vectors in ECEF.
    rows: [[f64; 3]; 3],
}

impl EnuFrame {
    /// Creates the ENU frame whose origin is `origin`.
    #[must_use]
    pub fn new(origin: Geodetic) -> Self {
        let (sin_lat, cos_lat) = sin_cos(origin.lat());
        let (sin_lon, cos_lon) = sin_cos(origin.lon());
        Self {
            origin,
            origin_ecef: origin.to_ecef(),
            rows: [
                [-sin_lon, cos_lon, 0.0],
                [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat],
                [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat],
            ],
        }
    }

    /// The frame's origin.
    #[must_use]
    pub const fn origin(&self) -> Geodetic {
        self.origin
    }

    /// The frame's origin in ECEF.
    #[must_use]
    pub const fn origin_ecef(&self) -> Position<Ecef> {
        self.origin_ecef
    }

    /// The frame's east, north, and up unit vectors, in ECEF.
    #[must_use]
    pub fn axes(&self) -> [Vector<Ecef>; 3] {
        self.rows.map(|r| Vector::new(r[0], r[1], r[2]))
    }

    /// Expresses an ECEF position in this frame.
    #[must_use]
    pub fn to_enu(&self, p: Position<Ecef>) -> Position<Enu> {
        let d = p - self.origin_ecef;
        let v = self.vector_to_enu(d);
        Position::new(v.x, v.y, v.z)
    }

    /// Expresses a position in this frame in ECEF.
    #[must_use]
    pub fn to_ecef(&self, p: Position<Enu>) -> Position<Ecef> {
        self.origin_ecef + self.vector_to_ecef(p.to_vector())
    }

    /// Rotates an ECEF vector into this frame (no translation).
    #[must_use]
    pub fn vector_to_enu(&self, v: Vector<Ecef>) -> Vector<Enu> {
        let [e, n, u] = self.rows;
        Vector::new(
            e[0] * v.x + e[1] * v.y + e[2] * v.z,
            n[0] * v.x + n[1] * v.y + n[2] * v.z,
            u[0] * v.x + u[1] * v.y + u[2] * v.z,
        )
    }

    /// Rotates a vector in this frame into ECEF (no translation).
    #[must_use]
    pub fn vector_to_ecef(&self, v: Vector<Enu>) -> Vector<Ecef> {
        let [e, n, u] = self.rows;
        Vector::new(
            e[0] * v.x + n[0] * v.y + u[0] * v.z,
            e[1] * v.x + n[1] * v.y + u[1] * v.z,
            e[2] * v.x + n[2] * v.y + u[2] * v.z,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(lat_deg: f64, lon_deg: f64) -> EnuFrame {
        EnuFrame::new(Geodetic::from_degrees(lat_deg, lon_deg, 0.0).unwrap())
    }

    #[test]
    fn axes_are_right_handed_and_orthonormal() {
        for (lat, lon) in [
            (0.0, 0.0),
            (40.6, -74.0),
            (-89.9, 170.0),
            (90.0, 0.0),
            (-90.0, -180.0),
        ] {
            let [e, n, u] = frame(lat, lon).axes();
            for a in [e, n, u] {
                assert!((a.length() - 1.0).abs() < 1e-15);
            }
            assert!(e.dot(n).abs() < 1e-15 && n.dot(u).abs() < 1e-15 && u.dot(e).abs() < 1e-15);
            assert!(
                (e.cross(n) - u).length() < 1e-15,
                "east x north = up at ({lat}, {lon})"
            );
        }
    }

    #[test]
    fn equator_prime_meridian_axes() {
        let [e, n, u] = frame(0.0, 0.0).axes();
        assert_eq!((e.x, e.y, e.z), (0.0, 1.0, 0.0));
        assert_eq!((n.x, n.y, n.z), (0.0, 0.0, 1.0));
        assert_eq!((u.x, u.y, u.z), (1.0, 0.0, 0.0));
    }

    #[test]
    fn up_is_the_ellipsoid_normal() {
        // A point straight up the normal has ENU (0, 0, h) and the same
        // latitude and longitude as the origin.
        let origin = Geodetic::from_degrees(40.6, -74.0, 0.0).unwrap();
        let f = EnuFrame::new(origin);
        let above = origin.with_height(250.0).unwrap().to_ecef();
        let enu = f.to_enu(above);
        assert!(enu.x.abs() < 1e-9 && enu.y.abs() < 1e-9, "{enu:?}");
        assert!((enu.z - 250.0).abs() < 1e-9, "{enu:?}");
    }

    #[test]
    fn origin_maps_to_zero_and_round_trips() {
        let f = frame(-33.9, 151.2);
        assert_eq!(f.to_enu(f.origin_ecef()), Position::origin());
        let p = Position::<Enu>::new(123.4, -567.8, 9.0);
        assert!(f.to_enu(f.to_ecef(p)).distance(p) < 1e-9);
        let v = Vector::<Enu>::new(1.0, 2.0, 3.0);
        assert!((f.vector_to_enu(f.vector_to_ecef(v)) - v).length() < 1e-15);
    }

    #[test]
    fn north_points_toward_the_pole() {
        // Moving north from (40.6, -74) increases latitude.
        let f = frame(40.6, -74.0);
        let p = f.to_ecef(Position::new(0.0, 1000.0, 0.0));
        let g = Geodetic::from_ecef(p).unwrap();
        assert!(g.lat() > 40.6_f64.to_radians());
        let p = f.to_ecef(Position::new(1000.0, 0.0, 0.0));
        let g = Geodetic::from_ecef(p).unwrap();
        assert!(
            g.lon() > (-74.0_f64).to_radians(),
            "moving east increases longitude"
        );
    }
}
