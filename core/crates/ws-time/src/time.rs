//! [`SimTime`]: an instant on the uniform simulation time scale.

use core::fmt;

use crate::leap::{naive_to_uniform, uniform_to_naive};
use crate::{MICROS_PER_SECOND, SimDuration, TimeOverflow, UtcDateTime};

/// An instant of simulation time: signed microseconds since
/// 2000-01-01T12:00:00 UTC on a uniform scale ([ADR 0014]).
///
/// The scale has no leap-second discontinuities: each inserted leap second
/// is one ordinary microsecond-counted second, labelled `23:59:60` in UTC.
/// `Copy`, 8 bytes, totally ordered. Arithmetic is checked or saturating
/// only.
///
/// [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SimTime(i64);

impl SimTime {
    /// 2000-01-01T12:00:00 UTC.
    pub const EPOCH: Self = Self(0);
    /// The earliest representable instant (about 290,308 BC).
    pub const MIN: Self = Self(i64::MIN);
    /// The latest representable instant (about AD 294,247).
    pub const MAX: Self = Self(i64::MAX);

    /// The instant `micros` microseconds after the epoch (before, if negative).
    #[must_use]
    pub const fn from_micros_since_epoch(micros: i64) -> Self {
        Self(micros)
    }

    /// Microseconds since the epoch.
    #[must_use]
    pub const fn as_micros_since_epoch(self) -> i64 {
        self.0
    }

    /// `self + d`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result is out of range.
    pub const fn checked_add(self, d: SimDuration) -> Result<Self, TimeOverflow> {
        match self.0.checked_add(d.as_micros()) {
            Some(v) => Ok(Self(v)),
            None => Err(TimeOverflow),
        }
    }

    /// `self - d`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result is out of range.
    pub const fn checked_sub(self, d: SimDuration) -> Result<Self, TimeOverflow> {
        match self.0.checked_sub(d.as_micros()) {
            Some(v) => Ok(Self(v)),
            None => Err(TimeOverflow),
        }
    }

    /// The signed duration from `earlier` to `self`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the difference does not fit in a [`SimDuration`]
    /// (instants more than about 292,000 years apart).
    pub const fn checked_duration_since(self, earlier: Self) -> Result<SimDuration, TimeOverflow> {
        match self.0.checked_sub(earlier.0) {
            Some(v) => Ok(SimDuration::from_micros(v)),
            None => Err(TimeOverflow),
        }
    }

    /// `self + d`, clamped to [`SimTime::MIN`]..=[`SimTime::MAX`].
    #[must_use]
    pub const fn saturating_add(self, d: SimDuration) -> Self {
        Self(self.0.saturating_add(d.as_micros()))
    }

    /// `self - d`, clamped to [`SimTime::MIN`]..=[`SimTime::MAX`].
    #[must_use]
    pub const fn saturating_sub(self, d: SimDuration) -> Self {
        Self(self.0.saturating_sub(d.as_micros()))
    }

    /// The instant labelled `utc`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if `utc` lies outside [`SimTime::MIN`]..=[`SimTime::MAX`].
    pub fn from_utc(utc: UtcDateTime) -> Result<Self, TimeOverflow> {
        let seconds = naive_to_uniform(utc.naive_seconds(), utc.is_leap_second());
        // i128 so that instants just above i64::MIN, whose whole-second
        // part alone is below it, combine exactly before the range check.
        let micros =
            i128::from(seconds) * i128::from(MICROS_PER_SECOND) + i128::from(utc.microsecond());
        i64::try_from(micros).map(Self).map_err(|_| TimeOverflow)
    }

    /// The UTC label of this instant. Total: every `SimTime` has one.
    #[must_use]
    pub fn to_utc(self) -> UtcDateTime {
        let seconds = self.0.div_euclid(MICROS_PER_SECOND);
        // rem_euclid is in 0..1_000_000, so the cast is lossless.
        let micros = self.0.rem_euclid(MICROS_PER_SECOND) as u32;
        let (naive, leap) = uniform_to_naive(seconds);
        UtcDateTime::from_naive(naive, leap, micros)
    }
}

