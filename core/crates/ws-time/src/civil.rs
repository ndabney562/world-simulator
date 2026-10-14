//! UTC calendar labels: proleptic Gregorian, astronomical year numbering.

use core::fmt;

use crate::leap::{EPOCH_SECOND_OF_DAY, SECS_PER_DAY, is_leap_date};
use crate::{CivilError, MICROS_PER_SECOND};

/// Days from 1970-01-01 to 2000-01-01.
const DAYS_1970_TO_2000: i64 = 10_957;

/// Days since 2000-01-01 of the proleptic Gregorian date `y-m-d`.
///
/// Howard Hinnant's `days_from_civil`, integer only. Exact for any `y`
/// whose result fits in `i64` (every `i32` year does).
pub(crate) const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // March = 0
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468 - DAYS_1970_TO_2000
}

/// Proleptic Gregorian `(year, month, day)` of `days` since 2000-01-01.
/// Inverse of [`days_from_civil`].
pub(crate) const fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + DAYS_1970_TO_2000 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m as u8, d as u8)
}

const fn is_leap_year(y: i64) -> bool {
    y.rem_euclid(4) == 0 && (y.rem_euclid(100) != 0 || y.rem_euclid(400) == 0)
}

const fn days_in_month(y: i64, m: u8) -> u8 {
    match m {
        2 if is_leap_year(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A validated UTC date and time, to the microsecond.
///
/// Dates are proleptic Gregorian with astronomical year numbering (year 0 is
/// 1 BC). `second` is 60 only at a leap second in the embedded table
/// ([ADR 0014] §4). Converted to and from simulation time with
/// [`SimTime::from_utc`](crate::SimTime::from_utc) and
/// [`SimTime::to_utc`](crate::SimTime::to_utc).
///
/// Ordering is chronological.
///
/// [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcDateTime {
    // Field order gives chronological derived ordering.
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    microsecond: u32,
}

impl UtcDateTime {
    /// Validates and builds a UTC label.
    ///
    /// # Errors
    ///
    /// A [`CivilError`] naming the first invalid field, or
    /// [`CivilError::NotALeapSecond`] for second 60 anywhere other than
    /// `23:59:60` on a tabulated leap-second date.
    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        microsecond: u32,
    ) -> Result<Self, CivilError> {
        if !(1..=12).contains(&month) {
            return Err(CivilError::Month);
        }
        if day == 0 || day > days_in_month(i64::from(year), month) {
            return Err(CivilError::Day);
        }
        if hour > 23 {
            return Err(CivilError::Hour);
        }
        if minute > 59 {
            return Err(CivilError::Minute);
        }
        if second > 60 {
            return Err(CivilError::Second);
        }
        if microsecond >= MICROS_PER_SECOND.unsigned_abs() as u32 {
            return Err(CivilError::Microsecond);
        }
        let dt = Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond,
        };
        if second == 60 && !(hour == 23 && minute == 59 && is_leap_date(dt.days())) {
            return Err(CivilError::NotALeapSecond);
        }
        Ok(dt)
    }

    /// Year (astronomical numbering).
    #[must_use]
    pub const fn year(&self) -> i32 {
        self.year
    }

    /// Month, 1..=12.
    #[must_use]
    pub const fn month(&self) -> u8 {
        self.month
    }

    /// Day of the month, from 1.
    #[must_use]
    pub const fn day(&self) -> u8 {
        self.day
    }

    /// Hour, 0..=23.
    #[must_use]
    pub const fn hour(&self) -> u8 {
        self.hour
    }

    /// Minute, 0..=59.
    #[must_use]
    pub const fn minute(&self) -> u8 {
        self.minute
    }

    /// Second, 0..=60 (60 only during a leap second).
    #[must_use]
    pub const fn second(&self) -> u8 {
        self.second
    }

    /// Microsecond within the second, 0..=999_999.
    #[must_use]
    pub const fn microsecond(&self) -> u32 {
        self.microsecond
    }

    /// True during an inserted leap second (`23:59:60`).
    #[must_use]
    pub const fn is_leap_second(&self) -> bool {
        self.second == 60
    }

    /// Days since 2000-01-01.
    pub(crate) const fn days(&self) -> i64 {
        days_from_civil(self.year as i64, self.month as i64, self.day as i64)
    }

    /// Naive seconds since the epoch (every day 86,400 s); a leap second
    /// reads as the `23:59:59` before it. Within ±2^57 for any `i32` year.
    pub(crate) const fn naive_seconds(&self) -> i64 {
        let second = if self.second == 60 { 59 } else { self.second };
        self.days() * SECS_PER_DAY
            + self.hour as i64 * 3_600
            + self.minute as i64 * 60
            + second as i64
            - EPOCH_SECOND_OF_DAY
    }

    /// Builds a label from naive seconds and microseconds, already known to
    /// be valid (`leap` only at a tabulated leap second).
    pub(crate) const fn from_naive(naive: i64, leap: bool, microsecond: u32) -> Self {
        let since_midnight = naive + EPOCH_SECOND_OF_DAY;
        let days = since_midnight.div_euclid(SECS_PER_DAY);
        let sod = since_midnight.rem_euclid(SECS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        Self {
            // Callers pass naive seconds from an i64 microsecond SimTime,
            // which spans about ±292,277 years, so the year fits in i32.
            year: year as i32,
            month,
            day,
            hour: (sod / 3_600) as u8,
            minute: (sod / 60 % 60) as u8,
            second: if leap { 60 } else { (sod % 60) as u8 },
            microsecond,
        }
    }
}

/// ISO 8601 / RFC 3339 with microseconds and a `Z` suffix, for example
/// `2016-12-31T23:59:60.000000Z`. Years outside 0000..=9999 carry a sign
/// (`-0001`, `+10000`).
impl fmt::Display for UtcDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if (0..=9999).contains(&self.year) {
            write!(f, "{:04}", self.year)?;
        } else {
            write!(f, "{:+05}", self.year)?;
        }
        write!(
            f,
            "-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
            self.month, self.day, self.hour, self.minute, self.second, self.microsecond
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> Result<UtcDateTime, CivilError> {
        UtcDateTime::new(y, mo, d, h, mi, s, 0)
    }

    #[test]
    fn day_numbers_of_known_dates() {
        assert_eq!(days_from_civil(2000, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 1), -10_957);
        assert_eq!(days_from_civil(2000, 3, 1), 60);
        assert_eq!(days_from_civil(2017, 1, 1), 6_210);
        assert_eq!(days_from_civil(0, 3, 1), 60 - 5 * 146_097);
        assert_eq!(civil_from_days(0), (2000, 1, 1));
        assert_eq!(civil_from_days(59), (2000, 2, 29));
        assert_eq!(civil_from_days(-1), (1999, 12, 31));
    }

    #[test]
    fn day_numbers_round_trip_over_many_eras() {
        for days in (-2_000_000..2_000_000).step_by(97) {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, i64::from(m), i64::from(d)), days);
        }
    }

    #[test]
    fn gregorian_leap_years() {
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(is_leap_year(0));
        assert!(is_leap_year(-4));
        assert!(!is_leap_year(-100));
    }

    #[test]
    fn validation_rejects_each_bad_field() {
        assert_eq!(dt(2023, 0, 1, 0, 0, 0), Err(CivilError::Month));
        assert_eq!(dt(2023, 13, 1, 0, 0, 0), Err(CivilError::Month));
        assert_eq!(dt(2023, 1, 0, 0, 0, 0), Err(CivilError::Day));
        assert_eq!(dt(2023, 2, 29, 0, 0, 0), Err(CivilError::Day));
        assert_eq!(dt(1900, 2, 29, 0, 0, 0), Err(CivilError::Day));
        assert_eq!(dt(2023, 4, 31, 0, 0, 0), Err(CivilError::Day));
        assert_eq!(dt(2023, 1, 1, 24, 0, 0), Err(CivilError::Hour));
        assert_eq!(dt(2023, 1, 1, 0, 60, 0), Err(CivilError::Minute));
        assert_eq!(dt(2023, 1, 1, 0, 0, 61), Err(CivilError::Second));
        assert_eq!(
            UtcDateTime::new(2023, 1, 1, 0, 0, 0, 1_000_000),
            Err(CivilError::Microsecond)
        );
        assert!(dt(2024, 2, 29, 23, 59, 59).is_ok());
        assert!(dt(2000, 2, 29, 0, 0, 0).is_ok());
    }

    #[test]
    fn second_sixty_only_at_tabulated_leap_seconds() {
        assert!(dt(2016, 12, 31, 23, 59, 60).is_ok());
        assert!(dt(2015, 6, 30, 23, 59, 60).is_ok());
        assert!(dt(1972, 6, 30, 23, 59, 60).is_ok());
        assert_eq!(
            dt(2017, 12, 31, 23, 59, 60),
            Err(CivilError::NotALeapSecond)
        );
        assert_eq!(
            dt(2016, 12, 31, 23, 58, 60),
            Err(CivilError::NotALeapSecond)
        );
        assert_eq!(
            dt(2016, 12, 31, 22, 59, 60),
            Err(CivilError::NotALeapSecond)
        );
        assert_eq!(
            dt(2016, 12, 30, 23, 59, 60),
            Err(CivilError::NotALeapSecond)
        );
    }

    #[test]
    fn display_is_iso_8601() {
        let t = UtcDateTime::new(2016, 12, 31, 23, 59, 60, 5).unwrap();
        assert_eq!(t.to_string(), "2016-12-31T23:59:60.000005Z");
        assert_eq!(
            dt(-1, 1, 1, 0, 0, 0).unwrap().to_string(),
            "-0001-01-01T00:00:00.000000Z"
        );
        assert_eq!(
            dt(0, 1, 1, 0, 0, 0).unwrap().to_string(),
            "0000-01-01T00:00:00.000000Z"
        );
        assert_eq!(
            dt(10_000, 1, 1, 0, 0, 0).unwrap().to_string(),
            "+10000-01-01T00:00:00.000000Z"
        );
    }

    #[test]
    fn ordering_is_chronological() {
        let a = dt(2016, 12, 31, 23, 59, 59).unwrap();
        let b = dt(2016, 12, 31, 23, 59, 60).unwrap();
        let c = dt(2017, 1, 1, 0, 0, 0).unwrap();
        assert!(a < b && b < c);
        assert!(dt(-5, 6, 1, 0, 0, 0).unwrap() < dt(-4, 1, 1, 0, 0, 0).unwrap());
    }
}
