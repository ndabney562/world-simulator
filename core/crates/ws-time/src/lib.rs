//! Simulation time for the World Simulator ([ADR 0014]).
//!
//! - [`SimTime`]: a signed 64-bit count of microseconds of a uniform time
//!   scale, with [`SimTime::EPOCH`] at 2000-01-01T12:00:00 UTC.
//! - [`SimDuration`]: a signed 64-bit count of microseconds.
//! - [`UtcDateTime`]: a proleptic Gregorian UTC label, converted to and from
//!   [`SimTime`] through an embedded, versioned leap-second table.
//! - [`Rate`]: a system's native step, and its epoch-aligned tick indices.
//!
//! Everything here is integer arithmetic (Tier 1, [25-determinism-policy]):
//! no floating point, no wall clock, and no operation that can overflow
//! silently. Fallible arithmetic returns [`TimeOverflow`].
//!
//! [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md
//! [25-determinism-policy]: https://github.com/ndabney562/world-simulator/blob/main/docs/architecture/25-determinism-policy.md

mod civil;
mod duration;
mod error;
mod leap;
mod rate;
mod time;

pub use civil::UtcDateTime;
pub use duration::SimDuration;
pub use error::{CivilError, InvalidStep, TimeOverflow};
pub use leap::LEAP_SECOND_TABLE_VERSION;
pub use rate::Rate;
pub use time::SimTime;

/// Microseconds in one second.
pub const MICROS_PER_SECOND: i64 = 1_000_000;
