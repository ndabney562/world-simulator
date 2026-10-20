//! Coordinate and reference-frame foundation of the World Simulator
//! (07-coordinates-and-time.md §8.1–8.5, ADR 0015).
//!
//! - [`wgs84`]: the WGS84 ellipsoid, [`Geodetic`] positions, and geodetic ↔
//!   ECEF conversions.
//! - [`frame`]: frame-tagged [`Position`] and [`Vector`] types, so mixing
//!   frames does not compile.
//! - [`enu`]: local East-North-Up frames ([`EnuFrame`]) and ECEF ↔ ENU
//!   transforms.
//! - [`cell`]: S2-style cube-sphere [`CellId`]s, Hilbert-ordered, with
//!   parents, children, containment, and edge neighbours.
//! - [`surface`]: [`SurfacePos`], a sector cell plus a 32-bit offset in the
//!   cell's ENU frame, with deterministic re-expression across cells.
//! - [`reference`](mod@reference): named reference points such as [`BAY_RIDGE`].
//!
//! Units are SI (metres, radians); frames are right-handed; heights are
//! ellipsoidal. All trigonometry comes from the pure-Rust `libm` crate, so
//! results do not depend on the platform math library. Cell IDs and cell
//! membership are Tier 1 (identical on every platform,
//! 25-determinism-policy.md); the continuous conversions are Tier 3 by
//! policy, although they also give the same bits on Linux and macOS.
//!
//! [`solar`]: bounded NOAA/Meeus geometric solar rule1 (ADR0030), with dedicated
//! Tier1 bit goldens. No general coordinate tier is changed.
//!
//! Out of scope here: inertial and barycentric frames, precision Earth rotation,
//! geoid heights, camera-relative rendering, and physics frames.
//!
//! [`BAY_RIDGE`]: crate::reference::BAY_RIDGE

pub mod cell;
pub mod enu;
pub mod error;
pub mod frame;
pub mod reference;
pub mod solar;
pub mod surface;
pub mod wgs84;

pub use cell::CellId;
pub use enu::EnuFrame;
pub use error::CoordError;
pub use frame::{Ecef, Enu, Frame, Position, Vector};
pub use surface::{CellFrame, SECTOR_LEVEL, SurfacePos};
pub use wgs84::Geodetic;
