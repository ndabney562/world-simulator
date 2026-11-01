//! Property tests for the clock's host-elapsed accumulator (TP-004
//! acceptance criteria: split invariance and scale proportionality).

use core::num::NonZeroU32;

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use ws_sim::{Clock, SimTime, TimeScale};
use ws_time::{Rate, SimDuration};

/// Fixed seed and no failure-persistence files, so every run checks the same
/// cases (docs/process/testing-policy.md §4: tests are deterministic).
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 2048,
        rng_seed: RngSeed::Fixed(0x5753_5f43_4c4f_434b),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

const STEP_US: i64 = 100_000;
const CAP: u32 = 100;

fn clock(scale: TimeScale) -> Clock {
    let mut c = Clock::new(
        SimTime::EPOCH,
        Rate::new(SimDuration::from_micros(STEP_US)).unwrap(),
        NonZeroU32::new(CAP).unwrap(),
    );
    c.set_scale(scale);
    c
}

fn any_scale() -> impl Strategy<Value = TimeScale> {
    prop_oneof![
        Just(TimeScale::Paused),
        Just(TimeScale::X1),
        Just(TimeScale::X10),
        Just(TimeScale::X100),
    ]
}

/// The largest host time one call may receive at ×100 without the step
/// cap binding, whatever the pending remainder: (CAP - 1) whole steps.
const MAX_CHUNK_US: u64 = (CAP as u64 - 1) * STEP_US as u64 / 100;

proptest! {
    #![proptest_config(config())]

    /// Splitting the same host time differently across calls (each within
    /// the cap) yields the same total steps and the same SimTime.
    #[test]
    fn split_invariance(
        scale in any_scale(),
        chunks in prop::collection::vec(0..=MAX_CHUNK_US, 0..64),
        cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..64),
    ) {
        // Re-split the concatenated host time at arbitrary points.
        let mut a = clock(scale);
        let mut steps_a = 0u64;
        for &h in &chunks {
            steps_a += u64::from(a.advance_by_host_elapsed(h).unwrap());
        }

        let total: u64 = chunks.iter().sum();
        let mut points: Vec<u64> = cuts.iter().map(|i| i.index(usize::try_from(total).unwrap() + 1) as u64).collect();
        points.push(0);
        points.push(total);
        points.sort_unstable();
        points.dedup();
        let mut b = clock(scale);
        let mut steps_b = 0u64;
        for w in points.windows(2) {
            let mut piece = w[1] - w[0];
            // Keep every call within the cap.
            while piece > 0 {
                let h = piece.min(MAX_CHUNK_US);
                steps_b += u64::from(b.advance_by_host_elapsed(h).unwrap());
                piece -= h;
            }
        }

        prop_assert_eq!(steps_a, steps_b);
        prop_assert_eq!(a.now(), b.now());
        prop_assert_eq!(&a, &b);
    }

    /// The same sequence of host elapsed values yields the same sequence of
    /// steps and the same state.
    #[test]
    fn same_inputs_same_steps(
        scale in any_scale(),
        inputs in prop::collection::vec(any::<u64>(), 0..64),
    ) {
        let mut a = clock(scale);
        let mut b = a.clone();
        for &h in &inputs {
            prop_assert_eq!(a.advance_by_host_elapsed(h), b.advance_by_host_elapsed(h));
        }
        prop_assert_eq!(a, b);
    }

    /// Over any sequence within the cap, ×10 takes ten times the simulated
    /// time of ×1, to within one step, and exactly when the host time is a
    /// whole number of steps.
    #[test]
    fn x10_is_ten_times_x1(chunks in prop::collection::vec(0..=MAX_CHUNK_US, 0..64)) {
        let mut x1 = clock(TimeScale::X1);
        let mut x10 = clock(TimeScale::X10);
        let mut paused = clock(TimeScale::Paused);
        for &h in &chunks {
            x1.advance_by_host_elapsed(h).unwrap();
            x10.advance_by_host_elapsed(h).unwrap();
            prop_assert_eq!(paused.advance_by_host_elapsed(h), Ok(0));
        }
        let total: u64 = chunks.iter().sum();
        prop_assert_eq!(x1.steps(), total / STEP_US as u64);
        prop_assert_eq!(x10.steps(), total * 10 / STEP_US as u64);
        if total.is_multiple_of(STEP_US as u64) {
            prop_assert_eq!(x10.steps(), 10 * x1.steps());
        }
        prop_assert_eq!(paused.now(), SimTime::EPOCH);
    }
}
