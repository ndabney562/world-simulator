//! Simulation core of the World Simulator.
//!
//! A [`World`] is created from a [`WorldConfig`] and advanced in fixed base
//! steps by its [`Clock`] (TP-004). Each step runs one toy-world tick
//! ([`ToyWorld::tick`]): residency reconciliation, the L1 movement system
//! (TP-006), and the L2 event drain (TP-007). Simulation-LOD transitions
//! are requested with [`World::flush_commands`] between steps ([ADR 0017]).
//! Commands and history follow ADR 0019; read-only logs never drive domain logic.
//!
//! The world never reads the wall clock, the environment, or the filesystem;
//! its state is a pure function of its configuration and the sequence of
//! calls made on it. Host-measured elapsed time and the start time are
//! inputs ([ADR 0014] §7).
//!
//! [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md
//! [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md

mod clock;
pub mod commands;
mod control;
mod restore;
pub mod slice;
use commands::{Command, Outcome};
pub use control::{ControlError, FlushReport, WhyIll};
pub use ws_events;
use ws_events::{EventError, EventStore};

use core::fmt;

pub use clock::{Clock, InvalidClock, TimeScale};
pub use restore::{RestoreError, WorldParts};
pub use ws_ecs::EntityId;
pub use ws_time::{SimTime, TimeOverflow};
use ws_world::{Dataset, WalkParams};
pub use ws_world::{
    Illness, IllnessKind, Lod, LodError, LodViolation, Population, RenderInstance, RenderKind,
    TOY_DATASET_VERSION, ToyWorld, TransitionReport, WorldError,
};

/// Parameters a [`World`] is created from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldConfig {
    /// Root seed from which all procedural decisions will be derived.
    pub seed: u64,
    /// Simulation time the world starts at. Supplied by the host (for
    /// example the real date when the player starts a world, or a chosen
    /// date); never read from the device clock.
    ///
    /// It must be a tick boundary of the base step (a whole number of
    /// 100 ms steps from the epoch, ADR 0017 §8), so every clock step and
    /// every L2 event time share the epoch-aligned grid; [`World::new`]
    /// rejects any other start rather than rounding it. A host that wants
    /// the boundary at or before an instant `t` uses
    /// `Rate::tick_start(Rate::tick_index(t))`.
    pub start: SimTime,
    /// Residents per toy building ([`Population::TOY`] by default: a few
    /// dozen agents in all). A generation input like the seed.
    pub population: Population,
}

impl WorldConfig {
    /// A configuration with the toy population.
    #[must_use]
    pub const fn new(seed: u64, start: SimTime) -> Self {
        Self {
            seed,
            start,
            population: Population::TOY,
        }
    }
}

/// Why a world could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateError {
    /// `WorldConfig::start` is not a tick boundary of the base step
    /// (ADR 0017 §8). It is never rounded.
    UnalignedStart(SimTime),
    /// Generating the toy world failed.
    World(WorldError),
    /// History reservation or record validation failed.
    History(EventError),
}

