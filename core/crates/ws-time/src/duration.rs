//! [`SimDuration`]: signed microseconds.

use core::fmt;

use crate::{MICROS_PER_SECOND, TimeOverflow};

/// A signed span of simulation time in microseconds.
///
/// Arithmetic is checked (returning [`TimeOverflow`]) or saturating; there
/// are no operator impls, so nothing can wrap or panic on overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SimDuration(i64);

impl SimDuration {
    /// Zero length.
    pub const ZERO: Self = Self(0);
    /// The most negative duration.
    pub const MIN: Self = Self(i64::MIN);
    /// The most positive duration.
    pub const MAX: Self = Self(i64::MAX);

    /// A duration of `micros` microseconds.
    #[must_use]
    pub const fn from_micros(micros: i64) -> Self {
        Self(micros)
    }

    /// A duration of `millis` milliseconds.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result does not fit in `i64` microseconds.
    pub const fn from_millis(millis: i64) -> Result<Self, TimeOverflow> {
        match millis.checked_mul(1_000) {
            Some(us) => Ok(Self(us)),
            None => Err(TimeOverflow),
        }
    }

    /// A duration of `secs` seconds.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result does not fit in `i64` microseconds.
    pub const fn from_secs(secs: i64) -> Result<Self, TimeOverflow> {
        match secs.checked_mul(MICROS_PER_SECOND) {
            Some(us) => Ok(Self(us)),
            None => Err(TimeOverflow),
        }
    }

    /// The length in microseconds.
    #[must_use]
    pub const fn as_micros(self) -> i64 {
        self.0
    }

    /// `self + rhs`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result is out of range.
    pub const fn checked_add(self, rhs: Self) -> Result<Self, TimeOverflow> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Ok(Self(v)),
            None => Err(TimeOverflow),
        }
    }

    /// `self - rhs`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result is out of range.
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, TimeOverflow> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Ok(Self(v)),
            None => Err(TimeOverflow),
        }
    }

    /// `self * factor`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the result is out of range.
    pub const fn checked_mul(self, factor: i64) -> Result<Self, TimeOverflow> {
        match self.0.checked_mul(factor) {
            Some(v) => Ok(Self(v)),
            None => Err(TimeOverflow),
        }
    }

    /// `-self`.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] for [`SimDuration::MIN`], whose negation is out of range.
    pub const fn checked_neg(self) -> Result<Self, TimeOverflow> {
        match self.0.checked_neg() {
            Some(v) => Ok(Self(v)),
            None => Err(TimeOverflow),
        }
    }

    /// `self + rhs`, clamped to [`SimDuration::MIN`]..=[`SimDuration::MAX`].
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// `self - rhs`, clamped to [`SimDuration::MIN`]..=[`SimDuration::MAX`].
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// True if the duration is greater than zero.
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }
}

/// Formats as signed decimal seconds with six fractional digits, for
/// example `1.500000s` or `-0.000001s`.
impl fmt::Display for SimDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let per_sec = MICROS_PER_SECOND.unsigned_abs();
        write!(f, "{sign}{}.{:06}s", abs / per_sec, abs % per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn us(v: i64) -> SimDuration {
        SimDuration::from_micros(v)
    }

    #[test]
    fn constructors_scale_units() {
        assert_eq!(SimDuration::from_millis(100), Ok(us(100_000)));
        assert_eq!(SimDuration::from_secs(-2), Ok(us(-2_000_000)));
        assert_eq!(us(7).as_micros(), 7);
    }

    #[test]
    fn constructors_reject_overflow() {
        assert_eq!(SimDuration::from_millis(i64::MAX), Err(TimeOverflow));
        assert_eq!(SimDuration::from_secs(i64::MIN), Err(TimeOverflow));
        assert_eq!(
            SimDuration::from_secs(i64::MAX / MICROS_PER_SECOND),
            Ok(us(i64::MAX / MICROS_PER_SECOND * MICROS_PER_SECOND))
        );
        assert_eq!(
            SimDuration::from_secs(i64::MAX / MICROS_PER_SECOND + 1),
            Err(TimeOverflow)
        );
    }

    #[test]
    fn checked_arithmetic_at_boundaries() {
        assert_eq!(SimDuration::MAX.checked_add(us(1)), Err(TimeOverflow));
        assert_eq!(SimDuration::MAX.checked_add(us(0)), Ok(SimDuration::MAX));
        assert_eq!(SimDuration::MIN.checked_sub(us(1)), Err(TimeOverflow));
        assert_eq!(SimDuration::MIN.checked_add(us(-1)), Err(TimeOverflow));
        assert_eq!(SimDuration::MIN.checked_neg(), Err(TimeOverflow));
        assert_eq!(SimDuration::MAX.checked_neg(), Ok(us(-i64::MAX)));
        assert_eq!(SimDuration::MIN.checked_mul(-1), Err(TimeOverflow));
        assert_eq!(us(i64::MAX / 2 + 1).checked_mul(2), Err(TimeOverflow));
        assert_eq!(us(3).checked_mul(-4), Ok(us(-12)));
        assert_eq!(us(5).checked_sub(us(8)), Ok(us(-3)));
    }

    #[test]
    fn saturating_arithmetic_clamps() {
        assert_eq!(SimDuration::MAX.saturating_add(us(1)), SimDuration::MAX);
        assert_eq!(SimDuration::MIN.saturating_sub(us(1)), SimDuration::MIN);
        assert_eq!(us(2).saturating_sub(us(5)), us(-3));
    }

    #[test]
    fn ordering_follows_micros() {
        assert!(us(-1) < SimDuration::ZERO);
        assert!(SimDuration::MIN < SimDuration::MAX);
        assert!(us(1).is_positive());
        assert!(!SimDuration::ZERO.is_positive());
    }

    #[test]
    fn display_is_signed_decimal_seconds() {
        assert_eq!(SimDuration::ZERO.to_string(), "0.000000s");
        assert_eq!(us(1_500_000).to_string(), "1.500000s");
        assert_eq!(us(-1).to_string(), "-0.000001s");
        assert_eq!(SimDuration::MAX.to_string(), "9223372036854.775807s");
        assert_eq!(SimDuration::MIN.to_string(), "-9223372036854.775808s");
    }
}
