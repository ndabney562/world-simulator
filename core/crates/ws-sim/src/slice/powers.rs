//! ADR 0038 reversible slice powers. Bless lives only in history and the
//! consequence index; the storm is a block-level episode; the gathering
//! follows (#125 Task 6).
use super::illness::{Group, Rejection, excluded};
use super::*;
use ws_events::{Actor, Cause, Confidence, EventId, Relation, Role, Target};
use ws_life::behavior::{Deferral, Prepared, StormWindow};
use ws_world::slice::vitals::Life;

/// Producer system for bless effects (plan decision D4).
const BLESS_SYSTEM: u16 = 84;
/// Producer system for storm effects (plan decision D4).
const STORM_SYSTEM: u16 = 85;
/// Shortest and longest storm, inclusive, on the 100-ms grid (ADR 0038 §1).
const STORM_DURATION_US: core::ops::RangeInclusive<u64> = 100_000..=3_600_000_000;

/// Block-level storm episode (ADR 0038 §3). Present from the cast until the
/// tick at or after `end` that records `StormKeptInside` and `StormEnded`.
/// `ws-life` keeps its own window after that until every hold has rejoined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storm {
    /// The `StormApplied` root.
    pub root: EventId,
    /// Cast time.
    pub start: SimTime,
    /// Exclusive end.
    pub end: SimTime,
    /// `StayedInside` events recorded so far under this storm.
    pub kept: u32,
}

