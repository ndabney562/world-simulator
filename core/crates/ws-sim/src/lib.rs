//! Simulation core of the World Simulator.
//!
//! This is the TP-001 skeleton: a [`World`] created from a [`WorldConfig`]
//! that advances an integer tick counter. Scheduling, commands, the journal,
//! and every simulated system arrive in later Technical Prototype issues.
//!
//! The world never reads the wall clock, the environment, or the filesystem;
//! its state is a pure function of its configuration and the number of steps
//! taken.

/// Parameters a [`World`] is created from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldConfig {
    /// Root seed from which all procedural decisions will be derived.
    pub seed: u64,
}

/// The authoritative simulation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    seed: u64,
    tick: u64,
}

impl World {
    /// Creates a world at tick 0 from `config`.
    #[must_use]
    pub fn new(config: WorldConfig) -> Self {
        Self {
            seed: config.seed,
            tick: 0,
        }
    }

    /// Advances the world by one tick.
    ///
    /// # Panics
    ///
    /// Panics if the tick counter would overflow `u64` (overflow checks are
    /// enabled in every profile). At one tick per millisecond that takes
    /// over 500 million years.
    pub fn step(&mut self) {
        self.tick += 1;
    }

    /// Number of ticks taken since the world was created.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
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
        World::new(WorldConfig { seed })
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
        w.step();
        assert_eq!(w.tick(), 1);
        w.step();
        assert_eq!(w.tick(), 2);
    }

    #[test]
    fn step_does_not_change_seed() {
        let mut w = world(99);
        for _ in 0..10 {
            w.step();
        }
        assert_eq!(w.seed(), 99);
        assert_eq!(w.tick(), 10);
    }

    #[test]
    fn same_seed_worlds_are_equal_after_same_steps() {
        let mut a = world(12345);
        let mut b = world(12345);
        for _ in 0..1000 {
            a.step();
            b.step();
        }
        assert_eq!(a, b);
    }

    #[test]
    fn worlds_differ_by_seed_or_tick() {
        let mut a = world(1);
        let b = world(2);
        assert_ne!(a, b);
        let c = world(1);
        a.step();
        assert_ne!(a, c);
    }
}
