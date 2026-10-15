//! Tier 1 golden table: UTC labels and their exact `SimTime` microseconds.
//!
//! The expected values were derived independently of this crate (Python's
//! `datetime` for the naive span from the epoch, plus TAI − UTC from the
//! IERS leap-second list) and must match exactly on every platform. Changing
//! one needs a `LEAP_SECOND_TABLE_VERSION` bump and Product Owner approval
//! (docs/process/testing-policy.md §2).

use ws_time::{LEAP_SECOND_TABLE_VERSION, SimTime, UtcDateTime};

/// (year, month, day, hour, minute, second, microsecond, SimTime µs, label)
type Row = (i32, u8, u8, u8, u8, u8, u32, i64, &'static str);

const GOLDEN: &[Row] = &[
    // The epoch and its neighbourhood.
    (2000, 1, 1, 12, 0, 0, 0, 0, "2000-01-01T12:00:00.000000Z"),
    (
        2000,
        1,
        1,
        0,
        0,
        0,
        0,
        -43_200_000_000,
        "2000-01-01T00:00:00.000000Z",
    ),
    (
        1999,
        12,
        31,
        23,
        59,
        59,
        0,
        -43_201_000_000,
        "1999-12-31T23:59:59.000000Z",
    ),
    (
        2000,
        2,
        29,
        12,
        0,
        0,
        0,
        5_097_600_000_000,
        "2000-02-29T12:00:00.000000Z",
    ),
    // Before the leap-second era: TAI − UTC fixed at 10 s.
    (
        1970,
        1,
        1,
        0,
        0,
        0,
        0,
        -946_728_022_000_000,
        "1970-01-01T00:00:00.000000Z",
    ),
    (
        1900,
        1,
        1,
        0,
        0,
        0,
        0,
        -3_155_716_822_000_000,
        "1900-01-01T00:00:00.000000Z",
    ),
    (
        1,
        1,
        1,
        0,
        0,
        0,
        0,
        -63_082_324_822_000_000,
        "0001-01-01T00:00:00.000000Z",
    ),
    (
        1972,
        1,
        1,
        0,
        0,
        0,
        0,
        -883_656_022_000_000,
        "1972-01-01T00:00:00.000000Z",
    ),
    // The first leap second (1972-06-30).
    (
        1972,
        6,
        30,
        23,
        59,
        59,
        0,
        -867_931_223_000_000,
        "1972-06-30T23:59:59.000000Z",
    ),
    (
        1972,
        6,
        30,
        23,
        59,
        60,
        0,
        -867_931_222_000_000,
        "1972-06-30T23:59:60.000000Z",
    ),
    (
        1972,
        7,
        1,
        0,
        0,
        0,
        0,
        -867_931_221_000_000,
        "1972-07-01T00:00:00.000000Z",
    ),
    // 2015-06-30.
    (
        2015,
        6,
        30,
        23,
        59,
        59,
        0,
        488_980_802_000_000,
        "2015-06-30T23:59:59.000000Z",
    ),
    (
        2015,
        6,
        30,
        23,
        59,
        60,
        0,
        488_980_803_000_000,
        "2015-06-30T23:59:60.000000Z",
    ),
    (
        2015,
        7,
        1,
        0,
        0,
        0,
        0,
        488_980_804_000_000,
        "2015-07-01T00:00:00.000000Z",
    ),
    // The last leap second (2016-12-31).
    (
        2016,
        12,
        31,
        23,
        59,
        59,
        0,
        536_500_803_000_000,
        "2016-12-31T23:59:59.000000Z",
    ),
    (
        2016,
        12,
        31,
        23,
        59,
        60,
        0,
        536_500_804_000_000,
        "2016-12-31T23:59:60.000000Z",
    ),
    (
        2016,
        12,
        31,
        23,
        59,
        60,
        999_999,
        536_500_804_999_999,
        "2016-12-31T23:59:60.999999Z",
    ),
    (
        2017,
        1,
        1,
        0,
        0,
        0,
        0,
        536_500_805_000_000,
        "2017-01-01T00:00:00.000000Z",
    ),
    // After the table: no further leap seconds assumed.
    (
        2026,
        10,
        14,
        0,
        0,
        0,
        0,
        845_208_005_000_000,
        "2026-10-14T00:00:00.000000Z",
    ),
    (
        2038,
        1,
        19,
        3,
        14,
        7,
        0,
        1_200_755_652_000_000,
        "2038-01-19T03:14:07.000000Z",
    ),
    (
        9999,
        12,
        31,
        23,
        59,
        59,
        999_999,
        252_455_572_804_999_999,
        "9999-12-31T23:59:59.999999Z",
    ),
];

#[test]
fn golden_table_version() {
    assert_eq!(LEAP_SECOND_TABLE_VERSION, 1);
}

#[test]
fn golden_utc_to_sim_time() {
    for &(y, mo, d, h, mi, s, us, micros, label) in GOLDEN {
        let utc = UtcDateTime::new(y, mo, d, h, mi, s, us).unwrap();
        assert_eq!(
            SimTime::from_utc(utc).unwrap().as_micros_since_epoch(),
            micros,
            "{label}"
        );
    }
}

#[test]
fn golden_sim_time_to_utc_and_display() {
    for &(y, mo, d, h, mi, s, us, micros, label) in GOLDEN {
        let t = SimTime::from_micros_since_epoch(micros);
        assert_eq!(
            t.to_utc(),
            UtcDateTime::new(y, mo, d, h, mi, s, us).unwrap(),
            "{label}"
        );
        assert_eq!(t.to_string(), label);
    }
}

#[test]
fn durations_across_a_leap_second_include_it() {
    let before = SimTime::from_utc(UtcDateTime::new(2016, 12, 31, 23, 59, 59, 0).unwrap()).unwrap();
    let after = SimTime::from_utc(UtcDateTime::new(2017, 1, 1, 0, 0, 0, 0).unwrap()).unwrap();
    assert_eq!(
        after.checked_duration_since(before).unwrap().as_micros(),
        2_000_000
    );

    // Across a non-leap year end, the same labels are one second apart.
    let before = SimTime::from_utc(UtcDateTime::new(2017, 12, 31, 23, 59, 59, 0).unwrap()).unwrap();
    let after = SimTime::from_utc(UtcDateTime::new(2018, 1, 1, 0, 0, 0, 0).unwrap()).unwrap();
    assert_eq!(
        after.checked_duration_since(before).unwrap().as_micros(),
        1_000_000
    );
}