/// Formats as the UTC label, for example `2000-01-01T12:00:00.000000Z`.
impl fmt::Display for SimTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_utc(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn t(us: i64) -> SimTime {
        SimTime::from_micros_since_epoch(us)
    }
    const fn d(us: i64) -> SimDuration {
        SimDuration::from_micros(us)
    }

    #[test]
    fn checked_arithmetic_at_boundaries() {
        assert_eq!(SimTime::MAX.checked_add(d(1)), Err(TimeOverflow));
        assert_eq!(SimTime::MIN.checked_add(d(-1)), Err(TimeOverflow));
        assert_eq!(SimTime::MIN.checked_sub(d(1)), Err(TimeOverflow));
        assert_eq!(SimTime::MAX.checked_sub(d(-1)), Err(TimeOverflow));
        assert_eq!(SimTime::MIN.checked_add(SimDuration::MAX), Ok(t(-1)));
        assert_eq!(t(10).checked_add(d(-3)), Ok(t(7)));
        assert_eq!(t(10).checked_sub(d(3)), Ok(t(7)));
    }

    #[test]
    fn duration_between_instants() {
        assert_eq!(t(10).checked_duration_since(t(4)), Ok(d(6)));
        assert_eq!(t(4).checked_duration_since(t(10)), Ok(d(-6)));
        assert_eq!(
            SimTime::MAX.checked_duration_since(SimTime::MIN),
            Err(TimeOverflow)
        );
        assert_eq!(
            SimTime::MAX.checked_duration_since(SimTime::EPOCH),
            Ok(SimDuration::MAX)
        );
    }

    #[test]
    fn saturating_arithmetic_clamps() {
        assert_eq!(SimTime::MAX.saturating_add(d(5)), SimTime::MAX);
        assert_eq!(SimTime::MIN.saturating_sub(d(5)), SimTime::MIN);
        assert_eq!(SimTime::MIN.saturating_add(d(-5)), SimTime::MIN);
    }

    #[test]
    fn ordering_follows_micros() {
        assert!(t(-1) < SimTime::EPOCH);
        assert!(SimTime::MIN < SimTime::MAX);
        assert_eq!(SimTime::default(), SimTime::EPOCH);
    }

    #[test]
    fn display_is_the_utc_label() {
        assert_eq!(SimTime::EPOCH.to_string(), "2000-01-01T12:00:00.000000Z");
        assert_eq!(t(-1).to_string(), "2000-01-01T11:59:59.999999Z");
    }

    #[test]
    fn extremes_convert_and_round_trip() {
        for x in [
            SimTime::MIN,
            t(i64::MIN + 1),
            t(-1),
            SimTime::EPOCH,
            t(i64::MAX - 1),
            SimTime::MAX,
        ] {
            assert_eq!(SimTime::from_utc(x.to_utc()), Ok(x), "{x:?}");
        }
        assert!(SimTime::MIN.to_utc().year() < -290_000);
        assert!(SimTime::MAX.to_utc().year() > 294_000);
    }

    #[test]
    fn labels_beyond_the_range_overflow() {
        let max = SimTime::MAX.to_utc();
        let next_year = UtcDateTime::new(max.year() + 1, 1, 1, 0, 0, 0, 0).unwrap();
        assert_eq!(SimTime::from_utc(next_year), Err(TimeOverflow));
        let far_past = UtcDateTime::new(i32::MIN, 1, 1, 0, 0, 0, 0).unwrap();
        assert_eq!(SimTime::from_utc(far_past), Err(TimeOverflow));
        let far_future = UtcDateTime::new(i32::MAX, 12, 31, 23, 59, 59, 999_999).unwrap();
        assert_eq!(SimTime::from_utc(far_future), Err(TimeOverflow));
    }
}
