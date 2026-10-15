//! Named reference points for tests and the toy world.

use crate::wgs84::Geodetic;

/// The Bay Ridge reference point: 40.6250° N, 74.0300° W, on the WGS84
/// ellipsoid (height 0 m), in Bay Ridge, Brooklyn (ADR 0006).
///
/// This is a fixed test and toy-world anchor, not a surveyed location.
/// Ground level there is roughly 10 m below the ellipsoid (the geoid is
/// about 33 m below the ellipsoid around New York, and the ground is
/// about 20 m above the geoid); real heights come from the data pipeline.
pub const BAY_RIDGE: Geodetic = match Geodetic::from_degrees(40.625, -74.03, 0.0) {
    Ok(g) => g,
    Err(_) => panic!("the Bay Ridge reference point is a valid geodetic position"),
};
