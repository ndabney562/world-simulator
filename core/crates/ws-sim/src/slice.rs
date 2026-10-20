//! Real-slice L2 orchestration (ADR 0027), separate from the compatible toy World.
use crate::{Clock, SimTime, TimeScale};
use ws_ecs::EntityId;
use ws_events::{Candidate, EventError, EventStore, Origin, Payload};
use ws_life::behavior::{Behavior, BehaviorError, Detail, Snapshot};
use ws_life::{InvalidPopulation, RULES_VERSION, Schedules};
use ws_rng::{Purpose, SipHasher24};
use ws_world::slice::population::{PopulatedSlice, PopulationError};

/// Pinned baseline and explicit model-week origin. Both times use the 100-ms grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceConfig {
    /// Procedural seed.
    pub seed: u64,
    /// Creation time; no past facts are invented.
    pub start: SimTime,
    /// Monday midnight in the model week; civil calendar alignment is VS-007.
    pub schedule_epoch: SimTime,
}
/// Typed construction, time, reservation or history failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceError {
    /// Cannot generate the reviewed source/population.
    Population(PopulationError),
    /// Required schedule destination is missing.
    Destination(InvalidPopulation),
    /// Behavior/route/detail validation or planning failure.
    Behavior(BehaviorError),
    /// Start or epoch is not on the fixed-step grid.
    Grid,
    /// The attempted clock advance is outside SimTime.
    TimeOverflow,
    /// Event preparation failed before the attempted tick committed.
    History(EventError),
    /// Operation-log reservation failed; operation did not begin or get recorded.
    Resource,
}
impl core::fmt::Display for SliceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "slice simulation error: {self:?}")
    }
}
impl std::error::Error for SliceError {}
/// Structured Rust-side operations; no new wire/debug-file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Explicit physical steps, independent of scale. Large calls preserve a committed prefix on failure.
    Steps(u64),
    /// Serialized scale change, retaining pending sub-step host time.
    SetScale(TimeScale),
    /// Host elapsed microseconds, using the existing cap/drop/remainder contract.
    HostElapsed(u64),
    /// Manual Rust-side fine/coarse request; camera/node policy remains VS-006.
    SetDetail {
        /// Existing resident identity.
        resident: EntityId,
        /// Requested implemented detail level.
        level: Detail,
    },
}
/// Result of an operation after operation-log reservation succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// Physical ticks that committed, including a batch prefix on failure.
    pub committed: u64,
    /// Failure of the first uncommitted tick or whole-budget time preflight.
    pub error: Option<SliceError>,
}
/// A deterministic environmental-stop checkpoint, outside canonical state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkpoint {
    /// Tick reached.
    pub tick: u64,
    /// Canonical simulation hash.
    pub state: u64,
    /// Factual history digest.
    pub events: u64,
    /// Host pacing remainder, compared separately from canonical simulation state.
    pub pending_micros: u64,
}
/// Session operation/outcome history. Adjacent successful explicit steps are run-length encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationRecord {
    /// Input; successful Steps runs are normalized regardless of host grouping.
    pub operation: Operation,
    /// Committed prefix and any failure.
    pub report: Report,
    /// Present on failure, so replay can verify the prefix and stop without retrying an environmental failure.
    pub stop: Option<Checkpoint>,
}
/// Typed replay construction/resource failure or the first inconsistent outcome/fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    /// Baseline creation or operation-log reservation failed.
    Simulation(SliceError),
    /// Zero-based record whose outcome, prefix or checkpoint is inconsistent.
    Record(usize),
}
impl core::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "slice replay error: {self:?}")
    }
}
impl std::error::Error for ReplayError {}
/// Owns the populated baseline, clock, immutable schedule domain and factual store.
/// Queries/derived boundary cache do not drive behavior or enter the state hash.
#[derive(Debug, Clone)]
pub struct SliceSimulation {
    config: SliceConfig,
    population: PopulatedSlice,
    schedules: Schedules,
    behavior: Behavior,
    clock: Clock,
    events: EventStore,
    next_change: Option<SimTime>,
    operations: Vec<OperationRecord>,
    operation_hash: SipHasher24,
    #[cfg(test)]
    fail_tick: Option<u64>,
    #[cfg(test)]
    fail_behavior_tick: Option<u64>,
    #[cfg(test)]
    fail_after_behavior_tick: Option<u64>,
}
impl SliceSimulation {
    /// Creates one slice with one creation fact and no false initial transitions.
    /// # Errors
    /// Invalid grid/baseline/destinations or failed history preparation.
    pub fn new(config: SliceConfig) -> Result<Self, SliceError> {
        let clock = Clock::with_defaults(config.start);
        if !clock.base_step().is_tick_start(config.start)
            || !clock.base_step().is_tick_start(config.schedule_epoch)
        {
            return Err(SliceError::Grid);
        }
        let population = PopulatedSlice::new(config.seed).map_err(SliceError::Population)?;
        let schedules =
            Schedules::new(&population, config.schedule_epoch).map_err(SliceError::Destination)?;
        let behavior =
            Behavior::new(&population, &schedules, config.start).map_err(SliceError::Behavior)?;
        let known = population
            .world()
            .buildings()
            .ids()
            .iter()
            .chain(population.households().ids())
            .chain(population.residents().ids())
            .chain(population.workplaces().ids())
            .copied()
            .collect();
        let mut events = EventStore::new(known);
        let b = population.baseline();
        let fact = Candidate::new(
            config.start,
            0,
            0,
            Payload::SliceCreated {
                seed: config.seed,
                dataset: b.structural.dataset_version,
                structural: b.structural.generator_version,
                population: b.generator_version,
                schedules: RULES_VERSION,
                schedule_epoch: config.schedule_epoch.as_micros_since_epoch(),
            },
            Origin::System(0),
        );
        events
            .prepare(0, &[fact])
            .map_err(SliceError::History)?
            .commit();
        let next_change = schedules.next_change(config.start);
        Ok(Self {
            config,
            population,
            schedules,
            behavior,
            clock,
            events,
            next_change,
            operations: Vec::new(),
            operation_hash: SipHasher24::new(0, Purpose::new("ws-sim/slice-operations").value()),
            #[cfg(test)]
            fail_tick: None,
            #[cfg(test)]
            fail_behavior_tick: None,
            #[cfg(test)]
            fail_after_behavior_tick: None,
        })
    }
    /// Immutable baseline.
    pub const fn population(&self) -> &PopulatedSlice {
        &self.population
    }
    /// Immutable pure schedule domain.
    pub const fn schedules(&self) -> &Schedules {
        &self.schedules
    }
    /// Read-only retained need/utility state and navigation proxy.
    pub const fn behavior(&self) -> &Behavior {
        &self.behavior
    }
    /// One current read-only resident view; no stepping or materialization side effect.
    /// # Errors
    /// Unknown resident or a violated committed-domain invariant.
    pub fn resident(&self, id: EntityId) -> Result<Snapshot, SliceError> {
        self.behavior
            .snapshot(id, self.clock.now())
            .map_err(SliceError::Behavior)
    }
    /// Immutable committed clock.
    pub const fn clock(&self) -> &Clock {
        &self.clock
    }
    /// Read-only factual history; queries never advance or flush anything.
    pub const fn events(&self) -> &EventStore {
        &self.events
    }
    /// Typed normalized session records, not a persistent transport.
    pub fn operations(&self) -> &[OperationRecord] {
        &self.operations
    }
    /// Digest of explicit normalized primitive operations and outcomes.
    /// Grouping successful explicit steps into batches does not change it.
    pub fn operation_digest(&self) -> u64 {
        self.operation_hash.finish()
    }
    /// Canonical state/configuration in explicit integer words. Host pacing,
    /// factual/operation histories, capacities and derived boundary cache are separate.
    pub fn state_hash(&self) -> u64 {
        let b = self.population.baseline();
        ws_rng::hash(
            0,
            Purpose::new("ws-sim/slice-state").value(),
            &[
                self.config.seed,
                self.config.start.as_micros_since_epoch() as u64,
                self.config.schedule_epoch.as_micros_since_epoch() as u64,
                b.structural.dataset_version,
                u64::from(b.structural.generator_version),
                u64::from(b.generator_version),
                u64::from(RULES_VERSION),
                self.clock.steps(),
                self.clock.now().as_micros_since_epoch() as u64,
                u64::from(self.clock.scale().multiplier()),
                self.schedules.state_hash(self.clock.now()),
                self.behavior.state_hash(),
            ],
        )
    }
    /// Current replay checkpoint, including pacing separately.
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            tick: self.clock.steps(),
            state: self.state_hash(),
            events: self.events.digest(),
            pending_micros: self.clock.pending_micros(),
        }
    }
    /// One explicit physical step, preserving the failing tick on error.
    /// # Errors
    /// Reservation, clock overflow or event preparation failure.
    pub fn step(&mut self) -> Result<(), SliceError> {
        match self.apply(Operation::Steps(1))?.error {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
    /// Applies a typed operation at a serialized boundary and records its outcome.
    /// Every event-bearing physical tick commits separately, independent of batching.
    /// # Errors
    /// Resource means the operation-log reservation failed before any mutation;
    /// subsequent failures are returned in Report with an explicit committed prefix.
    pub fn apply(&mut self, operation: Operation) -> Result<Report, SliceError> {
        self.operations
            .try_reserve(2)
            .map_err(|_| SliceError::Resource)?;
        let mut report = Report {
            committed: 0,
            error: None,
        };
        match operation {
            Operation::SetDetail { resident, level } => {
                self.operation_hash
                    .write_words(&[5, resident.raw(), level as u64]);
                report.error = self
                    .behavior
                    .set_detail(resident, level, self.clock.now())
                    .err()
                    .map(SliceError::Behavior);
            }
            Operation::SetScale(scale) => {
                self.clock.set_scale(scale);
                self.operation_hash
                    .write_words(&[1, u64::from(scale.multiplier())]);
            }
            Operation::Steps(count) => {
                for _ in 0..count {
                    match self.physical_step() {
                        Ok(()) => {
                            report.committed += 1;
                            self.operation_hash.write_u64(0);
                        }
                        Err(e) => {
                            report.error = Some(e);
                            break;
                        }
                    }
                }
            }
            Operation::HostElapsed(elapsed) => {
                self.operation_hash.write_words(&[2, elapsed]);
                let mut target = self.clock.clone();
                match target.advance_by_host_elapsed(elapsed) {
                    Err(_) => report.error = Some(SliceError::TimeOverflow),
                    Ok(due) => {
                        for _ in 0..due {
                            match self.physical_step() {
                                Ok(()) => {
                                    report.committed += 1;
                                    self.operation_hash.write_u64(0);
                                }
                                Err(e) => {
                                    report.error = Some(e);
                                    break;
                                }
                            }
                        }
                        if report.error.is_none() {
                            self.clock = target;
                        }
                    }
                }
                self.operation_hash.write_words(&[
                    3,
                    report.committed,
                    self.clock.pending_micros(),
                ]);
            }
        }
        let stop = report.error.map(|_| self.checkpoint());
        if let Some(error) = report.error {
            let at = stop.unwrap();
            self.operation_hash.write_words(&[
                4,
                error_tag(error),
                at.tick,
                at.state,
                at.events,
                at.pending_micros,
            ]);
        }
        // Success grouping is representation only. Hashing above always used primitive steps.
        if let Operation::Steps(count) = operation
            && report.error.is_none()
        {
            if count == 0 {
                return Ok(report);
            }
            if let Some(OperationRecord {
                operation: Operation::Steps(n),
                report: old,
                stop: None,
            }) = self.operations.last_mut()
                && old.error.is_none()
            {
                *n += count;
                old.committed += count;
                return Ok(report);
            }
        }
        self.operations.push(OperationRecord {
            operation,
            report,
            stop,
        });
        Ok(report)
    }
    /// Replays typed session records, verifying outcomes and environmental stop fences.
    /// A failure must be the last record. Resource stops replay only their committed
    /// prefix and verify its checkpoint; they never retry or fabricate the suffix.
    /// Returns the reconstructed world and whether replay stopped at a failure.
    /// There is no serialization or persistent transport here.
    /// # Errors
    /// Invalid outcomes/checkpoints, a suffix after a stop, or actual replay resource failure.
    pub fn replay(
        config: SliceConfig,
        records: &[OperationRecord],
    ) -> Result<(Self, bool), ReplayError> {
        let mut world = Self::new(config).map_err(ReplayError::Simulation)?;
        let mut saved = Vec::new();
        saved
            .try_reserve(records.len())
            .map_err(|_| ReplayError::Simulation(SliceError::Resource))?;
        let mut stopped = false;
        for (index, record) in records.iter().enumerate() {
            if record.report.error.is_some() && index + 1 != records.len() {
                return Err(ReplayError::Record(index));
            }
            if matches!(
                record.report.error,
                Some(
                    SliceError::History(EventError::Resource)
                        | SliceError::Behavior(BehaviorError::Resource)
                )
            ) {
                match record.operation {
                    Operation::Steps(requested) if record.report.committed < requested => {
                        let report = world
                            .apply(Operation::Steps(record.report.committed))
                            .map_err(ReplayError::Simulation)?;
                        if report.error.is_some() {
                            return Err(ReplayError::Record(index));
                        }
                    }
                    Operation::HostElapsed(elapsed) => {
                        let mut target = world.clock.clone();
                        let due = target
                            .advance_by_host_elapsed(elapsed)
                            .map_err(|_| ReplayError::Record(index))?;
                        if record.report.committed >= u64::from(due) {
                            return Err(ReplayError::Record(index));
                        }
                        world.operation_hash.write_words(&[2, elapsed]);
                        for _ in 0..record.report.committed {
                            world.physical_step().map_err(ReplayError::Simulation)?;
                            world.operation_hash.write_u64(0);
                        }
                        world.operation_hash.write_words(&[
                            3,
                            record.report.committed,
                            world.clock.pending_micros(),
                        ]);
                    }
                    _ => return Err(ReplayError::Record(index)),
                }
                let checkpoint = world.checkpoint();
                if record.stop != Some(checkpoint) {
                    return Err(ReplayError::Record(index));
                }
                world.operation_hash.write_words(&[
                    4,
                    error_tag(record.report.error.unwrap()),
                    checkpoint.tick,
                    checkpoint.state,
                    checkpoint.events,
                    checkpoint.pending_micros,
                ]);
                stopped = true;
            } else {
                let report = world
                    .apply(record.operation)
                    .map_err(ReplayError::Simulation)?;
                if report != record.report
                    || world.operations.last().and_then(|r| r.stop) != record.stop
                {
                    return Err(ReplayError::Record(index));
                }
                stopped = report.error.is_some();
            }
            saved.push(*record);
        }
        world.operations = saved;
        Ok((world, stopped))
    }
    fn physical_step(&mut self) -> Result<(), SliceError> {
        let mut clock = self.clock.clone();
        clock.step().map_err(|_| SliceError::TimeOverflow)?;
        #[cfg(test)]
        if self.fail_tick == Some(clock.steps()) {
            return Err(SliceError::History(EventError::Resource));
        }
        let behavior = self
            .behavior
            .prepare(clock.now())
            .map_err(SliceError::Behavior)?;
        #[cfg(test)]
        if self.fail_behavior_tick == Some(clock.steps()) {
            return Err(SliceError::Behavior(BehaviorError::Resource));
        }
        #[cfg(test)]
        if self.fail_after_behavior_tick == Some(clock.steps()) {
            return Err(SliceError::History(EventError::Resource));
        }
        if self.next_change.is_some_and(|t| t <= clock.now()) {
            let facts = self
                .schedules
                .facts_at(clock.now())
                .map_err(SliceError::History)?;
            let next = self.schedules.next_change(clock.now());
            self.events
                .prepare(clock.steps(), &facts)
                .map_err(SliceError::History)?
                .commit();
            self.next_change = next;
        }
        behavior.commit();
        self.clock = clock;
        Ok(())
    }
}
fn error_tag(error: SliceError) -> u64 {
    match error {
        SliceError::Population(_) => 1,
        SliceError::Destination(_) => 2,
        SliceError::Grid => 3,
        SliceError::TimeOverflow => 4,
        SliceError::Resource => 5,
        SliceError::Behavior(e) => {
            20 + match e {
                BehaviorError::Navigation(e) => match e {
                    ws_life::navigation::NavError::Geometry => 0,
                    ws_life::navigation::NavError::Endpoint => 1,
                    ws_life::navigation::NavError::Disconnected => 2,
                    ws_life::navigation::NavError::Resource => 3,
                },
                BehaviorError::InfeasibleRoute => 4,
                BehaviorError::Grid => 5,
                BehaviorError::Time => 6,
                BehaviorError::Counter => 7,
                BehaviorError::UnknownResident => 8,
                BehaviorError::Resource => 9,
            }
        }
        SliceError::History(e) => {
            10 + match e {
                EventError::Limit => 0,
                EventError::Order => 1,
                EventError::Cause => 2,
                EventError::UnknownActor => 3,
                EventError::Resource => 4,
            }
        }
    }
}
#[cfg(test)]
mod active_tests;
#[cfg(test)]
mod tests;
