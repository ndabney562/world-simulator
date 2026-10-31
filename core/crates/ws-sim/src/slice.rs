//! Real-slice L2 orchestration (ADR 0027), separate from the compatible toy World.
use crate::{Clock, SimTime, TimeScale};
use ws_ecs::EntityId;
use ws_events::{Candidate, EventError, EventStore, Origin, Payload};
use ws_life::behavior::{Behavior, BehaviorError, Detail, Snapshot};
pub use ws_life::lod::CameraInterest;
use ws_life::lod::{Controller, LodError, View};
pub use ws_life::navigation::Point;
use ws_life::{InvalidPopulation, RULES_VERSION, Schedules};
use ws_rng::{Purpose, SipHasher24};
use ws_world::slice::population::{PopulatedSlice, PopulationError};

#[path = "slice/god.rs"]
mod god;
#[path = "slice/illness.rs"]
pub mod illness;
#[path = "slice/powers.rs"]
mod powers;
pub use god::{
    Caps, FAITH_MAX, FAITH_MIN, Goal, GoalFact, GoalKind, Goals, God, GodError, MAX_GOALS,
    Observed, day_of, generate as generate_goals,
};
pub use powers::{Gathering, MAX_ATTENDEES, Phase, REACH_MM, Storm, distance_ok, offset_mm};

#[path = "slice_inspect.rs"]
pub mod inspect;
#[path = "slice_render.rs"]
pub mod render;
#[path = "slice/restore.rs"]
pub mod restore;