/// Block-level power episodes in canonical slice state. The gathering joins
/// in #125 Task 6; `busy` and `active` are its hooks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Powers {
    pub(super) storm: Option<Storm>,
}
impl Powers {
    /// Whether the powers hash enters `state_hash`. False whenever no episode
    /// is active, so worlds that never used a block power keep their goldens.
    pub(super) fn active(&self) -> bool {
        self.storm.is_some()
    }
    /// ADR 0038 §1: a storm or a gathering is in progress.
    fn busy(&self) -> bool {
        self.storm.is_some()
    }
    /// Canonical words of every block-level episode.
    pub(super) fn hash(&self) -> u64 {
        let mut h = SipHasher24::new(0, Purpose::new("ws-sim/slice-powers/v1").value());
        match self.storm {
            Some(s) => h.write_words(&[
                1,
                s.root.0,
                s.start.as_micros_since_epoch() as u64,
                s.end.as_micros_since_epoch() as u64,
                u64::from(s.kept),
            ]),
            None => h.write_u64(0),
        }
        h.finish()
    }
}

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
            .try_reserve(2 + self.lod.nodes().len())
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
        let mut lod = None;
        let mut powers = self.powers;
        match blessing {
            Blessing::Noop => {}
            Blessing::Cure => {
                let e = self
                    .illness
                    .episode(resident)
                    .ok_or(SliceError::Lod(LodError::Invariant))?;
                let (illness, episode_root) = (e.illness, e.root);
                // ADR 0034 recovery, brought forward: the home hold ends now and
                // the resident rejoins the baseline by the continuity rule. A
                // started homeward leg and its pending return-home are kept.
                let mut prepared = self.behavior.prepare(time).map_err(SliceError::Behavior)?;
                prepared
                    .end_illness(resident, episode_root.0)
                    .map_err(SliceError::Behavior)?;
                let plan = self
                    .lod
                    .prepare(&mut prepared, self.clock.scale().multiplier())
                    .map_err(lod_error)?;
                facts.extend(
                    plan.changes()
                        .enumerate()
                        .map(|(i, c)| c.fact(time, i as u64)),
                );
                // A rejoin due now under a storm may defer a departure at once.
                facts.extend(stayed_inside(&mut powers, &prepared, time)?);
                behavior = Some(prepared);
                lod = Some(plan);
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
        // The root (system 10) stays first, so the effects' Local(0) link holds.
        facts.sort_unstable_by_key(|c| (c.system, c.sequence));
        for (i, c) in facts.iter().enumerate().skip(1) {
            if c.system != 70 && !excluded(&c.payload) {
                group.append(EventId(first + i as u64))?;
            }
        }
        self.events
            .prepare(self.clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        if let Some(prepared) = behavior {
            prepared.commit();
        }
        if let Some(plan) = lod {
            plan.commit();
        }
        self.powers = powers;
        if blessing == Blessing::Cure {
            let e = self
                .illness
                .episode_mut(resident)
                .expect("validated active episode");
            // The natural recovery at the original end must not fire again.
            e.end = time;
            e.recovered = true;
            assert!(self.vitals.set_living_health(resident, 100));
        }
        self.illness.groups.push(group);
        Ok(())
    }
}

impl SliceSimulation {
    /// The active storm, if any; persisted by `ws-persist` (#125 Task 7).
    pub fn storm(&self) -> Option<&Storm> {
        self.powers.storm.as_ref()
    }
    /// Deterministic semantic validation shared by execution and stop-fence
    /// replay. Returns the storm's end.
    pub(super) fn storm_preflight(&self, duration_us: u64) -> Result<SimTime, SliceError> {
        let reject = SliceError::Rejected;
        if !STORM_DURATION_US.contains(&duration_us) || !duration_us.is_multiple_of(100_000) {
            return Err(reject(Rejection::Duration));
        }
        if self.powers.busy() {
            return Err(reject(Rejection::Busy));
        }
        self.clock
            .now()
            .as_micros_since_epoch()
            .checked_add(duration_us as i64)
            .map(SimTime::from_micros_since_epoch)
            .ok_or(reject(Rejection::Deadline))
    }
    /// Storm (ADR 0038 §3) at the current committed boundary: one root, the
    /// episode and the `ws-life` window. The window is staged after preparing
    /// this boundary, so a departure the baseline already started at the cast
    /// instant is not deferred retroactively; holds begin at the next
    /// re-evaluation. All fallible work precedes the first mutation.
    pub(super) fn cast_storm(&mut self, duration_us: u64) -> Result<(), SliceError> {
        let end = self.storm_preflight(duration_us)?;
        let time = self.clock.now();
        self.illness
            .groups
            .try_reserve(1)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let first = self.events.counters().map_err(SliceError::History)?.0;
        let root = EventId(first);
        let sequence = self.operations.len() as u64;
        let payload = Payload::StormApplied {
            sequence,
            duration_us,
        };
        let group =
            Group::for_root(root, time, payload)?.ok_or(SliceError::History(EventError::Order))?;
        let mut prepared = self.behavior.prepare(time).map_err(SliceError::Behavior)?;
        // No storm is active, so this boundary can defer nothing.
        if !prepared.deferrals().is_empty() {
            return Err(SliceError::Lod(LodError::Invariant));
        }
        prepared.set_storm(Some(StormWindow { start: time, end }));
        let mut facts = Vec::new();
        facts
            .try_reserve(1 + self.lod.nodes().len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let plan = self
            .lod
            .prepare(&mut prepared, self.clock.scale().multiplier())
            .map_err(lod_error)?;
        facts.push(Candidate::new(
            time,
            10,
            sequence,
            payload,
            Origin::Command(sequence),
        ));
        facts.extend(
            plan.changes()
                .enumerate()
                .map(|(i, c)| c.fact(time, i as u64)),
        );
        facts.sort_unstable_by_key(|c| (c.system, c.sequence));
        self.events
            .prepare(self.clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        prepared.commit();
        plan.commit();
        self.powers.storm = Some(Storm {
            root,
            start: time,
            end,
            kept: 0,
        });
        self.illness.groups.push(group);
        Ok(())
    }
}

/// One `StayedInside` per new deferral in `behavior`, in its ascending
/// resident order, each `ContributedTo` the storm root by committed ID and
/// counted in `powers`. Deferrals without an active storm are an invariant
/// violation.
fn stayed_inside(
    powers: &mut Powers,
    behavior: &Prepared<'_>,
    time: SimTime,
) -> Result<Vec<Candidate>, SliceError> {
    let deferrals = behavior.deferrals();
    let mut facts = Vec::new();
    if deferrals.is_empty() {
        return Ok(facts);
    }
    let storm = powers
        .storm
        .as_mut()
        .ok_or(SliceError::Lod(LodError::Invariant))?;
    facts
        .try_reserve(deferrals.len() + 2)
        .map_err(|_| SliceError::History(EventError::Resource))?;
    for &Deferral {
        resident,
        building,
        activity,
    } in deferrals
    {
        let detail = behavior
            .common(resident)
            .map_err(SliceError::Behavior)?
            .detail;
        let mut c = storm_fact(
            time,
            resident.raw(),
            Payload::StayedInside {
                resident,
                building,
                activity,
            },
            storm.root,
            Relation::ContributedTo,
        )?;
        c.lod = detail as u8;
        for (id, role) in [(resident, Role::Subject), (building, Role::Object)] {
            c.actors
                .push(Actor { id, role })
                .map_err(SliceError::History)?;
        }
        facts.push(c);
        storm.kept = storm
            .kept
            .checked_add(1)
            .ok_or(SliceError::History(EventError::Limit))?;
    }
    Ok(facts)
}
/// One tick of the storm: record this tick's deferrals, and at the first
/// tick at or after `end` close the group with `StormKeptInside` and
/// `StormEnded`, both `CausedBy` the root. Returns the next powers state,
/// published only after the tick commits.
pub(super) fn storm_tick(
    mut powers: Powers,
    events: &EventStore,
    behavior: &Prepared<'_>,
    time: SimTime,
) -> Result<(Powers, Vec<Candidate>), SliceError> {
    let mut facts = stayed_inside(&mut powers, behavior, time)?;
    let Some(storm) = powers.storm.filter(|s| time >= s.end) else {
        return Ok((powers, facts));
    };
    let Some(Payload::StormApplied { sequence, .. }) =
        events.get(storm.root).map(|e| e.record.payload)
    else {
        return Err(SliceError::Lod(LodError::Invariant));
    };
    facts
        .try_reserve(2)
        .map_err(|_| SliceError::History(EventError::Resource))?;
    // After every per-resident row of this tick, kept before ended.
    facts.push(storm_fact(
        time,
        u64::MAX - 1,
        Payload::StormKeptInside { count: storm.kept },
        storm.root,
        Relation::CausedBy,
    )?);
    facts.push(storm_fact(
        time,
        u64::MAX,
        Payload::StormEnded { sequence },
        storm.root,
        Relation::CausedBy,
    )?);
    powers.storm = None;
    Ok((powers, facts))
}

/// One storm effect from system 85, linked to the committed root by ID.
fn storm_fact(
    time: SimTime,
    sequence: u64,
    payload: Payload,
    root: EventId,
    relation: Relation,
) -> Result<Candidate, SliceError> {
    let mut c = Candidate::new(
        time,
        STORM_SYSTEM,
        sequence,
        payload,
        Origin::System(STORM_SYSTEM),
    );
    c.causes
        .push(Cause {
            target: Target::Id(root),
            relation,
            confidence: Confidence::Direct,
        })
        .map_err(SliceError::History)?;
    Ok(c)
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
