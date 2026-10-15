//! The simulation clock: current [`SimTime`], fixed base step, time scale,
//! and the integer accumulator that turns host-measured elapsed real time
//! into whole steps ([ADR 0014] §7).
//!
//! The clock never reads the wall clock. The host measures elapsed real time
//! and passes it in as microseconds.
//!
//! [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md

use core::num::NonZeroU32;

use ws_time::{Rate, SimDuration, SimTime, TimeOverflow};

/// How fast simulation time runs relative to host-measured real time.
///
/// Changing the scale is a plain method call for now; TP-008 turns it into a
/// journaled command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TimeScale {
    /// No simulation time passes for host time.
    Paused,
    /// One simulated second per real second.
    #[default]
    X1,
    /// Ten simulated seconds per real second.
    X10,
    /// One hundred simulated seconds per real second.
    X100,
}

impl TimeScale {
    /// Simulated microseconds per host microsecond.
    #[must_use]
    pub const fn multiplier(self) -> u32 {
        match self {
            Self::Paused => 0,
            Self::X1 => 1,
            Self::X10 => 10,
            Self::X100 => 100,
        }
    }
}

/// The clock owned by a [`World`](crate::World).
///
/// Invariant: `now == start + steps * base_step`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clock {
    start: SimTime,
    now: SimTime,
    base_step: Rate,
    steps: u64,
    scale: TimeScale,
    max_steps_per_call: NonZeroU32,
    /// Scaled (simulated) microseconds received from the host but not yet
    /// turned into a whole step. Always less than one base step.
    pending_us: u64,
}

impl Clock {
    /// The default base step: 100 ms of simulation time.
    pub const DEFAULT_BASE_STEP: SimDuration = SimDuration::from_micros(100_000);

    /// The default cap on steps per [`Clock::advance_by_host_elapsed`] call:
    /// 100 steps, 10 s of simulation time at the default step. At ×100 and
    /// 30 frames per second a frame needs about 33 steps, so this allows a
    /// few frames of catch-up before the simulation falls behind.
    pub const DEFAULT_MAX_STEPS_PER_CALL: NonZeroU32 = NonZeroU32::new(100).unwrap();

    /// A clock at `start`, at scale ×1, with no pending time.
    #[must_use]
    pub const fn new(start: SimTime, base_step: Rate, max_steps_per_call: NonZeroU32) -> Self {
        Self {
            start,
            now: start,
            base_step,
            steps: 0,
            scale: TimeScale::X1,
            max_steps_per_call,
            pending_us: 0,
        }
    }

    /// A clock at `start` with [`Clock::DEFAULT_BASE_STEP`] and
    /// [`Clock::DEFAULT_MAX_STEPS_PER_CALL`].
    #[must_use]
    pub const fn with_defaults(start: SimTime) -> Self {
        match Rate::new(Self::DEFAULT_BASE_STEP) {
            Ok(rate) => Self::new(start, rate, Self::DEFAULT_MAX_STEPS_PER_CALL),
            Err(_) => unreachable!(), // the default step is positive
        }
    }

    /// The current simulation time.
    #[must_use]
    pub const fn now(&self) -> SimTime {
        self.now
    }

    /// The time the clock started at.
    #[must_use]
    pub const fn start(&self) -> SimTime {
        self.start
    }

    /// The fixed base step.
    #[must_use]
    pub const fn base_step(&self) -> Rate {
        self.base_step
    }

    /// Base steps taken since the clock started.
    #[must_use]
    pub const fn steps(&self) -> u64 {
        self.steps
    }

    /// The current time scale.
    #[must_use]
    pub const fn scale(&self) -> TimeScale {
        self.scale
    }

    /// Sets the time scale. Pending sub-step time is kept (it is already in
    /// simulated microseconds), so pausing and resuming loses nothing.
    pub fn set_scale(&mut self, scale: TimeScale) {
        self.scale = scale;
    }

    /// The cap on steps per [`Clock::advance_by_host_elapsed`] call.
    #[must_use]
    pub const fn max_steps_per_call(&self) -> NonZeroU32 {
        self.max_steps_per_call
    }

    /// Advances by exactly one base step, regardless of scale.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the step would pass [`SimTime::MAX`]; the clock is
    /// then unchanged.
    pub fn step(&mut self) -> Result<(), TimeOverflow> {
        self.now = self.now.checked_add(self.base_step.step())?;
        // steps * base_step <= SimTime::MAX - SimTime::MIN < 2^64, so this
        // cannot overflow.
        self.steps += 1;
        Ok(())
    }