pub use restore::{SliceParts, SliceRestoreError};

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
impl SliceConfig {
    /// Construct explicit civil-phase configuration under NY policy1 (ADR0030).
    /// Store the returned epoch in configuration; do not recompute it while
    /// stepping. Subsequent appointments retain elapsed model-week semantics.
    /// Existing raw construction remains available, including extreme times.
    /// # Errors
    /// Creation outside UTC2000–2099. Grid validation remains [`SliceSimulation::new`]'s responsibility.
    pub fn calendar_aligned_v1(
        seed: u64,
        start: SimTime,
    ) -> Result<Self, ws_time::UnsupportedAlignment> {
        Ok(Self {
            seed,
            start,
            schedule_epoch: ws_time::model_monday_v1(start)?,
        })
    }
}
/// Typed construction, time, reservation or history failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceError {
    /// Cannot generate the reviewed source/population.
    Population(PopulationError),
    /// Semantic illness rejection; replay may continue with later operations.
    Rejected(illness::Rejection),
    /// Required schedule destination is missing.
    Destination(InvalidPopulation),
    /// Behavior/route/detail validation or planning failure.
    Behavior(BehaviorError),
    /// Camera ownership, controller planning or count validation failure.
    Lod(LodError),
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
    /// Finite single-resident illness at the current committed boundary.
    Afflict {
        /// Existing living resident.
        resident: EntityId,
        /// Integer severity,1 through10.
        severity: u32,
        /// Grid-aligned duration, one tick through24hours.
        duration_us: u64,
    },
    /// Bless one living resident (ADR 0038 §2): cure an active illness episode,
    /// otherwise zero the four need deficits; a no-op is accepted with `affected = 0`.
    Bless {
        /// Existing living resident.
        resident: EntityId,
    },
    /// Block-wide storm (ADR 0038 §3); rejected while a storm or a gathering is active.
    Storm {
        /// Grid-aligned duration, one tick through one hour.
        duration_us: u64,
    },
    /// Gathering at a featured residential building (ADR 0038 §4); rejected while
    /// a gathering or a storm is active.
    Gather {
        /// Featured residential building.
        building: EntityId,
        /// Grid-aligned duration, ten minutes through one hour.
        duration_us: u64,
    },
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
    /// One coarse camera-interest input for the complete real slice.
    CameraInterest(CameraInterest),
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
    /// Historical error checkpoint (including non-terminal rejections).
    /// Only non-Rejected errors stop replay; presence alone is not a stop flag.
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
    vitals: ws_world::slice::vitals::Vitals,
    illness: illness::Illness,
    powers: powers::Powers,
    god: god::God,
    schedules: Schedules,
    behavior: Behavior,
    lod: Controller,
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
    #[cfg(test)]
    fail_controller_tick: Option<u64>,
}
impl SliceSimulation {
    /// Immutable explicit creation/model-epoch configuration. Querying never
    /// recomputes civil alignment or changes elapsed schedule phase.
    pub const fn config(&self) -> SliceConfig {
        self.config
    }
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
        let vitals = ws_world::slice::vitals::Vitals::new(&population);
        let schedules =
            Schedules::new(&population, config.schedule_epoch).map_err(SliceError::Destination)?;
        let behavior =
            Behavior::new(&population, &schedules, config.start).map_err(SliceError::Behavior)?;
        let lod = Controller::new(&population, &behavior, config.start, config.schedule_epoch)
            .map_err(lod_error)?;
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
        let god = fresh_god(config, &population, &vitals, config.start);
        Ok(Self {
            config,
            population,
            vitals,
            illness: illness::Illness::default(),
            powers: powers::Powers::default(),
            god,
            schedules,
            behavior,
            lod,
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
            #[cfg(test)]
            fail_controller_tick: None,
        })
    }
    /// Immutable baseline.
    pub const fn population(&self) -> &PopulatedSlice {
        &self.population
    }
    /// Retained authoritative sex, health and life state.
    pub const fn vitals(&self) -> &ws_world::slice::vitals::Vitals {
        &self.vitals
    }
    /// Immutable pure schedule domain.
    pub const fn schedules(&self) -> &Schedules {
        &self.schedules
    }
    /// Read-only retained need/utility state and navigation proxy.
    pub const fn behavior(&self) -> &Behavior {
        &self.behavior
    }
    /// Read-only camera policy and derived recent observations.
    pub const fn lod(&self) -> &Controller {
        &self.lod
    }
    /// Pure bulk committed residents and exact building/count summaries.
    /// # Errors
    /// Invalid domain view or output capacity reservation failure.
    pub fn view(&self) -> Result<View, SliceError> {
        self.lod
            .view(&self.behavior, self.clock.now())
            .map_err(lod_error)
    }
    /// Discards only rebuildable observation acceleration; no journal or hash change.
    pub fn discard_observation_cache(&mut self) {
        self.lod.discard_observations();
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
        let state = self.state_hash_without_god();
        // ADR 0039 §4: an untouched god is a pure function of hashed state
        // and the day, so it is mixed only once faith or a goal has moved.
        if !self.god.touched() {
            return state;
        }
        ws_rng::hash(
            0,
            Purpose::new("ws-sim/slice-state/v5").value(),
            &[state, self.god.hash()],
        )
    }
    /// [`Self::state_hash`] without the ADR 0039 god block: the hash a
    /// format-4 or format-5 save of this world reloads to, since those
    /// formats restore faith 0 and freshly generated goals.
    #[doc(hidden)]
    pub fn state_hash_without_god(&self) -> u64 {
        let vitals = ws_rng::hash(
            0,
            Purpose::new("ws-sim/slice-state/v2").value(),
            &[
                self.legacy_state_hash(),
                u64::from(ws_world::slice::vitals::VERSION),
                self.vitals.state_hash(),
            ],
        );
        let state = if self.illness.has_episodes() {
            ws_rng::hash(
                0,
                Purpose::new("ws-sim/slice-state/v3").value(),
                &[vitals, u64::from(illness::VERSION), self.illness.hash()],
            )
        } else {
            vitals
        };
        // Mixed only while a block power is active, so earlier goldens hold.
        if !self.powers.active() {
            return state;
        }
        ws_rng::hash(
            0,
            Purpose::new("ws-sim/slice-state/v4").value(),
            &[state, self.powers.hash()],
        )
    }
    /// Resident faith and the day's goals (ADR 0039).
    pub const fn god(&self) -> &god::God {
        &self.god
    }
    /// Goal feasibility caps from the current committed state (ADR 0039 §2).
    pub fn caps(&self) -> god::Caps {
        caps(&self.population, &self.vitals)
    }
    fn legacy_state_hash(&self) -> u64 {
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
                self.lod.state_hash(),
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
            Operation::Afflict {
                resident,
                severity,
                duration_us,
            } => {
                self.operation_hash.write_words(&[
                    7,
                    resident.raw(),
                    u64::from(severity),
                    duration_us,
                ]);
                report.error = self.afflict(resident, severity, duration_us).err();
            }
            Operation::Bless { resident } => {
                self.operation_hash.write_words(&[8, resident.raw()]);
                report.error = self.bless(resident).err();
            }
            Operation::Storm { duration_us } => {
                self.operation_hash.write_words(&[9, duration_us]);
                report.error = self.cast_storm(duration_us).err();
            }
            Operation::Gather {
                building,
                duration_us,
            } => {
                self.operation_hash
                    .write_words(&[10, building.raw(), duration_us]);
                report.error = self.gather(building, duration_us).err();
            }
            Operation::CameraInterest(camera) => {
                self.operation_hash.write_words(&[
                    6,
                    camera.center.east as u64,
                    camera.center.north as u64,
                    camera.radius_mm,
                ]);
                report.error = self.lod.set_camera(camera).err().map(lod_error);
            }
            Operation::SetDetail { resident, level } => {
                self.operation_hash
                    .write_words(&[5, resident.raw(), level as u64]);
                report.error = if self.lod.camera().is_some() {
                    Some(SliceError::Lod(LodError::CameraOwned))
                } else {
                    self.behavior
                        .set_detail(resident, level, self.clock.now())
                        .err()
                        .map(SliceError::Behavior)
                };
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
        // Every error contributes this checkpoint to the operation digest.
        // Rejections may continue, but still need their historical metadata
        // for canonical snapshot restoration without replaying the world.
        let stop = report.error.map(|_| self.checkpoint());
        if let Some(error) = report.error {
            let at = self.checkpoint();
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
            if record
                .report
                .error
                .is_some_and(|e| !matches!(e, SliceError::Rejected(_)))
                && index + 1 != records.len()
            {
                return Err(ReplayError::Record(index));
            }
            if matches!(
                record.report.error,
                Some(
                    SliceError::History(EventError::Resource)
                        | SliceError::Behavior(BehaviorError::Resource)
                        | SliceError::Lod(LodError::Resource)
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
                    Operation::Afflict {
                        resident,
                        severity,
                        duration_us,
                    } if record.report.committed == 0 => {
                        world
                            .afflict_preflight(resident, severity, duration_us)
                            .map_err(|_| ReplayError::Record(index))?;
                        world.operation_hash.write_words(&[
                            7,
                            resident.raw(),
                            u64::from(severity),
                            duration_us,
                        ]);
                    }
                    Operation::Bless { resident } if record.report.committed == 0 => {
                        world
                            .bless_preflight(resident)
                            .map_err(|_| ReplayError::Record(index))?;
                        world.operation_hash.write_words(&[8, resident.raw()]);
                    }
                    Operation::Storm { duration_us } if record.report.committed == 0 => {
                        world
                            .storm_preflight(duration_us)
                            .map_err(|_| ReplayError::Record(index))?;
                        world.operation_hash.write_words(&[9, duration_us]);
                    }
                    Operation::Gather {
                        building,
                        duration_us,
                    } if record.report.committed == 0 => {
                        world
                            .gather_preflight(building, duration_us)
                            .map_err(|_| ReplayError::Record(index))?;
                        world
                            .operation_hash
                            .write_words(&[10, building.raw(), duration_us]);
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
                stopped = report
                    .error
                    .is_some_and(|e| !matches!(e, SliceError::Rejected(_)));
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
        let minute = self
            .behavior
            .next_decision()
            .is_some_and(|t| t <= clock.now());
        let schedule_due = self.next_change.is_some_and(|t| t <= clock.now());
        let mut previous = Vec::new();
        if schedule_due {
            previous
                .try_reserve(self.schedules.profiles().len())
                .map_err(|_| SliceError::History(EventError::Resource))?;
            for p in self.schedules.profiles() {
                previous.push((
                    p.resident,
                    self.behavior
                        .common(p.resident, self.clock.now())
                        .map(|s| s.logical.activity),
                ));
            }
        }
        // Attendee routines before preparation may clear them at their rejoin.
        let routines = powers::attendee_routines(&self.powers, &self.behavior)?;
        let mut behavior = self
            .behavior
            .prepare(clock.now())
            .map_err(SliceError::Behavior)?;
        #[cfg(test)]
        if self.fail_behavior_tick == Some(clock.steps()) {
            return Err(SliceError::Behavior(BehaviorError::Resource));
        }
        let mut illness_plan = self.illness.prepare_tick(
            &mut behavior,
            &self.population,
            &self.schedules,
            &self.vitals,
            minute,
            schedule_due,
        )?;
        let lod = self
            .lod
            .prepare(&mut behavior, clock.scale().multiplier())
            .map_err(lod_error)?;
        let (powers, mut storm_facts) =
            powers::storm_tick(self.powers, &self.events, &behavior, clock.now())?;
        let (powers, mut gathering_facts) =
            powers::gathering_tick(powers, &routines, &behavior, self.clock.now(), clock.now())?;
        #[cfg(test)]
        if self.fail_controller_tick == Some(clock.steps()) {
            return Err(SliceError::Lod(LodError::Resource));
        }
        #[cfg(test)]
        if self.fail_after_behavior_tick == Some(clock.steps()) {
            return Err(SliceError::History(EventError::Resource));
        }
        let (mut facts, next) = if schedule_due {
            let mut facts = self
                .schedules
                .facts_at(clock.now())
                .map_err(SliceError::History)?;
            facts.retain(|c| {
                let Payload::ScheduleChanged {
                    resident,
                    previous: old,
                    next,
                    ..
                } = c.payload
                else {
                    return true;
                };
                previous
                    .iter()
                    .find(|(id, _)| *id == resident)
                    .is_some_and(|(_, a)| *a == Ok(old))
                    && behavior
                        .common(resident)
                        .is_ok_and(|s| s.logical.activity == next)
            });
            let next = self.schedules.next_change(clock.now());
            (facts, next)
        } else {
            (Vec::new(), self.next_change)
        };
        facts
            .try_reserve(lod.changes().count())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        facts.extend(
            lod.changes()
                .enumerate()
                .map(|(i, c)| c.fact(clock.now(), i as u64)),
        );
        facts
            .try_reserve(illness_plan.facts.len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        facts.append(&mut illness_plan.facts);
        facts
            .try_reserve(storm_facts.len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        facts.append(&mut storm_facts);
        facts
            .try_reserve(gathering_facts.len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        facts.append(&mut gathering_facts);
        facts.sort_unstable_by_key(|c| (c.system, c.sequence));
        let god = god_pass(
            &self.god,
            self.config,
            &self.population,
            &self.vitals,
            &mut facts,
            clock.now(),
        )?;
        let updates = self.illness.stage_index(
            &self.events,
            &facts,
            self.events.counters().map_err(SliceError::History)?.0,
        )?;
        self.events
            .prepare(clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        behavior.commit();
        lod.commit();
        self.illness.commit(illness_plan, updates, &mut self.vitals);
        self.powers = powers;
        self.god = god;
        self.next_change = next;
        self.clock = clock;
        Ok(())
    }
}
/// Producer system for `GoalCompleted` (ADR 0039 §3), after every other producer.
const GOAL_SYSTEM: u16 = 87;
/// ADR 0039 §2 caps: the most living residents whose home lies within reach
/// of any one featured residential building, living and retained residents.
fn caps(population: &PopulatedSlice, vitals: &ws_world::slice::vitals::Vitals) -> god::Caps {
    use ws_world::slice::vitals::Life;
    let buildings = population.world().buildings();
    let alive = |id| vitals.get(id).is_some_and(|v| v.life() == Life::Alive);
    let gathering_reach = buildings
        .iter()
        .filter(|(_, b)| b.residential_units > 0)
        .map(|(_, target)| {
            population
                .residents()
                .iter()
                .filter(|(id, r)| {
                    alive(*id)
                        && buildings
                            .get(r.home)
                            .is_some_and(|home| powers::distance_ok(home, target))
                })
                .count() as u32
        })
        .max()
        .unwrap_or(0);
    let ids = population.residents().ids();
    god::Caps {
        gathering_reach,
        living: ids.iter().filter(|id| alive(**id)).count() as u32,
        residents: ids.len() as u32,
    }
}
/// Faith 0 for every retained resident and the goals of `now`'s day, as at a
/// day boundary (creation, and restore until saves carry god state).
fn fresh_god(
    config: SliceConfig,
    population: &PopulatedSlice,
    vitals: &ws_world::slice::vitals::Vitals,
    now: SimTime,
) -> god::God {
    let day = god::day_of(now, config.schedule_epoch);
    god::God::new(
        population.residents().ids(),
        god::generate(config.seed, day, caps(population, vitals)),
    )
}
/// ADR 0039 post-pass over one commit batch whose candidates are final and
/// sorted. Rolls the goal day first when `now` is in a new day (so this batch
/// counts toward it), applies the faith and goal rules in batch order, and
/// appends one `GoalCompleted` per completed goal, `ContributedTo/Direct` the
/// triggering candidate by in-batch link. Returns the next god, which the
/// caller publishes only after the events commit.
fn god_pass(
    current: &god::God,
    config: SliceConfig,
    population: &PopulatedSlice,
    vitals: &ws_world::slice::vitals::Vitals,
    facts: &mut Vec<Candidate>,
    now: SimTime,
) -> Result<god::God, SliceError> {
    use ws_events::{Cause, Confidence, Relation, Target};
    let day = god::day_of(now, config.schedule_epoch);
    let rolled;
    let base = if day == current.goals().day {
        current
    } else {
        rolled = current.roll_day(
            day,
            god::generate(config.seed, day, caps(population, vitals)),
        );
        &rolled
    };
    // A cure is a `Recovered` caused in this batch by a bless of that resident.
    let mut cured = Vec::new();
    for c in facts.iter() {
        let Payload::Recovered { resident, .. } = c.payload else {
            continue;
        };
        let by_bless = c.causes.iter().any(|l| {
            l.relation == Relation::CausedBy
                && matches!(l.target, Target::Local(j)
                    if matches!(facts.get(j as usize).map(|f| f.payload),
                        Some(Payload::BlessApplied { resident: r, .. }) if r == resident))
        });
        if by_bless {
            cured
                .try_reserve(1)
                .map_err(|_| SliceError::History(EventError::Resource))?;
            cured.push(resident);
        }
    }
    let mut observed = Vec::new();
    observed
        .try_reserve(facts.len())
        .map_err(|_| SliceError::History(EventError::Resource))?;
    observed.extend(facts.iter().enumerate().map(|(index, c)| god::Observed {
        index,
        payload: &c.payload,
    }));
    let (next, completed) = base.plan(&observed, &cured);
    drop(observed);
    facts
        .try_reserve(completed.len())
        .map_err(|_| SliceError::History(EventError::Resource))?;
    for g in completed {
        let mut c = Candidate::new(
            now,
            GOAL_SYSTEM,
            u64::from(g.slot),
            Payload::GoalCompleted {
                day,
                slot: g.slot,
                kind: g.kind as u8,
                target: g.target,
            },
            Origin::System(GOAL_SYSTEM),
        );
        c.causes
            .push(Cause {
                target: Target::Local(
                    u32::try_from(g.trigger).map_err(|_| SliceError::History(EventError::Limit))?,
                ),
                relation: Relation::ContributedTo,
                confidence: Confidence::Direct,
            })
            .map_err(SliceError::History)?;
        facts.push(c);
    }
    Ok(next)
}
fn lod_error(error: LodError) -> SliceError {
    match error {
        LodError::Behavior(error) => SliceError::Behavior(error),
        other => SliceError::Lod(other),
    }
}
fn error_tag(error: SliceError) -> u64 {
    match error {
        SliceError::Rejected(e) => 100 + e as u64,
        SliceError::Population(_) => 1,
        SliceError::Destination(_) => 2,
        SliceError::Grid => 3,
        SliceError::TimeOverflow => 4,
        SliceError::Resource => 5,
        SliceError::Lod(e) => match e {
            LodError::Input => 40,
            LodError::CameraOwned => 41,
            LodError::Population => 42,
            LodError::Resource => 43,
            LodError::Invariant => 44,
            LodError::Behavior(e) => error_tag(SliceError::Behavior(e)),
        },
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
mod camera_tests;
#[cfg(test)]
mod tests;
