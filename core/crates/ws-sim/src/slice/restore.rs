//! Rebuilding a [`SliceSimulation`] from saved parts (ADR 0036).
//!
//! Loading restores canonical state directly and never replays the journal.
//! Derived indices are rebuilt, and every invariant the running slice keeps
//! is checked before the simulation is returned, so a corrupt save is an error
//! and never a half-valid world.

use core::fmt;

use ws_ecs::EntityId;
use ws_events::{Event, EventError, EventStore};
use ws_life::Schedules;
use ws_life::behavior::{Behavior, ResidentStateSnapshot, StormWindow};
use ws_life::lod::{CameraInterest, Controller, Mode};
use ws_rng::{Purpose, SipHasher24};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;
use ws_world::slice::vitals::VitalState;

use super::illness::{Episode, Illness};
use super::powers::{Gathering, Powers, Storm};
use super::{
    Operation, OperationRecord, SliceConfig, SliceError, SliceSimulation, error_tag, lod_error,
};
use crate::Clock;

/// The canonical state of a [`SliceSimulation`]: everything a slice save stores.
#[derive(Debug, Clone, PartialEq)]
pub struct SliceParts {
    /// Pinned slice configuration.
    pub config: SliceConfig,
    /// Simulation clock.
    pub clock: Clock,
    /// Authoritative resident vitals.
    pub vitals: Vec<(EntityId, VitalState)>,
    /// Active and past finite illness episodes.
    pub episodes: Vec<Episode>,
    /// Canonical resident behavior driving states.
    pub behavior_states: Vec<ResidentStateSnapshot>,
    /// Coarse camera interest.
    pub camera: Option<CameraInterest>,
    /// Node representation modes and changed_at timestamps.
    pub nodes: Vec<(EntityId, Mode, SimTime)>,
    /// Every committed slice event, in commit order.
    pub events: Vec<Event>,
    /// Session operation records.
    pub operations: Vec<OperationRecord>,
    /// Active storm episode (ADR 0038 §3); always None in format 4.
    pub storm: Option<Storm>,
    /// `ws-life` storm window, retained after the episode until every hold
    /// has rejoined; always None in format 4.
    pub storm_window: Option<StormWindow>,
    /// Active gathering episode (ADR 0038 §4); always None in format 4.
    pub gathering: Option<Gathering>,
}

/// Why saved slice parts do not form a valid slice simulation.
#[derive(Debug, Clone, PartialEq)]
pub enum SliceRestoreError {
    /// Domain or simulation error during baseline generation or invariant check.
    Simulation(SliceError),
    /// Factual event history failed to reconstruct.
    History(EventError),
    /// A structural, roster, or schema invariant is violated.
    Corrupt(&'static str),
}

impl fmt::Display for SliceRestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Simulation(e) => write!(f, "slice simulation restore error: {e}"),
            Self::History(e) => write!(f, "slice event history error: {e}"),
            Self::Corrupt(msg) => write!(f, "corrupt slice save: {msg}"),
        }
    }
}

impl std::error::Error for SliceRestoreError {}

impl From<SliceError> for SliceRestoreError {
    fn from(e: SliceError) -> Self {
        Self::Simulation(e)
    }
}

impl SliceSimulation {
    /// Extracts canonical state for saving.
    #[must_use]
    pub fn to_parts(&self) -> SliceParts {
        let vitals = self.vitals.iter().map(|(id, v)| (id, *v)).collect();
        let episodes = self.illness.episodes().to_vec();
        let behavior_states = self.behavior.resident_states().collect();
        let nodes = self
            .lod
            .nodes()
            .iter()
            .map(|n| (n.building, n.mode, n.changed_at))
            .collect();
        SliceParts {
            config: self.config,
            clock: self.clock.clone(),
            vitals,
            episodes,
            behavior_states,
            camera: self.lod.camera(),
            nodes,
            events: self.events.records().to_vec(),
            operations: self.operations.clone(),
            storm: self.powers.storm,
            storm_window: self.behavior.storm(),
            gathering: self.powers.gathering,
        }
    }