impl fmt::Display for CreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnalignedStart(t) => write!(f, "start {t} is not a base-step boundary"),
            Self::World(e) => write!(f, "{e}"),
            Self::History(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CreateError {}

impl From<WorldError> for CreateError {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}

/// Why a world could not be stepped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepError {
    /// The step would pass [`SimTime::MAX`].
    Time(TimeOverflow),
    /// A system failed (never expected; see [`WorldError`]).
    World(WorldError),
    /// Boundary command processing failed.
    Control(ControlError),
    /// Tick history could not be committed.
    History(EventError),
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(e) => write!(f, "{e}"),
            Self::World(e) => write!(f, "{e}"),
            Self::Control(e) => write!(f, "{e}"),
            Self::History(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StepError {}

impl From<TimeOverflow> for StepError {
    fn from(e: TimeOverflow) -> Self {
        Self::Time(e)
    }
}

impl From<WorldError> for StepError {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}

/// The authoritative simulation state. Hosts have no direct mutation escape.
///
/// ```compile_fail
/// use ws_sim::{World,WorldConfig,SimTime,TimeScale};
/// let mut world=World::new(WorldConfig::new(1,SimTime::EPOCH)).unwrap();
/// world.set_time_scale(TimeScale::Paused);
/// ```
/// ```compile_fail
/// use ws_sim::{World,WorldConfig,SimTime,Lod};
/// let world=World::new(WorldConfig::new(1,SimTime::EPOCH)).unwrap();
/// world.toy().set_lod(world.now(),world.clock().base_step(),&[]);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    seed: u64,
    clock: Clock,
    toy: ToyWorld,
    events: EventStore,
    pending: std::collections::VecDeque<(u64, Command)>,
    pending_bytes: usize,
    next_sequence: u64,
    boundary: u64,
    journal: Vec<Outcome>,
    /// Interventions accepted for a later time, by journal sequence.
    scheduled: std::collections::BTreeMap<u64, commands::Intervention>,
    #[cfg(test)]
    fail_after: Option<usize>,
    #[cfg(test)]
    fail_history: bool,
}

impl World {
    /// Creates a world at tick 0 and time `config.start`, with a default
    /// [`Clock`] (100 ms base step, scale ×1) and the toy world generated
    /// from `config.seed` and the built-in toy dataset, every building at
    /// [`Lod::L1`].
    ///
    /// # Errors
    ///
    /// [`CreateError::UnalignedStart`] if `config.start` is not a base-step
    /// boundary, and [`CreateError::World`] with [`WorldError::Duplicate`]
    /// if two procedural IDs collide (a collision is never resolved
    /// silently, ADR 0016).
    pub fn new(config: WorldConfig) -> Result<Self, CreateError> {
        let clock = Clock::with_defaults(config.start);
        if !clock.base_step().is_tick_start(config.start) {
            return Err(CreateError::UnalignedStart(config.start));
        }
        let dataset = Dataset::toy().map_err(WorldError::from)?;
        let toy = ToyWorld::generate(
            config.seed,
            &dataset,
            config.population,
            WalkParams::DEFAULT,
        )?;
        let known = toy
            .buildings()
            .ids()
            .iter()
            .chain(toy.residents().ids())
            .copied()
            .collect();
        let mut events = EventStore::new(known);
        let created = ws_events::Candidate::new(
            config.start,
            0,
            0,
            ws_events::Payload::WorldCreated {
                seed: config.seed,
                dataset: toy.dataset_version(),
                generator: 1,
                rules: 1,
                min: config.population.min_per_building,
                max: config.population.max_per_building,
            },
            ws_events::Origin::System(0),
        );
        events
            .prepare(0, &[created])
            .map_err(CreateError::History)?
            .commit();
        Ok(Self {
            seed: config.seed,
            clock,
            toy,
            events,
            pending: Default::default(),
            pending_bytes: 0,
            next_sequence: 1,
            boundary: 0,
            journal: Vec::new(),
            scheduled: Default::default(),
            #[cfg(test)]
            fail_after: None,
            #[cfg(test)]
            fail_history: false,
        })
    }

    /// Advances the world by one tick: the toy world's tick from the
    /// current boundary (reconcile residency, L1 movement, L2 events up to
    /// the next boundary; ADR 0017 §8), then one base step of simulation
    /// time.
    ///
    /// # Errors
    ///
    /// [`StepError::Time`] if the step would pass [`SimTime::MAX`] (about
    /// AD 294,247), and [`StepError::World`] if the tick fails; the physical tick is
    /// unchanged. Commands committed by the preceding flush remain applied.
    pub fn step(&mut self) -> Result<(), StepError> {
        self.flush_commands().map_err(StepError::Control)?;
        self.physical_step()
    }

    /// Checks every LOD invariant of the toy world at the current boundary
    /// ([`ToyWorld::check_lod_invariants`]). Residency is checked only when
    /// `reconciled` (directly after [`World::flush_commands`]).
    ///
    /// # Errors
    ///
    /// Every violation found.
    pub fn check_lod_invariants(&self, reconciled: bool) -> Result<(), Vec<LodViolation>> {
        self.toy
            .check_lod_invariants(self.clock.now(), self.clock.base_step(), reconciled)
    }

    /// Advances the world by the whole base steps that `host_elapsed_us`
    /// microseconds of host-measured real time amount to at the current
    /// [`TimeScale`], running one toy-world tick per step, and returns how
    /// many ticks were taken. See
    /// [`Clock::advance_by_host_elapsed`] for the accumulator and the
    /// per-call cap.
    ///
    /// # Errors
    ///
    /// [`StepError::Time`] if the steps would pass [`SimTime::MAX`]; the
    /// physical tick is unchanged; committed boundary commands remain applied. [`StepError::World`] if a tick fails
    /// (never expected); the ticks before the failing one stay applied, the
    /// clock counts exactly those, and the pending remainder is unchanged.
    pub fn advance_by_host_elapsed(&mut self, host_elapsed_us: u64) -> Result<u32, StepError> {
        self.flush_commands().map_err(StepError::Control)?;
        let mut advanced = self.clock.clone();
        let steps = advanced.advance_by_host_elapsed(host_elapsed_us)?;
        for _ in 0..steps {
            self.physical_step()?;
        }
        self.clock = advanced;
        Ok(steps)
    }

    /// Hash of the canonical state: the seed, the tick, the time, and the
    /// toy world's [`ToyWorld::state_hash`]. Tier 2: stable within one
    /// build and platform; per-tick hashes are what replay tests compare.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        ws_rng::hash(
            0,
            ws_rng::Purpose::new("ws-sim/state-hash").value(),
            &[
                self.seed,
                self.clock.steps(),
                self.clock.now().as_micros_since_epoch() as u64,
                self.toy.state_hash(),
                self.clock.scale().multiplier() as u64,
            ],
        )
    }

    /// The toy world.
    #[must_use]
    pub const fn toy(&self) -> &ToyWorld {
        &self.toy
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

    /// How many records [`World::render_instances`] yields right now.
    #[must_use]
    pub fn render_instance_count(&self) -> usize {
        self.toy.render_instance_count()
    }

    /// The render snapshot's records, buildings then L1 residents, each in
    /// ascending ID order (ADR 0022). Read-only.
    pub fn render_instances(&self, f: impl FnMut(RenderInstance)) {
        self.toy.render_instances(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(seed: u64) -> World {
        World::new(WorldConfig::new(seed, SimTime::EPOCH)).unwrap()
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
        let w = World::new(WorldConfig::new(1, start)).unwrap();
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
        w.submit(vec![Command::new(
            commands::IssuedBy::Test,
            commands::Action::SetTimeScale(TimeScale::Paused),
        )])
        .unwrap();
        assert_eq!(w.advance_by_host_elapsed(1_000_000), Ok(0));
        w.submit(vec![Command::new(
            commands::IssuedBy::Test,
            commands::Action::SetTimeScale(TimeScale::X10),
        )])
        .unwrap();
        assert_eq!(w.advance_by_host_elapsed(1_000_000), Ok(100));
        assert_eq!(w.tick(), 102);
    }

    #[test]
    fn time_is_start_plus_ticks_times_step() {
        // A start before the epoch, on the grid (ADR 0017 §8).
        let start = SimTime::from_micros_since_epoch(-700_000);
        let mut w = World::new(WorldConfig::new(3, start)).unwrap();
        for host in [0, 33_333, 16_667, 250_000, 1, 99_999] {
            w.advance_by_host_elapsed(host).unwrap();
            w.step().unwrap();
            let expected = -700_000 + i64::try_from(w.tick()).unwrap() * 100_000;
            assert_eq!(w.now().as_micros_since_epoch(), expected);
            assert!(w.clock().base_step().is_tick_start(w.now()));
        }
    }

    #[test]
    fn unaligned_starts_are_rejected_not_rounded() {
        // TP-004's old test started at -7 µs; that start is now an error.
        for us in [-7, 1, -1, 99_999, 100_001, -100_001, 845_208_005_000_001] {
            let start = SimTime::from_micros_since_epoch(us);
            assert_eq!(
                World::new(WorldConfig::new(3, start)),
                Err(CreateError::UnalignedStart(start)),
                "{us}"
            );
        }
    }

    #[test]
    fn aligned_starts_are_accepted() {
        let rate = Clock::with_defaults(SimTime::EPOCH).base_step();
        for us in [
            0,
            100_000,
            -100_000,
            845_208_005_000_000,
            -845_208_005_000_000,
            rate.tick_start(rate.tick_index(SimTime::MAX))
                .unwrap()
                .as_micros_since_epoch(),
            rate.tick_start(rate.tick_index(SimTime::MIN) + 1)
                .unwrap()
                .as_micros_since_epoch(),
        ] {
            let start = SimTime::from_micros_since_epoch(us);
            let w = World::new(WorldConfig::new(3, start)).unwrap();
            assert_eq!(w.now(), start, "{us}");
        }
    }

    #[test]
    fn step_at_the_end_of_time_is_an_error_not_a_wrap() {
        let rate = Clock::with_defaults(SimTime::EPOCH).base_step();
        let start = rate.tick_start(rate.tick_index(SimTime::MAX)).unwrap();
        let mut w = World::new(WorldConfig::new(0, start)).unwrap();
        assert_eq!(w.step(), Err(StepError::Time(TimeOverflow)));
        assert_eq!(w.tick(), 0);
        assert_eq!(w.now(), start);
    }
}

#[cfg(test)]
mod control_tests;
