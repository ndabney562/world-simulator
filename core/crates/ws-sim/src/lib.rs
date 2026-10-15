//! Simulation core of the World Simulator.
//!
//! A [`World`] is created from a [`WorldConfig`] and advanced in fixed base
//! steps by its [`Clock`] (TP-004). Scheduling, commands, the journal, and
//! every simulated system arrive in later Technical Prototype issues.
//!
//! The world never reads the wall clock, the environment, or the filesystem;
//! its state is a pure function of its configuration and the sequence of
//! calls made on it. Host-measured elapsed time and the start time are
//! inputs ([ADR 0014] §7).
//!
//! [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md

mod clock;

pub use clock::{Clock, TimeScale};
pub use ws_time::{SimTime, TimeOverflow};

/// Parameters a [`World`] is created from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldConfig {
    /// Root seed from which all procedural decisions will be derived.
    pub seed: u64,
    /// Simulation time the world starts at. Supplied by the host (for
    /// example the real date when the player starts a world, or a chosen
    /// date); never read from the device clock.
    ///
    /// Any instant is accepted, but [`ws_time::Rate`] ticks are aligned to
    /// the epoch, so a start that is not a whole number of base steps from
    /// it (100 ms by default) puts every clock step off the base step's
    /// `Rate` grid: `world.clock().base_step().is_tick_start(world.now())`
    /// is then never true, and neither is it for any rate whose step is a
    /// multiple of the base step. Hosts should
    /// supply an aligned start until the scheduler (TP-007) decides whether
    /// to require or enforce alignment.
    pub start: SimTime,
}

/// The authoritative simulation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    seed: u64,
    clock: Clock,
}

impl World {
    /// Creates a world at tick 0 and time `config.start`, with a
    /// default [`Clock`] (100 ms base step, scale ×1).
    #[must_use]
    pub fn new(config: WorldConfig) -> Self {
        Self {
            seed: config.seed,
            clock: Clock::with_defaults(config.start),
        }
    }

    /// Advances the world by one tick: one base step of simulation time.
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the step would pass [`SimTime::MAX`] (about
    /// AD 294,247); the world is then unchanged.
    pub fn step(&mut self) -> Result<(), TimeOverflow> {
        self.clock.step()
    }

    /// Advances the world by the whole base steps that `host_elapsed_us`
    /// microseconds of host-measured real time amount to at the current
    /// [`TimeScale`], and returns how many ticks were taken. See
    /// [`Clock::advance_by_host_elapsed`] for the accumulator and the
    /// per-call cap.
    ///
    /// The world has no per-tick systems yet, so this only advances the
    /// clock; when the scheduler arrives (TP-007) each of these ticks runs
    /// like [`World::step`].
    ///
    /// # Errors
    ///
    /// [`TimeOverflow`] if the steps would pass [`SimTime::MAX`]; the world
    /// is then unchanged.
    pub fn advance_by_host_elapsed(&mut self, host_elapsed_us: u64) -> Result<u32, TimeOverflow> {
        self.clock.advance_by_host_elapsed(host_elapsed_us)
    }

    /// Sets the time scale (a plain call until TP-008 journals it).
    pub fn set_time_scale(&mut self, scale: TimeScale) {
        self.clock.set_scale(scale);
    }

    /// Number of ticks taken since the world was created.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.clock.steps()
    }

    /// The current simulation time.
    #[must_use]
    pub fn now(&self) -> SimTime {
        self.clock.now()
    }

    /// The world's clock.
    #[must_use]
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The seed this world was created with.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(seed: u64) -> World {
        World::new(WorldConfig {
            seed,
            start: SimTime::EPOCH,
        })
    }

    #[test]
    fn new_world_starts_at_tick_zero() {
        assert_eq!(world(7).tick(), 0);
    }

    #[test]
    fn new_world_keeps_its_seed() {
        assert_eq!(world(0).seed(), 0);
        assert_eq!(world(42).seed(), 42);
        assert_eq!(world(u64::MAX).seed(), u64::MAX);
    }

    #[test]
    fn step_advances_tick_by_one() {
        let mut w = world(1);
        w.step().unwrap();
        assert_eq!(w.tick(), 1);
        w.step().unwrap();
        assert_eq!(w.tick(), 2);
    }

    #[test]
    fn step_does_not_change_seed() {
        let mut w = world(99);
        for _ in 0..10 {
            w.step().unwrap();
        }
        assert_eq!(w.seed(), 99);
        assert_eq!(w.tick(), 10);
    }

    #[test]
    fn same_seed_worlds_are_equal_after_same_steps() {
        let mut a = world(12345);
        let mut b = world(12345);
        for _ in 0..1000 {
            a.step().unwrap();
            b.step().unwrap();
        }
        assert_eq!(a, b);
    }

    #[test]
    fn worlds_differ_by_seed_or_tick() {
        let mut a = world(1);
        let b = world(2);
        assert_ne!(a, b);
        let c = world(1);
        a.step().unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn world_starts_at_the_configured_time() {
        let start = SimTime::from_micros_since_epoch(845_208_005_000_000); // 2026-10-14T00:00Z
        let w = World::new(WorldConfig { seed: 1, start });
        assert_eq!(w.now(), start);
        assert_eq!(w.clock().start(), start);
        assert_eq!(w.now().to_string(), "2026-10-14T00:00:00.000000Z");
    }

    #[test]
    fn step_advances_time_by_one_base_step() {
        let mut w = world(1);
        w.step().unwrap();
        assert_eq!(w.now().as_micros_since_epoch(), 100_000);
        assert_eq!(w.tick(), 1);
    }

    #[test]
    fn advance_by_host_elapsed_counts_as_ticks() {
        let mut w = world(1);
        assert_eq!(w.advance_by_host_elapsed(250_000), Ok(2));
        assert_eq!(w.tick(), 2);
        assert_eq!(w.now().as_micros_since_epoch(), 200_000);
        w.set_time_scale(TimeScale::Paused);
        assert_eq!(w.advance_by_host_elapsed(1_000_000), Ok(0));
        w.set_time_scale(TimeScale::X10);
        assert_eq!(w.advance_by_host_elapsed(1_000_000), Ok(100));
        assert_eq!(w.tick(), 102);
    }

    #[test]
    fn time_is_start_plus_ticks_times_step() {
        let start = SimTime::from_micros_since_epoch(-7);
        let mut w = World::new(WorldConfig { seed: 3, start });
        for host in [0, 33_333, 16_667, 250_000, 1, 99_999] {
            w.advance_by_host_elapsed(host).unwrap();
            w.step().unwrap();
            let expected = -7 + i64::try_from(w.tick()).unwrap() * 100_000;
            assert_eq!(w.now().as_micros_since_epoch(), expected);
        }
    }

    #[test]
    fn step_at_the_end_of_time_is_an_error_not_a_wrap() {
        let start = SimTime::MAX;
        let mut w = World::new(WorldConfig { seed: 0, start });
        assert_eq!(w.step(), Err(TimeOverflow));
        assert_eq!(w.tick(), 0);
        assert_eq!(w.now(), start);
    }
}