    /// Converts `host_elapsed_us` of host-measured real time into whole base
    /// steps, advances by them, and returns how many were taken.
    ///
    /// The host time is multiplied by the scale and added to the pending
    /// sub-step remainder; every whole base step in that total is taken, up
    /// to [`Clock::max_steps_per_call`]. Whole steps beyond the cap are
    /// dropped (the simulation falls behind rather than spiralling); the
    /// sub-step remainder is always kept. All arithmetic is integer, so the
    /// result depends only on the sequence of inputs, and splitting the same
    /// total host time differently across calls gives the same steps as
    /// long as no call reaches the cap.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the steps would pass [`SimTime::MAX`]; the clock
    /// (including the pending remainder) is then unchanged.
    pub fn advance_by_host_elapsed(&mut self, host_elapsed_us: u64) -> Result<u32, TimeOverflow> {
        // u128 holds u64::MAX * 100 + u64::MAX without overflow.
        let scaled = u128::from(host_elapsed_us) * u128::from(self.scale.multiplier())
            + u128::from(self.pending_us);
        // The step is positive, so the cast is lossless.
        let step_us = self.base_step.step().as_micros() as u128;
        let due = scaled / step_us;
        let steps = due.min(u128::from(self.max_steps_per_call.get())) as u32;
        // The remainder is below the step, which fits in i64.
        let pending_us = (scaled % step_us) as u64;

        let advance = self.base_step.step().checked_mul(i64::from(steps))?;
        self.now = self.now.checked_add(advance)?;
        self.steps += u64::from(steps);
        self.pending_us = pending_us;
        Ok(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> Clock {
        Clock::with_defaults(SimTime::EPOCH)
    }

    fn clock_with(step_us: i64, cap: u32) -> Clock {
        Clock::new(
            SimTime::EPOCH,
            Rate::new(SimDuration::from_micros(step_us)).unwrap(),
            NonZeroU32::new(cap).unwrap(),
        )
    }

    #[test]
    fn defaults_are_documented_values() {
        let c = clock();
        assert_eq!(c.base_step().step().as_micros(), 100_000);
        assert_eq!(c.max_steps_per_call().get(), 100);
        assert_eq!(c.scale(), TimeScale::X1);
        assert_eq!(c.now(), SimTime::EPOCH);
        assert_eq!(c.steps(), 0);
    }

    #[test]
    fn step_advances_by_one_base_step() {
        let mut c = clock();
        c.step().unwrap();
        c.step().unwrap();
        assert_eq!(c.now().as_micros_since_epoch(), 200_000);
        assert_eq!(c.steps(), 2);
    }

    #[test]
    fn step_ignores_the_scale() {
        let mut c = clock();
        c.set_scale(TimeScale::Paused);
        c.step().unwrap();
        assert_eq!(c.steps(), 1);
    }

    #[test]
    fn step_at_the_end_of_time_fails_and_leaves_the_clock_unchanged() {
        let start = SimTime::MAX
            .checked_sub(SimDuration::from_micros(150_000))
            .unwrap();
        let mut c = Clock::with_defaults(start);
        c.step().unwrap();
        let before = c.clone();
        assert_eq!(c.step(), Err(TimeOverflow));
        assert_eq!(c, before);
    }

    #[test]
    fn paused_advances_zero_steps() {
        let mut c = clock();
        c.set_scale(TimeScale::Paused);
        for host in [0, 1, 99_999, 100_000, 10_000_000, u64::MAX] {
            assert_eq!(c.advance_by_host_elapsed(host), Ok(0));
        }
        assert_eq!(c.now(), SimTime::EPOCH);
    }

    #[test]
    fn scales_multiply_steps_for_the_same_host_time() {
        // One real second at a 100 ms step.
        for (scale, expected) in [
            (TimeScale::Paused, 0),
            (TimeScale::X1, 10),
            (TimeScale::X10, 100),
        ] {
            let mut c = clock_with(100_000, 1_000);
            c.set_scale(scale);
            assert_eq!(
                c.advance_by_host_elapsed(1_000_000),
                Ok(expected),
                "{scale:?}"
            );
        }
        let mut c = clock_with(100_000, 1_000);
        c.set_scale(TimeScale::X100);
        assert_eq!(c.advance_by_host_elapsed(1_000_000), Ok(1_000));
    }

    #[test]
    fn sub_step_time_accumulates_across_calls() {
        let mut c = clock();
        assert_eq!(c.advance_by_host_elapsed(60_000), Ok(0));
        assert_eq!(c.advance_by_host_elapsed(60_000), Ok(1));
        assert_eq!(c.advance_by_host_elapsed(80_000), Ok(1));
        assert_eq!(c.now().as_micros_since_epoch(), 200_000);
        assert_eq!(c.steps(), 2);
    }

    #[test]
    fn pending_time_survives_a_pause() {
        let mut c = clock();
        assert_eq!(c.advance_by_host_elapsed(90_000), Ok(0));
        c.set_scale(TimeScale::Paused);
        assert_eq!(c.advance_by_host_elapsed(5_000_000), Ok(0));
        c.set_scale(TimeScale::X1);
        assert_eq!(c.advance_by_host_elapsed(10_000), Ok(1));
    }

    #[test]
    fn steps_beyond_the_cap_are_dropped_but_the_remainder_is_kept() {
        let mut c = clock_with(100_000, 5);
        // 1.25 s is 12 steps and 50 ms; only 5 are taken.
        assert_eq!(c.advance_by_host_elapsed(1_250_000), Ok(5));
        assert_eq!(c.now().as_micros_since_epoch(), 500_000);
        // The 50 ms remainder plus 50 ms completes one step.
        assert_eq!(c.advance_by_host_elapsed(50_000), Ok(1));
    }

    #[test]
    fn huge_host_elapsed_does_not_overflow() {
        let mut c = clock();
        c.set_scale(TimeScale::X100);
        assert_eq!(c.advance_by_host_elapsed(u64::MAX), Ok(100));
        assert_eq!(c.advance_by_host_elapsed(u64::MAX), Ok(100));
    }

    #[test]
    fn advance_at_the_end_of_time_fails_and_leaves_the_clock_unchanged() {
        let start = SimTime::MAX
            .checked_sub(SimDuration::from_micros(250_000))
            .unwrap();
        let mut c = Clock::with_defaults(start);
        assert_eq!(c.advance_by_host_elapsed(150_000), Ok(1));
        let before = c.clone();
        assert_eq!(c.advance_by_host_elapsed(300_000), Err(TimeOverflow));
        assert_eq!(c, before);
        // A smaller request that fits still works.
        assert_eq!(c.advance_by_host_elapsed(50_000), Ok(1));
    }
}
