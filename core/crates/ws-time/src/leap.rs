//! The leap-second table and the mapping between the uniform scale and UTC
//! labels ([ADR 0014] §4).
//!
//! Two second counts relative to the epoch (2000-01-01T12:00:00 UTC):
//!
//! - **uniform** seconds `s`: what [`SimTime`](crate::SimTime) counts; every
//!   second, including an inserted leap second, is one second.
//! - **naive** seconds `n`: the UTC label read as if every day had 86,400
//!   seconds (so `23:59:60` has no naive value of its own).
//!
//! `s = n + (leap seconds inserted between the epoch and n)`, with the
//! inserted second itself at `s_i` between `23:59:59` and the next midnight.
//!
//! [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md

use crate::civil::days_from_civil;

/// Version of the embedded leap-second table. Bumped whenever an entry is
/// added; UTC labels after the new entry change, `SimTime` values do not.
pub const LEAP_SECOND_TABLE_VERSION: u32 = 1;

/// Seconds per day.
pub(crate) const SECS_PER_DAY: i64 = 86_400;
/// The epoch is at noon, so naive seconds are offset by half a day from
/// midnight-based day numbers.
pub(crate) const EPOCH_SECOND_OF_DAY: i64 = 43_200;

/// UTC dates (year, month, day) at whose end IERS inserted a positive leap
/// second (`23:59:60`). TAI − UTC was 10 s from 1972-01-01 and grows by one
/// at each entry (37 s since 2017-01-01). Source: IERS Bulletin C / the IERS
/// `Leap_Second.dat` file. Must stay sorted and strictly increasing.
const LEAP_DATES: [(i32, u8, u8); 27] = [
    (1972, 6, 30),
    (1972, 12, 31),
    (1973, 12, 31),
    (1974, 12, 31),
    (1975, 12, 31),
    (1976, 12, 31),
    (1977, 12, 31),
    (1978, 12, 31),
    (1979, 12, 31),
    (1981, 6, 30),
    (1982, 6, 30),
    (1983, 6, 30),
    (1985, 6, 30),
    (1987, 12, 31),
    (1989, 12, 31),
    (1990, 12, 31),
    (1992, 6, 30),
    (1993, 6, 30),
    (1994, 6, 30),
    (1995, 12, 31),
    (1997, 6, 30),
    (1998, 12, 31),
    (2005, 12, 31),
    (2008, 12, 31),
    (2012, 6, 30),
    (2015, 6, 30),
    (2016, 12, 31),
];

const N: usize = LEAP_DATES.len();

/// Naive seconds of the midnight that ends each leap date (just after its
/// `23:59:60`).
const LEAP_END_NAIVE: [i64; N] = {
    let mut out = [0; N];
    let mut i = 0;
    while i < N {
        let (y, m, d) = LEAP_DATES[i];
        out[i] = (days_from_civil(y as i64, m as i64, d as i64) + 1) * SECS_PER_DAY
            - EPOCH_SECOND_OF_DAY;
        i += 1;
    }
    out
};

/// Leap seconds inserted before the epoch (22: TAI − UTC was 32 s in 2000).
const LEAPS_BEFORE_EPOCH: i64 = {
    let mut count = 0;
    let mut i = 0;
    while i < N {
        if LEAP_END_NAIVE[i] <= 0 {
            count += 1;
        }
        i += 1;
    }
    count
};

/// Uniform second occupied by each inserted leap second (`23:59:60`).
const LEAP_UNIFORM: [i64; N] = {
    let mut out = [0; N];
    let mut i = 0;
    while i < N {
        out[i] = LEAP_END_NAIVE[i] + i as i64 - LEAPS_BEFORE_EPOCH;
        i += 1;
    }
    out
};

/// Uniform seconds for naive seconds `n`. With `leap`, `n` must be the
/// `23:59:59` of a leap date and the result is that date's `23:59:60`.
///
/// `n` is within ±2^57 for every caller, so the small additions cannot
/// overflow.
pub(crate) fn naive_to_uniform(n: i64, leap: bool) -> i64 {
    let before = LEAP_END_NAIVE.partition_point(|&end| end <= n) as i64;
    n + before - LEAPS_BEFORE_EPOCH + i64::from(leap)
}

/// Naive seconds for uniform seconds `s`, and whether `s` is an inserted leap
/// second (in which case the naive value is that date's `23:59:59`).
pub(crate) fn uniform_to_naive(s: i64) -> (i64, bool) {
    let k = LEAP_UNIFORM.partition_point(|&u| u < s);
    if k < N && LEAP_UNIFORM[k] == s {
        (LEAP_END_NAIVE[k] - 1, true)
    } else {
        (s - k as i64 + LEAPS_BEFORE_EPOCH, false)
    }
}

/// True if the day `days` (days since 2000-01-01) ends with a leap second.
pub(crate) fn is_leap_date(days: i64) -> bool {
    // `days` comes from a validated i32 year, so this cannot overflow.
    let end = (days + 1) * SECS_PER_DAY - EPOCH_SECOND_OF_DAY;
    LEAP_END_NAIVE.binary_search(&end).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_strictly_increasing() {
        assert!(LEAP_END_NAIVE.windows(2).all(|w| w[0] < w[1]));
        assert!(LEAP_UNIFORM.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn tai_minus_utc_was_32_seconds_at_the_epoch() {
        // 10 s at 1972-01-01 plus 22 leap seconds before 2000.
        assert_eq!(10 + LEAPS_BEFORE_EPOCH, 32);
        // 37 s after the last entry.
        assert_eq!(10 + N as i64, 37);
    }

    #[test]
    fn naive_and_uniform_agree_at_the_epoch() {
        assert_eq!(naive_to_uniform(0, false), 0);
        assert_eq!(uniform_to_naive(0), (0, false));
    }

    #[test]
    fn every_leap_second_maps_both_ways() {
        for i in 0..N {
            let s = LEAP_UNIFORM[i];
            let (n, leap) = uniform_to_naive(s);
            assert!(leap);
            assert_eq!(n, LEAP_END_NAIVE[i] - 1);
            assert_eq!(naive_to_uniform(n, true), s);
            // The seconds either side are ordinary and adjacent.
            assert_eq!(uniform_to_naive(s - 1), (n, false));
            assert_eq!(uniform_to_naive(s + 1), (n + 1, false));
        }
    }
}