    /// Rebuilds a real-slice simulation directly from saved parts.
    ///
    /// # Errors
    ///
    /// [`SliceRestoreError`] if the parts do not form a valid, coherent slice simulation.
    pub fn from_parts(parts: SliceParts) -> Result<Self, SliceRestoreError> {
        let SliceParts {
            config,
            clock,
            vitals,
            episodes,
            behavior_states,
            camera,
            nodes,
            events,
            operations,
            storm,
            storm_window,
            gathering,
        } = parts;

        if !clock.base_step().is_tick_start(config.start)
            || !clock.base_step().is_tick_start(config.schedule_epoch)
            || !clock.base_step().is_tick_start(clock.now())
        {
            return Err(SliceRestoreError::Simulation(SliceError::Grid));
        }

        let population = PopulatedSlice::new(config.seed)
            .map_err(|e| SliceRestoreError::Simulation(SliceError::Population(e)))?;

        let vitals = ws_world::slice::vitals::Vitals::from_rows(&population, vitals).ok_or(
            SliceRestoreError::Corrupt("saved vitals do not match population roster"),
        )?;

        let schedules = Schedules::new(&population, config.schedule_epoch)
            .map_err(|e| SliceRestoreError::Simulation(SliceError::Destination(e)))?;

        let behavior = Behavior::from_saved_states(
            &population,
            &schedules,
            &behavior_states,
            clock.now(),
            storm_window,
        )
        .map_err(|e| SliceRestoreError::Simulation(SliceError::Behavior(e)))?;

        let lod = Controller::from_saved_parts(
            &population,
            &behavior,
            config.schedule_epoch,
            camera,
            &nodes,
        )
        .map_err(|e| SliceRestoreError::Simulation(lod_error(e)))?;

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

        let events =
            EventStore::from_records(known, &events).map_err(SliceRestoreError::History)?;

        let illness = Illness::from_episodes(episodes);

        let next_change = schedules.next_change(clock.now());

        // Reconstruct operation hash from operation history.
        let mut operation_hash =
            SipHasher24::new(0, Purpose::new("ws-sim/slice-operations").value());
        // Reconstruct host pacing independently of world evolution. Historical
        // HostElapsed digests contain that call's remainder, not the final one.
        let mut history_clock =
            Clock::new(clock.start(), clock.base_step(), clock.max_steps_per_call());
        for record in &operations {
            if record.report.error.is_some() != record.stop.is_some() {
                return Err(SliceRestoreError::Corrupt(
                    "saved error checkpoint metadata is missing or unexpected",
                ));
            }
            if !matches!(
                record.operation,
                Operation::Steps(_) | Operation::HostElapsed(_)
            ) && record.report.committed != 0
            {
                return Err(SliceRestoreError::Corrupt(
                    "non-stepping operation reports committed ticks",
                ));
            }
            match record.operation {
                Operation::Afflict {
                    resident,
                    severity,
                    duration_us,
                } => {
                    operation_hash.write_words(&[
                        7,
                        resident.raw(),
                        u64::from(severity),
                        duration_us,
                    ]);
                }
                Operation::Bless { resident } => {
                    operation_hash.write_words(&[8, resident.raw()]);
                }
                Operation::Storm { duration_us } => {
                    operation_hash.write_words(&[9, duration_us]);
                }
                Operation::Gather {
                    building,
                    duration_us,
                } => {
                    operation_hash.write_words(&[10, building.raw(), duration_us]);
                }
                Operation::CameraInterest(c) => {
                    operation_hash.write_words(&[
                        6,
                        c.center.east as u64,
                        c.center.north as u64,
                        c.radius_mm,
                    ]);
                }
                Operation::SetDetail { resident, level } => {
                    operation_hash.write_words(&[5, resident.raw(), level as u64]);
                }
                Operation::SetScale(scale) => {
                    history_clock.set_scale(scale);
                    operation_hash.write_words(&[1, u64::from(scale.multiplier())]);
                }
                Operation::Steps(requested) => {
                    if record.report.committed > requested
                        || (record.report.error.is_none() && record.report.committed != requested)
                    {
                        return Err(SliceRestoreError::Corrupt(
                            "inconsistent saved step outcome",
                        ));
                    }
                    for _ in 0..record.report.committed {
                        history_clock
                            .step()
                            .map_err(|_| SliceRestoreError::Corrupt("saved steps overflow"))?;
                        operation_hash.write_u64(0);
                    }
                }
                Operation::HostElapsed(elapsed) => {
                    operation_hash.write_words(&[2, elapsed]);
                    let mut target = history_clock.clone();
                    match target.advance_by_host_elapsed(elapsed) {
                        Ok(due)
                            if record.report.error.is_none()
                                && u64::from(due) == record.report.committed =>
                        {
                            history_clock = target;
                        }
                        Ok(due)
                            if record.report.error.is_some()
                                && record.report.committed < u64::from(due) =>
                        {
                            // Failed calls keep their PRE-call pacing remainder,
                            // while retaining only their committed physical prefix.
                            for _ in 0..record.report.committed {
                                history_clock.step().map_err(|_| {
                                    SliceRestoreError::Corrupt("saved prefix overflows")
                                })?;
                            }
                        }
                        Err(_)
                            if record.report.error == Some(SliceError::TimeOverflow)
                                && record.report.committed == 0 => {}
                        _ => {
                            return Err(SliceRestoreError::Corrupt(
                                "inconsistent saved host outcome",
                            ));
                        }
                    }
                    for _ in 0..record.report.committed {
                        operation_hash.write_u64(0);
                    }
                    let pending = history_clock.pending_micros();
                    if record
                        .stop
                        .is_some_and(|stop| stop.pending_micros != pending)
                    {
                        return Err(SliceRestoreError::Corrupt(
                            "saved stop remainder disagrees with history",
                        ));
                    }
                    operation_hash.write_words(&[3, record.report.committed, pending]);
                }
            }
            if let (Some(error), Some(stop)) = (record.report.error, record.stop) {
                if stop.tick != history_clock.steps()
                    || stop.pending_micros != history_clock.pending_micros()
                {
                    return Err(SliceRestoreError::Corrupt(
                        "saved error checkpoint disagrees with clock history",
                    ));
                }
                operation_hash.write_words(&[
                    4,
                    error_tag(error),
                    stop.tick,
                    stop.state,
                    stop.events,
                    stop.pending_micros,
                ]);
            }
        }

        if history_clock != clock {
            return Err(SliceRestoreError::Corrupt(
                "saved clock disagrees with operation history",
            ));
        }

        let mut sim = Self {
            config,
            population,
            vitals,
            illness,
            // Format 4 holds no block power, so both are None there.
            powers: Powers { storm, gathering },
            schedules,
            behavior,
            lod,
            clock,
            events,
            next_change,
            operations,
            operation_hash,
            #[cfg(test)]
            fail_tick: None,
            #[cfg(test)]
            fail_behavior_tick: None,
            #[cfg(test)]
            fail_after_behavior_tick: None,
            #[cfg(test)]
            fail_controller_tick: None,
        };

        // Rebuild derived consequence groups index from committed history.
        sim.rebuild_consequence_index()
            .map_err(SliceRestoreError::Simulation)?;

        Ok(sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ws_life::navigation::Point;

    #[test]
    fn slice_parts_round_trip_reproduces_state_hash_checkpoints_and_future_stepping() {
        let config =
            SliceConfig::calendar_aligned_v1(42, SimTime::from_micros_since_epoch(1_000_000_000))
                .unwrap();
        let mut sim = SliceSimulation::new(config).unwrap();

        // Advance 25 steps
        sim.apply(Operation::Steps(25)).unwrap();

        // Afflict one resident
        let resident = sim.population().residents().ids()[0];
        sim.apply(Operation::Afflict {
            resident,
            severity: 6,
            duration_us: 10_000_000,
        })
        .unwrap();

        // Set camera interest
        sim.apply(Operation::CameraInterest(CameraInterest {
            center: Point {
                east: 10_000,
                north: 20_000,
            },
            radius_mm: 50_000,
        }))
        .unwrap();

        // Advance 25 more steps
        sim.apply(Operation::Steps(25)).unwrap();

        let parts = sim.to_parts();
        let mut restored = SliceSimulation::from_parts(parts).expect("round trip from parts");

        assert_eq!(restored.state_hash(), sim.state_hash());
        assert_eq!(restored.checkpoint(), sim.checkpoint());
        assert_eq!(restored.operation_digest(), sim.operation_digest());
        assert_eq!(restored.events().digest(), sim.events().digest());

        // Step both identically and assert future hashes match
        for _ in 0..50 {
            sim.step().unwrap();
            restored.step().unwrap();
            assert_eq!(restored.state_hash(), sim.state_hash());
        }
    }
    fn paced(config: SliceConfig) -> SliceSimulation {
        let mut sim = SliceSimulation::new(config).unwrap();
        sim.apply(Operation::HostElapsed(90_000)).unwrap();
        sim
    }
    fn config(start: i64) -> SliceConfig {
        SliceConfig {
            seed: 42,
            start: SimTime::from_micros_since_epoch(start),
            schedule_epoch: SimTime::EPOCH,
        }
    }
    #[test]
    fn restore_retains_failed_host_prefix_and_pre_call_remainder() {
        let mut sim = paced(config(0));
        sim.fail_tick = Some(3);
        let report = sim.apply(Operation::HostElapsed(1_000_123)).unwrap();
        assert_eq!(report.committed, 2);
        assert!(report.error.is_some());
        let restored = SliceSimulation::from_parts(sim.to_parts()).unwrap();
        assert_eq!(restored.clock(), sim.clock());
        assert_eq!(restored.operation_digest(), sim.operation_digest());
        assert_eq!(restored.checkpoint(), sim.checkpoint());
    }
    #[test]
    fn restore_retains_host_overflow_and_rejects_forged_outcomes_or_clock() {
        let max = i64::MAX / 100_000 * 100_000;
        let mut sim = paced(config(max - 100_000));
        let report = sim.apply(Operation::HostElapsed(u64::MAX)).unwrap();
        assert_eq!(report.error, Some(SliceError::TimeOverflow));
        let restored = SliceSimulation::from_parts(sim.to_parts()).unwrap();
        assert_eq!(restored.clock(), sim.clock());
        assert_eq!(restored.operation_digest(), sim.operation_digest());
        let mut parts = sim.to_parts();
        parts.operations.last_mut().unwrap().report.committed = 1;
        assert!(SliceSimulation::from_parts(parts).is_err());
        let mut parts = paced(config(0)).to_parts();
        parts.clock.set_scale(crate::TimeScale::X10);
        assert!(SliceSimulation::from_parts(parts).is_err());
        let mut parts = paced(config(0)).to_parts();
        parts.operations.last_mut().unwrap().report.committed = u64::MAX;
        assert!(SliceSimulation::from_parts(parts).is_err());
    }
}
