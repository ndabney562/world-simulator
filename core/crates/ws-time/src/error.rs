//! Typed errors.

use core::fmt;

/// A time or duration computation would leave the `i64` microsecond range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeOverflow;

impl fmt::Display for TimeOverflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("simulation time arithmetic overflowed the i64 microsecond range")
    }
}

impl std::error::Error for TimeOverflow {}

/// A UTC calendar label is not valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CivilError {
    /// Month outside 1..=12.
    Month,
    /// Day outside the month's length.
    Day,
    /// Hour outside 0..=23.
    Hour,
    /// Minute outside 0..=59.
    Minute,
    /// Second outside 0..=60.
    Second,
    /// Second 60 on a date and time that is not a leap second in the table.
    NotALeapSecond,
    /// Microsecond outside 0..=999_999.
    Microsecond,
}

impl fmt::Display for CivilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Month => "month must be 1..=12",
            Self::Day => "day is outside the month",
            Self::Hour => "hour must be 0..=23",
            Self::Minute => "minute must be 0..=59",
            Self::Second => "second must be 0..=60",
            Self::NotALeapSecond => "second 60 is valid only at a tabulated leap second",
            Self::Microsecond => "microsecond must be 0..=999999",
        })
    }
}

impl std::error::Error for CivilError {}

/// A step must be a positive duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidStep;

impl fmt::Display for InvalidStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a step must be a positive duration")
    }
}

impl std::error::Error for InvalidStep {}
