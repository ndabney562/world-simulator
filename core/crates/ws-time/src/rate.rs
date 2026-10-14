//! [`Rate`]: a system's native step.

use crate::{InvalidStep, SimDuration, SimTime, TimeOverflow};

/// A system's native step: a positive [`SimDuration`].
///
/// Tick `k` of a rate covers `[k * step, (k + 1) * step)` microseconds from
/// the epoch, so ticks are aligned to [`SimTime::EPOCH`] and systems whose
/// steps divide one another share boundaries. Integer division only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rate {
    step: SimDuration,
}

impl Rate {
    /// A rate stepping every `step`.
    ///
    /// # Errors
    ///
    /// [`InvalidStep`] if `step` is zero or negative.
    pub const fn new(step: SimDuration) -> Result<Self, InvalidStep> {
        if step.is_positive() {
            Ok(Self { step })
        } else {
            Err(InvalidStep)
        }
    }

    /// The step, always positive.
    #[must_use]
    pub const fn step(self) -> SimDuration {
        self.step
    }

    /// The index of the tick containing `t`: `floor(t / step)`. Total, and
    /// negative for instants before the epoch.
    #[must_use]
    pub const fn tick_index(self, t: SimTime) -> i64 {
        t.as_micros_since_epoch().div_euclid(self.step.as_micros())
    }

    /// The instant tick `index` starts.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if that instant is out of range.
    pub const fn tick_start(self, index: i64) -> Result<SimTime, TimeOverflow> {
        match index.checked_mul(self.step.as_micros()) {
            Some(us) => Ok(SimTime::from_micros_since_epoch(us)),
            None => Err(TimeOverflow),
        }
    }

    /// True if a tick starts exactly at `t`.
    #[must_use]
    pub const fn is_tick_start(self, t: SimTime) -> bool {
        t.as_micros_since_epoch().rem_euclid(self.step.as_micros()) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(us: i64) -> Rate {
        Rate::new(SimDuration::from_micros(us)).unwrap()
    }
    const fn t(us: i64) -> SimTime {
        SimTime::from_micros_since_epoch(us)
    }

    #[test]
    fn rejects_non_positive_steps() {
        assert_eq!(Rate::new(SimDuration::ZERO), Err(InvalidStep));
        assert_eq!(Rate::new(SimDuration::from_micros(-1)), Err(InvalidStep));
        assert_eq!(Rate::new(SimDuration::MIN), Err(InvalidStep));
        assert!(Rate::new(SimDuration::MAX).is_ok());
    }

    #[test]
    fn tick_index_floors_on_both_sides_of_the_epoch() {
        let r = rate(100_000);
        assert_eq!(r.tick_index(t(0)), 0);
        assert_eq!(r.tick_index(t(99_999)), 0);
        assert_eq!(r.tick_index(t(100_000)), 1);
        assert_eq!(r.tick_index(t(-1)), -1);
        assert_eq!(r.tick_index(t(-100_000)), -1);
        assert_eq!(r.tick_index(t(-100_001)), -2);
    }

    #[test]
    fn tick_index_is_total_at_extremes() {
        assert_eq!(rate(1).tick_index(SimTime::MIN), i64::MIN);
        assert_eq!(rate(1).tick_index(SimTime::MAX), i64::MAX);
        assert_eq!(rate(i64::MAX).tick_index(SimTime::MAX), 1);
        assert_eq!(rate(i64::MAX).tick_index(SimTime::MIN), -2);
    }

    #[test]
    fn tick_start_inverts_tick_index() {
        let r = rate(3_600_000_000);
        for k in [-5_i64, -1, 0, 1, 42] {
            let start = r.tick_start(k).unwrap();
            assert_eq!(r.tick_index(start), k);
            assert!(r.is_tick_start(start));
            assert!(!r.is_tick_start(t(start.as_micros_since_epoch() + 1)));
        }
        assert_eq!(r.tick_start(i64::MAX), Err(TimeOverflow));
        assert_eq!(r.tick_start(i64::MIN), Err(TimeOverflow));
    }

    #[test]
    fn commensurate_rates_share_boundaries() {
        let fine = rate(100_000); // 100 ms
        let coarse = rate(1_000_000); // 1 s
        for k in -20..20 {
            let start = coarse.tick_start(k).unwrap();
            assert!(fine.is_tick_start(start));
            assert_eq!(fine.tick_index(start), k * 10);
        }
    }
}
