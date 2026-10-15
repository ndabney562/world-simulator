//! The error type for invalid coordinate inputs.

use core::fmt;

/// Why a coordinate value or cell ID was rejected.
///
/// Every fallible constructor and conversion in this crate returns this
/// type instead of panicking or producing a NaN, so invalid input is caught
/// where it enters the coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordError {
    /// A component was NaN or infinite (or, for ECEF input, so large that
    /// its square overflows).
    NonFinite,
    /// A latitude was outside [-π/2, π/2] radians.
    LatitudeOutOfRange,
    /// A longitude was outside [-π, π] radians.
    LongitudeOutOfRange,
    /// An ECEF position was closer to the Earth's centre than
    /// [`MIN_GEODETIC_RADIUS`](crate::wgs84::MIN_GEODETIC_RADIUS), where
    /// geodetic latitude is not well defined.
    NearEarthCentre,
    /// A raw `u64` is not a valid cell ID.
    InvalidCellId,
    /// A cell level was above [`MAX_LEVEL`](crate::cell::MAX_LEVEL), or
    /// finer than the cell it was applied to when a coarser level was needed.
    InvalidLevel,
    /// A surface position's cell is not at
    /// [`SECTOR_LEVEL`](crate::surface::SECTOR_LEVEL).
    NotSectorCell,
    /// A cell frame was used with a surface position in a different cell.
    FrameMismatch,
    /// No `f32` offset within two representable steps of the rounded one
    /// lies inside the target cell (ADR 0016 §4). Never observed; it would
    /// indicate a bug in the cell geometry rather than bad input.
    BoundaryUnresolved,
}

impl fmt::Display for CoordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NonFinite => "coordinate component is NaN or infinite",
            Self::LatitudeOutOfRange => "latitude is outside [-pi/2, pi/2] radians",
            Self::LongitudeOutOfRange => "longitude is outside [-pi, pi] radians",
            Self::NearEarthCentre => {
                "position is too close to the Earth's centre for geodetic coordinates"
            }
            Self::InvalidCellId => "value is not a valid cell ID",
            Self::InvalidLevel => "cell level is out of range for this operation",
            Self::NotSectorCell => "surface positions must use a sector-level cell",
            Self::FrameMismatch => "cell frame does not belong to this position's cell",
            Self::BoundaryUnresolved => {
                "no representable offset near the position lies inside its cell"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for CoordError {}
