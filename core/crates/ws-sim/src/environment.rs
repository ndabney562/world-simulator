//! Coherent immutable committed-boundary calendar/sky query (ADR0030).
use crate::SimTime;

/// One world boundary, with all environmental values derived from its instant.
/// Callers pair `tick` with the render snapshot under serialized ownership.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalendarSky {
    /// Completed physical tick count.
    pub tick: u64,
    /// Committed uniform-SI simulation instant.
    pub time: SimTime,
    /// Canonical state hash, unchanged by queries.
    pub state_hash: u64,
    /// Committed pacing scale0/1/10/100, excluding pending commands.
    pub scale_tag: u32,
    /// Exact UTC and optional local/sun output in this world's render basis.
    pub environment: ws_world::environment::CalendarSky,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TimeScale, UtcDateTime};
    fn time(y: i32, m: u8, d: u8, h: u8, mi: u8, s: u8) -> SimTime {
        SimTime::from_utc(UtcDateTime::new(y, m, d, h, mi, s, 0).unwrap()).unwrap()
    }
    #[test]
    fn civil_slice_configuration_aligns_once_and_preserves_elapsed_epoch() {
        use crate::slice::{Operation, SliceConfig, SliceSimulation};
        let start = time(2026, 3, 8, 6, 59, 59);
        let c = SliceConfig::calendar_aligned_v1(7, start).unwrap();
        assert_eq!(c.schedule_epoch, time(2026, 3, 2, 5, 0, 0));
        let mut w = SliceSimulation::new(c).unwrap();
        let epoch = w.config().schedule_epoch;
        for op in [
            Operation::SetScale(TimeScale::Paused),
            Operation::HostElapsed(50_000_000),
            Operation::SetScale(TimeScale::X100),
            Operation::HostElapsed(1_000_000),
        ] {
            assert!(w.apply(op).unwrap().error.is_none());
        }
        assert_eq!(w.clock().steps(), 100);
        assert_eq!(w.config().schedule_epoch, epoch);
        let l = ws_time::new_york(w.clock().now().to_utc()).unwrap();
        assert_eq!((l.hour, l.minute, l.second), (3, 0, 9));
        assert_eq!(l.offset_seconds, -14400);
        assert!(SliceConfig::calendar_aligned_v1(7, SimTime::MIN).is_err());
    }
}
