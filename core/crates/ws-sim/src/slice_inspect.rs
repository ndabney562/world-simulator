//! ADR0032 pure bounded real-slice inspection, independent of rendering or FFI.
use super::{SliceError, SliceSimulation};
use crate::{EntityId, SimTime};
use ws_events::RecentEvents;
pub use ws_life::{Location, Place};
use ws_life::{Schedule, behavior::CommonSnapshot, lod::BuildingSummary};
pub use ws_world::slice::vitals::{Life, Sex, VitalState};
use ws_world::slice::{SourceBuilding, population::SyntheticResident};

/// Existing authoritative identity/state, borrowed from one committed boundary.
#[derive(Debug)]
pub enum Entity<'a> {
    /// Retained resident, without reconstructing any fine geometry.
    Resident {
        /// Immutable synthetic identity.
        identity: &'a SyntheticResident,
        /// Stored sex, health and life state.
        vitals: &'a VitalState,
        /// Current needs/action/logical location and sole membership.
        state: CommonSnapshot,
        /// Immutable shifted weekday/weekend model timetable.
        schedule: &'a Schedule,
    },
    /// One of the four simulated buildings, never a context BIN.
    Building {
        /// Exact registered source row.
        identity: &'a SourceBuilding,
        /// Exact occupancy/activity/governing summary.
        summary: BuildingSummary,
    },
}
/// One pure complete record; queries neither advance nor append history.
#[derive(Debug)]
pub struct Inspection<'a> {
    /// Selected stable simulated identity.
    pub id: EntityId,
    /// Committed physical tick.
    pub tick: u64,
    /// Committed uniform simulation instant.
    pub time: SimTime,
    /// Canonical hash at this boundary.
    pub state_hash: u64,
    /// Dataset shared with geometry/sky.
    pub dataset: u64,
    /// Committed host pacing scale.
    pub scale: u32,
    /// Explicit model-week epoch, independent of civil DST.
    pub schedule_epoch: SimTime,
    /// Kind-specific authoritative values.
    pub entity: Entity<'a>,
    /// At most sixteen actual actor facts with an explicit more-results flag.
    pub events: RecentEvents<'a>,
}
impl SliceSimulation {
    /// Inspect an existing resident or featured building without materializing it.
    /// # Errors
    /// Unknown target, violated domain invariant or bounded result reservation failure.
    pub fn inspect(&self, id: EntityId) -> Result<Inspection<'_>, SliceError> {
        let entity = if let Some(identity) = self.population().residents().get(id) {
            Entity::Resident {
                identity,
                vitals: self.vitals().get(id).ok_or(SliceError::Resource)?,
                state: self
                    .behavior()
                    .common(id, self.clock().now())
                    .map_err(SliceError::Behavior)?,
                schedule: self
                    .schedules()
                    .get(id)
                    .ok_or(SliceError::Lod(ws_life::lod::LodError::Invariant))?,
            }
        } else if let Some(identity) = self.population().world().buildings().get(id) {
            Entity::Building {
                identity,
                summary: self
                    .view()?
                    .summary
                    .buildings
                    .into_iter()
                    .find(|row| row.building == id)
                    .ok_or(SliceError::Lod(ws_life::lod::LodError::Invariant))?,
            }
        } else {
            return Err(SliceError::Lod(ws_life::lod::LodError::Input));
        };
        Ok(Inspection {
            id,
            tick: self.clock().steps(),
            time: self.clock().now(),
            state_hash: self.state_hash(),
            dataset: ws_world::slice::DATASET_VERSION,
            scale: self.clock().scale().multiplier(),
            schedule_epoch: self.schedules().epoch(),
            entity,
            events: self
                .events()
                .recent(id, self.clock().now())
                .map_err(SliceError::History)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slice::{Operation, SliceConfig};
    #[test]
    fn every_identity_matches_independent_domain_dump_at_weekday_weekend_and_negative_time() {
        for seed in [0, 42, 71449] {
            for time in [
                -86_400_000_000,
                0,
                30_000_000_000,
                5 * 86_400_000_000,
                6 * 86_400_000_000,
            ] {
                let world = SliceSimulation::new(SliceConfig {
                    seed,
                    start: SimTime::from_micros_since_epoch(time),
                    schedule_epoch: SimTime::EPOCH,
                })
                .unwrap();
                for (id, identity) in world.population().residents().iter() {
                    let r = world.inspect(id).unwrap();
                    assert_eq!(r.id, id);
                    assert_eq!(r.state_hash, world.state_hash());
                    let Entity::Resident {
                        identity: got,
                        state,
                        schedule,
                        vitals,
                    } = r.entity
                    else {
                        panic!("wrong kind")
                    };
                    assert_eq!(got, identity);
                    assert_eq!(Some(vitals), world.vitals().get(id));
                    assert_eq!(
                        state,
                        world.behavior().common(id, world.clock().now()).unwrap()
                    );
                    assert_eq!(schedule, world.schedules().get(id).unwrap());
                    assert_eq!(
                        state.logical,
                        schedule.at(world.schedules().epoch(), world.clock().now())
                    );
                }
                for (id, identity) in world.population().world().buildings().iter() {
                    let r = world.inspect(id).unwrap();
                    let Entity::Building {
                        identity: got,
                        summary,
                    } = r.entity
                    else {
                        panic!("wrong kind")
                    };
                    assert_eq!(got, identity);
                    assert_eq!(
                        summary,
                        world
                            .view()
                            .unwrap()
                            .summary
                            .buildings
                            .into_iter()
                            .find(|b| b.building == id)
                            .unwrap()
                    );
                }
                for id in world
                    .population()
                    .households()
                    .ids()
                    .iter()
                    .chain(world.population().workplaces().ids())
                {
                    assert!(world.inspect(*id).is_err());
                }
            }
        }
    }
    #[test]
    fn repeated_success_and_failure_queries_preserve_checkpoint_logs_membership_and_future_replay()
    {
        let mut world = SliceSimulation::new(SliceConfig {
            seed: 71449,
            start: SimTime::EPOCH,
            schedule_epoch: SimTime::EPOCH,
        })
        .unwrap();
        let mut control = world.clone();
        let checkpoint = world.checkpoint();
        let digest = world.operation_digest();
        let ids = world.population().residents().ids().to_vec();
        for _ in 0..20 {
            for id in &ids {
                let record = world.inspect(*id).unwrap();
                assert_eq!(record.tick, 0);
            }
            assert!(world.inspect(EntityId::spawned(99999).unwrap()).is_err());
        }
        assert_eq!(world.checkpoint(), checkpoint);
        assert_eq!(world.operation_digest(), digest);
        assert!(world.operations().is_empty());
        world.apply(Operation::Steps(1000)).unwrap();
        control.apply(Operation::Steps(1000)).unwrap();
        assert_eq!(world.checkpoint(), control.checkpoint());
        assert_eq!(world.operation_digest(), control.operation_digest());
        assert_eq!(world.view().unwrap(), control.view().unwrap());
    }
}
