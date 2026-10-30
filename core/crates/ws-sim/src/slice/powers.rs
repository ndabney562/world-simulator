//! ADR 0038 reversible slice powers. Bless lives only in history and the
//! consequence index; storm and gathering episodes follow (#125).
use super::illness::{Group, Rejection};
use super::*;
use ws_events::{Actor, Cause, Confidence, EventId, Relation, Role, Target};
use ws_world::slice::vitals::Life;

/// Producer system for bless effects (plan decision D4).
const BLESS_SYSTEM: u16 = 84;

/// What a validated bless will do at the current committed boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Blessing {
    /// End the active illness episode now and restore health.
    Cure,
    /// Zero a healthy resident's four need deficits.
    Relieve,
    /// A healthy resident already at zero deficits: root only, `affected = 0`.
    Noop,
}

impl SliceSimulation {
    /// Deterministic semantic validation shared by execution and stop-fence replay.
    pub(super) fn bless_preflight(&self, resident: EntityId) -> Result<Blessing, SliceError> {
        let reject = SliceError::Rejected;
        self.population
            .residents()
            .get(resident)
            .ok_or(reject(Rejection::Target))?;
        let vital = self.vitals.get(resident).ok_or(reject(Rejection::Target))?;
        if vital.life() != Life::Alive {
            return Err(reject(Rejection::Dead));
        }
        if self.illness.episode(resident).is_some_and(|e| !e.recovered) {
            return Ok(Blessing::Cure);
        }
        let needs = self
            .behavior
            .common(resident, self.clock.now())
            .map_err(SliceError::Behavior)?
            .needs;
        Ok(if needs.values() == [0; 4] {
            Blessing::Noop
        } else {
            Blessing::Relieve
        })
    }

    /// Bless (ADR 0038 §2) at the current committed boundary. One root, plus one
    /// `Recovered` (cure) or one `NeedsRelieved` (relief), or nothing else for a
    /// no-op. All fallible work precedes the first mutation.
    pub(super) fn bless(&mut self, resident: EntityId) -> Result<(), SliceError> {
        let blessing = self.bless_preflight(resident)?;
        let time = self.clock.now();
        self.illness
            .groups
            .try_reserve(1)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let first = self.events.counters().map_err(SliceError::History)?.0;
        let root = EventId(first);
        let sequence = self.operations.len() as u64;
        let payload = Payload::BlessApplied {
            resident,
            sequence,
            affected: u32::from(blessing != Blessing::Noop),
        };
        let mut group =
            Group::for_root(root, time, payload)?.ok_or(SliceError::History(EventError::Order))?;
        let mut facts = Vec::new();
        facts
            .try_reserve(2)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let mut c = Candidate::new(time, 10, sequence, payload, Origin::Command(sequence));
        c.actors
            .push(Actor {
                id: resident,
                role: Role::Subject,
            })
            .map_err(SliceError::History)?;
        facts.push(c);
        let detail = self
            .behavior
            .common(resident, time)
            .map_err(SliceError::Behavior)?
            .detail;
        let mut behavior = None;
        match blessing {
            Blessing::Noop => {}
            Blessing::Cure => {
                let illness = self
                    .illness
                    .episode(resident)
                    .ok_or(SliceError::Lod(LodError::Invariant))?
                    .illness;
                facts.push(effect(
                    time,
                    Payload::Recovered {
                        resident,
                        health: 100,
                    },
                    detail,
                    resident,
                    Some(illness),
                )?);
            }
            Blessing::Relieve => {
                let mut prepared = self.behavior.prepare(time).map_err(SliceError::Behavior)?;
                prepared.relieve(resident).map_err(SliceError::Behavior)?;
                behavior = Some(prepared);
                facts.push(effect(
                    time,
                    Payload::NeedsRelieved { resident },
                    detail,
                    resident,
                    None,
                )?);
            }
        }
        for i in 1..facts.len() {
            group.append(EventId(first + i as u64))?;
        }
        self.events
            .prepare(self.clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        if let Some(prepared) = behavior {
            prepared.commit();
        }
        if blessing == Blessing::Cure {
            let e = self
                .illness
                .episode_mut(resident)
                .expect("validated active episode");
            // The natural recovery at the original end must not fire again; the
            // ADR 0034 home hold and any pending return-home stay as they are.
            e.end = time;
            e.recovered = true;
            assert!(self.vitals.set_living_health(resident, 100));
        }
        self.illness.groups.push(group);
        Ok(())
    }
}

/// One bless effect, `CausedBy/Direct` the in-batch root; a cure also links
/// `ContributedTo/Direct` the episode's `LifeIllness`, so `root_intervention`
/// (which prefers `CausedBy`) places it under the bless root only.
fn effect(
    time: SimTime,
    payload: Payload,
    detail: Detail,
    resident: EntityId,
    illness: Option<EventId>,
) -> Result<Candidate, SliceError> {
    let mut c = Candidate::new(
        time,
        BLESS_SYSTEM,
        resident.raw(),
        payload,
        Origin::System(BLESS_SYSTEM),
    );
    c.lod = detail as u8;
    c.actors
        .push(Actor {
            id: resident,
            role: Role::Subject,
        })
        .map_err(SliceError::History)?;
    c.causes
        .push(Cause {
            target: Target::Local(0),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .map_err(SliceError::History)?;
    if let Some(illness) = illness {
        c.causes
            .push(Cause {
                target: Target::Id(illness),
                relation: Relation::ContributedTo,
                confidence: Confidence::Direct,
            })
            .map_err(SliceError::History)?;
    }
    Ok(c)
}
