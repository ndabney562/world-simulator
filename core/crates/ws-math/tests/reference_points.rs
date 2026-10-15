//! Known benchmark points against independently computed ECEF values.
//!
//! Expected values come from PROJ 9.8.1 (pyproj 3.8.0, EPSG:4979 →
//! EPSG:4978) and agree with a 40-digit evaluation of the closed-form
//! WGS84 formulas (mpmath) to within 1 nm. The equator / prime meridian and
//! the poles are the defining values `(a, 0, 0)` and `(0, 0, ±b)`.

use ws_math::reference::BAY_RIDGE;
use ws_math::wgs84::{A, B};
use ws_math::{Ecef, EnuFrame, Geodetic, Position};

/// Tolerance for geodetic → ECEF, in metres (1 µm).
const TOL_M: f64 = 1e-6;

const CASES: &[(f64, f64, f64, [f64; 3])] = &[
    (0.0, 0.0, 0.0, [A, 0.0, 0.0]),
    (0.0, 90.0, 0.0, [0.0, A, 0.0]),
    (0.0, 180.0, -1000.0, [-(A - 1000.0), 0.0, 0.0]),
    (90.0, 0.0, 0.0, [0.0, 0.0, B]),
    (-90.0, 0.0, 0.0, [0.0, 0.0, -B]),
    (
        40.625,
        -74.03,
        0.0,
        [1_333_797.038_850, -4_660_711.921_396, 4_130_905.001_129],
    ),
    (
        40.625,
        -74.03,
        -10.0,
        [1_333_794.950_618, -4_660_704.624_448, 4_130_898.490_075],
    ),
    (
        45.0,
        45.0,
        1000.0,
        [3_194_919.145_061, 3_194_919.145_061, 4_488_055.515_647],
    ),
    (
        -33.8568,
        151.2153,
        50.0,
        [-4_647_005.028_383, 2_553_096.913_659, -3_533_294.983_447],
    ),
    (
        27.9881,
        86.925,
        8848.86,
        [302_769.934_269, 5_636_026.225_470, 2_979_493.490_937],
    ),
];

#[test]
fn benchmark_points_convert_to_reference_ecef() {
    for &(lat, lon, h, [x, y, z]) in CASES {
        let p = Geodetic::from_degrees(lat, lon, h).unwrap().to_ecef();
        // Expected values are printed to 1 µm, so allow rounding of the table.
        let err = p.distance(Position::new(x, y, z));
        assert!(
            err < 2.0 * TOL_M,
            "({lat}, {lon}, {h}): {p:?}, error {err} m"
        );
    }
}

#[test]
fn reference_ecef_converts_back_to_benchmark_geodetic() {
    for &(lat, lon, h, [x, y, z]) in CASES {
        let g = Geodetic::from_ecef(Position::<Ecef>::new(x, y, z)).unwrap();
        assert!(
            (g.lat() - lat.to_radians()).abs() < 1e-12,
            "lat at ({lat}, {lon})"
        );
        if lat.abs() < 90.0 {
            // Longitude is undefined at the poles; 180 and -180 are the same.
            let dlon = (g.lon() - lon.to_radians()).abs();
            assert!(
                dlon < 1e-12 || (dlon - 2.0 * core::f64::consts::PI).abs() < 1e-12,
                "lon at ({lat}, {lon})"
            );
        }
        assert!((g.h() - h).abs() < 1e-5, "h at ({lat}, {lon}): {}", g.h());
    }
}

#[test]
fn bay_ridge_reference_point() {
    assert_eq!(
        BAY_RIDGE,
        Geodetic::from_degrees(40.625, -74.03, 0.0).unwrap()
    );
    let p = BAY_RIDGE.to_ecef();
    let err = p.distance(Position::new(
        1_333_797.038_850,
        -4_660_711.921_396,
        4_130_905.001_129,
    ));
    assert!(err < 2.0 * TOL_M);
}

#[test]
fn enu_of_a_known_offset() {
    // 100 m up from the Bay Ridge point is (0, 0, 100) in its ENU frame, and
    // its ECEF displacement is 100 m along the ellipsoid normal
    // (cos φ cos λ, cos φ sin λ, sin φ).
    let f = EnuFrame::new(BAY_RIDGE);
    let up = f.to_ecef(Position::new(0.0, 0.0, 100.0)) - f.origin_ecef();
    let (lat, lon) = (40.625_f64.to_radians(), (-74.03_f64).to_radians());
    let n = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];
    assert!((up.x - 100.0 * n[0]).abs() < 1e-9);
    assert!((up.y - 100.0 * n[1]).abs() < 1e-9);
    assert!((up.z - 100.0 * n[2]).abs() < 1e-9);
    let higher = BAY_RIDGE.with_height(100.0).unwrap().to_ecef();
    assert!(f.to_enu(higher).distance(Position::new(0.0, 0.0, 100.0)) < 1e-9);
}
