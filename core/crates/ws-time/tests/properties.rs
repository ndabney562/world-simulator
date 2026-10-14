//! Property tests for ws-time (docs/process/testing-policy.md §1).

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use ws_time::{Rate, SimDuration, SimTime, TimeOverflow, UtcDateTime};

/// Years whose every instant is inside the `SimTime` range.
const SAFE_YEARS: core::ops::RangeInclusive<i32> = -290_000..=294_000;

fn any_time() -> impl Strategy<Value = SimTime> {
    any::<i64>().prop_map(SimTime::from_micros_since_epoch)
}

fn any_duration() -> impl Strategy<Value = SimDuration> {
    any::<i64>().prop_map(SimDuration::from_micros)
}

/// Any valid label in the safe years (second 0..=59; leap seconds are
/// covered exhaustively in the golden and unit tests).
fn any_utc() -> impl Strategy<Value = UtcDateTime> {
    (
        SAFE_YEARS,
        1u8..=12,
        1u8..=31,
        0u8..=23,
        0u8..=59,
        0u8..=59,
        0u32..1_000_000,
    )
        .prop_filter_map("day outside month", |(y, mo, d, h, mi, s, us)| {
            UtcDateTime::new(y, mo, d, h, mi, s, us).ok()
        })
}

/// Fixed seed and no failure-persistence files, so every run checks the same
/// cases (docs/process/testing-policy.md §4: tests are deterministic).
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 4096,
        rng_seed: RngSeed::Fixed(0x5753_5f54_494d_4500),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn sim_time_utc_round_trip_over_the_whole_range(t in any_time()) {
        prop_assert_eq!(SimTime::from_utc(t.to_utc()), Ok(t));
    }

    #[test]
    fn utc_sim_time_round_trip(utc in any_utc()) {
        let t = SimTime::from_utc(utc).unwrap();
        prop_assert_eq!(t.to_utc(), utc);
    }

    #[test]
    fn to_utc_preserves_order(a in any_time(), b in any_time()) {
        prop_assert_eq!(a.cmp(&b), a.to_utc().cmp(&b.to_utc()));
    }

    #[test]
    fn add_then_sub_is_identity(a in any_time(), d in any_duration()) {
        if let Ok(b) = a.checked_add(d) {
            prop_assert_eq!(b.checked_sub(d), Ok(a));
            prop_assert_eq!(b.checked_duration_since(a), Ok(d));
        }
    }

    #[test]
    fn checked_add_fails_exactly_when_i64_overflows(a in any_time(), d in any_duration()) {
        let expected = a
            .as_micros_since_epoch()
            .checked_add(d.as_micros())
            .map(SimTime::from_micros_since_epoch)
            .ok_or(TimeOverflow);
        prop_assert_eq!(a.checked_add(d), expected);
        prop_assert_eq!(a.saturating_add(d), expected.unwrap_or(if d.is_positive() { SimTime::MAX } else { SimTime::MIN }));
    }

    #[test]
    fn duration_add_then_sub_is_identity(a in any_duration(), b in any_duration()) {
        if let Ok(c) = a.checked_add(b) {
            prop_assert_eq!(c.checked_sub(b), Ok(a));
        }
    }

    #[test]
    fn tick_index_brackets_the_instant(t in any_time(), step in 1i64..=i64::MAX) {
        let rate = Rate::new(SimDuration::from_micros(step)).unwrap();
        let k = rate.tick_index(t);
        // start(k) <= t < start(k + 1), checked in i128 to avoid overflow.
        let start = i128::from(k) * i128::from(step);
        let t = i128::from(t.as_micros_since_epoch());
        prop_assert!(start <= t && t < start + i128::from(step));
    }
}
