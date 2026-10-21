//! ADR0034 finite episodes and bounded producer-defined causal groups.
use super::*;
use ws_events::{Actor, Cause, Confidence, Event, EventId, Relation, Role, Target};
use ws_life::{Activity, Location};
use ws_world::slice::vitals::Life;

/// Illness policy version, independent of baseline schedules and vitals.
pub const VERSION: u32 = 1;
/// Maximum produced records per intervention; never silently truncate.
pub const MAX_CONSEQUENCES: usize = 16;
/// Typed semantic command rejection, replayable before later operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Rejection {
    /// Missing or nonresident target.
    Target,
    /// Target is not alive.
    Dead,
    /// Severity outside 1 through 10.
    Severity,
    /// Duration outside policy bounds or off the physical grid.
    Duration,
    /// Episode deadline outside SimTime.
    Deadline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Episode {
    resident: EntityId,
    root: EventId,
    illness: EventId,
    severity: u32,
    start: SimTime,
    end: SimTime,
    return_at: Option<i128>,
    recovered: bool,
    returned: bool,
    missed: u8,
    visited: bool,
}
/// Fixed-size index row. Derived solely from committed typed history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    /// Intervention root.
    pub root: EventId,
    /// Target resident.
    pub subject: EntityId,
    /// Requested severity.
    pub severity: u32,
    /// Applied occurrence time.
    pub applied: SimTime,
    /// None for an accepted duplicate no-op.
    pub end: Option<SimTime>,
    ids: [EventId; MAX_CONSEQUENCES],
    len: usize,
}
impl Group {
    /// At most sixteen retained produced records in ascending EventId order.
    pub fn ids(&self) -> &[EventId] {
        &self.ids[..self.len]
    }
    fn append(&mut self, id: EventId) -> Result<(), SliceError> {
        if self.len == MAX_CONSEQUENCES || self.ids().last().is_some_and(|v| *v >= id) {
            return Err(SliceError::History(EventError::Limit));
        }
        self.ids[self.len] = id;
        self.len += 1;
        Ok(())
    }
}
#[derive(Debug, Clone, Default)]
pub(super) struct Illness {
    episodes: Vec<Episode>,
    groups: Vec<Group>,
}
/// Current authoritative inspection extension; no query changes the episode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    /// Routine kind: 0 none, 1 illness/home hold, 2 visit.
    pub override_kind: u32,
    /// Whether health is currently affected.
    pub active: bool,
    /// Producing root for a current routine, zero without one.
    pub override_root: u64,
    /// Most recent intervention that actually affected this resident.
    pub own_root: u64,
    /// Active deadline, None after recovery.
    pub end: Option<SimTime>,
}
impl Illness {
    fn episode(&self, id: EntityId) -> Option<&Episode> {
        self.episodes
            .binary_search_by_key(&id, |e| e.resident)
            .ok()
            .map(|i| &self.episodes[i])
    }
    pub(super) fn has_episodes(&self) -> bool {
        !self.episodes.is_empty()
    }
    pub(super) fn hash(&self) -> u64 {
        let mut h = SipHasher24::new(0, Purpose::new("ws-sim/slice-illness/v1").value());
        h.write_u64(u64::from(VERSION));
        for e in &self.episodes {
            h.write_words(&[
                e.resident.raw(),
                e.root.0,
                e.illness.0,
                u64::from(e.severity),
                e.start.as_micros_since_epoch() as u64,
                e.end.as_micros_since_epoch() as u64,
                u64::from(e.return_at.is_some()),
                e.return_at.unwrap_or(0) as u64,
                (e.return_at.unwrap_or(0) >> 64) as u64,
                u64::from(e.recovered),
                u64::from(e.returned),
                u64::from(e.missed),
                u64::from(e.visited),
            ]);
        }
        h.finish()
    }
    pub(super) fn due(&self, time: SimTime, minute: bool, schedule_due: bool) -> bool {
        self.episodes.iter().any(|e| {
            (!e.recovered && time >= e.end)
                || (!e.returned
                    && e.return_at
                        .is_some_and(|t| i128::from(time.as_micros_since_epoch()) >= t))
                || (!e.recovered && minute)
                || schedule_due
        })
    }
    fn copied_episodes(&self) -> Result<Vec<Episode>, SliceError> {
        let mut out = Vec::new();
        out.try_reserve(self.episodes.len() + 1)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        out.extend_from_slice(&self.episodes);
        Ok(out)
    }
    pub(super) fn prepare_tick(
        &self,
        behavior: &mut ws_life::behavior::Prepared<'_>,
        population: &PopulatedSlice,
        schedules: &Schedules,
        vitals: &ws_world::slice::vitals::Vitals,
        minute: bool,
        schedule_due: bool,
    ) -> Result<Plan, SliceError> {
        let time = behavior.time();
        if !self.due(time, minute, schedule_due) {
            return Ok(Plan::default());
        }
        let mut episodes = self.copied_episodes()?;
        let mut facts = Vec::new();
        facts
            .try_reserve(episodes.len() * 4)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let mut health = Vec::new();
        health
            .try_reserve(episodes.len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        for e in &mut episodes {
            let state = behavior.common(e.resident).map_err(SliceError::Behavior)?;
            let home = population
                .residents()
                .get(e.resident)
                .ok_or(SliceError::Lod(LodError::Invariant))?
                .home;
            if !e.returned
                && e.return_at
                    .is_some_and(|t| i128::from(time.as_micros_since_epoch()) >= t)
            {
                facts.push(fact(
                    time,
                    (50, e.resident.raw()),
                    Payload::ReturnedHome {
                        resident: e.resident,
                        home,
                    },
                    state.detail,
                    e.illness,
                    (Relation::ContributedTo, Confidence::Direct),
                    &[e.resident, home],
                )?);
                e.returned = true;
            }
            if !e.recovered && time >= e.end {
                facts.push(fact(
                    time,
                    (82, e.resident.raw()),
                    Payload::Recovered {
                        resident: e.resident,
                        health: 100,
                    },
                    state.detail,
                    e.illness,
                    (Relation::CausedBy, Confidence::Direct),
                    &[e.resident],
                )?);
                health.push((e.resident, 100));
                e.recovered = true;
            }
            if schedule_due
                && behavior
                    .routine(e.resident)
                    .map_err(SliceError::Behavior)?
                    .is_some_and(|r| r.kind == 1 && r.root == e.root.0)
            {
                let profile = schedules
                    .get(e.resident)
                    .ok_or(SliceError::Lod(LodError::Invariant))?;
                let baseline = profile.at(schedules.epoch(), time);
                let before = time
                    .as_micros_since_epoch()
                    .checked_sub(1)
                    .map(SimTime::from_micros_since_epoch);
                if matches!(baseline.activity, Activity::Work | Activity::School)
                    && before.is_some_and(|t| {
                        profile.at(schedules.epoch(), t).activity != baseline.activity
                    })
                    && state.logical != baseline
                {
                    facts.push(missed(
                        time,
                        e,
                        state.detail,
                        baseline.destination(),
                        baseline.activity,
                    )?);
                }
            }
            if !e.recovered
                && !e.visited
                && minute
                && state.detail == Detail::L1
                && state.logical.location == Location::AtBuilding(home)
                && state.logical.activity == Activity::Home
            {
                let Some(until) = time
                    .as_micros_since_epoch()
                    .checked_add(300_000_000)
                    .map(SimTime::from_micros_since_epoch)
                else {
                    continue;
                };
                let patient = population
                    .residents()
                    .get(e.resident)
                    .ok_or(SliceError::Lod(LodError::Invariant))?;
                for (id, visitor) in population.residents().iter() {
                    if id == e.resident
                        || visitor.home != home
                        || visitor.household == patient.household
                        || behavior
                            .routine(id)
                            .map_err(SliceError::Behavior)?
                            .is_some()
                    {
                        continue;
                    }
                    let v = behavior.common(id).map_err(SliceError::Behavior)?;
                    let living = vitals
                        .get(id)
                        .is_some_and(|v| v.life() == Life::Alive && v.health() == 100)
                        && !episodes_active(&self.episodes, id, time);
                    if !living
                        || v.detail != Detail::L1
                        || v.logical.activity != Activity::Home
                        || v.logical.location != Location::AtBuilding(home)
                    {
                        continue;
                    }
                    let profile = schedules
                        .get(id)
                        .ok_or(SliceError::Lod(LodError::Invariant))?;
                    if profile
                        .next_change(schedules.epoch(), time)
                        .is_some_and(|t| t < until)
                    {
                        continue;
                    }
                    behavior
                        .visit(id, home, until, e.root.0)
                        .map_err(SliceError::Behavior)?;
                    facts.push(fact(
                        time,
                        (81, e.resident.raw()),
                        Payload::NeighbourCheckIn {
                            visitor: id,
                            patient: e.resident,
                            home,
                            intended_duration_us: 300_000_000,
                        },
                        Detail::L1,
                        e.illness,
                        (Relation::ContributedTo, Confidence::Contributing),
                        &[id, e.resident, home],
                    )?);
                    e.visited = true;
                    break;
                }
            }
        }
        Ok(Plan {
            episodes: Some(episodes),
            facts,
            health,
        })
    }
    pub(super) fn stage_index(
        &self,
        facts: &[Candidate],
        first: u64,
    ) -> Result<Vec<(usize, Group)>, SliceError> {
        let mut updates: Vec<(usize, Group)> = Vec::new();
        updates
            .try_reserve(facts.len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        for (offset, c) in facts.iter().enumerate() {
            let Some(Target::Id(cause)) = c.causes.iter().next().map(|c| c.target) else {
                continue;
            };
            let Some(e) = self.episodes.iter().find(|e| e.illness == cause) else {
                continue;
            };
            let i = self
                .groups
                .binary_search_by_key(&e.root, |g| g.root)
                .map_err(|_| SliceError::History(EventError::Resource))?;
            let at = match updates.iter().position(|(j, _)| *j == i) {
                Some(at) => at,
                None => {
                    updates.push((i, self.groups[i]));
                    updates.len() - 1
                }
            };
            updates[at].1.append(EventId(first + offset as u64))?;
        }
        Ok(updates)
    }
    pub(super) fn commit(
        &mut self,
        plan: Plan,
        updates: Vec<(usize, Group)>,
        vitals: &mut ws_world::slice::vitals::Vitals,
    ) {
        if let Some(e) = plan.episodes {
            self.episodes = e;
        }
        for (id, health) in plan.health {
            assert!(vitals.set_living_health(id, health));
        }
        for (i, g) in updates {
            self.groups[i] = g;
        }
    }
}
fn episodes_active(episodes: &[Episode], id: EntityId, time: SimTime) -> bool {
    episodes.iter().any(|e| e.resident == id && time < e.end)
}
#[derive(Default)]
pub(super) struct Plan {
    episodes: Option<Vec<Episode>>,
    pub(super) facts: Vec<Candidate>,
    health: Vec<(EntityId, u8)>,
}
fn fact(
    time: SimTime,
    producer: (u16, u64),
    payload: Payload,
    detail: Detail,
    parent: EventId,
    claim: (Relation, Confidence),
    actors: &[EntityId],
) -> Result<Candidate, SliceError> {
    let (system, sequence) = producer;
    let (relation, confidence) = claim;
    let mut c = Candidate::new(time, system, sequence, payload, Origin::System(system));
    c.lod = detail as u8;
    for (i, id) in actors.iter().enumerate() {
        c.actors
            .push(Actor {
                id: *id,
                role: if i == 0 { Role::Subject } else { Role::Object },
            })
            .map_err(SliceError::History)?;
    }
    c.causes
        .push(Cause {
            target: Target::Id(parent),
            relation,
            confidence,
        })
        .map_err(SliceError::History)?;
    Ok(c)
}
fn missed(
    time: SimTime,
    e: &mut Episode,
    detail: Detail,
    destination: EntityId,
    activity: Activity,
) -> Result<Candidate, SliceError> {
    e.missed = e
        .missed
        .checked_add(1)
        .filter(|v| *v <= 2)
        .ok_or(SliceError::History(EventError::Limit))?;
    fact(
        time,
        (80, e.resident.raw()),
        Payload::MissedDuty {
            resident: e.resident,
            destination,
            activity,
        },
        detail,
        e.illness,
        (Relation::CausedBy, Confidence::Direct),
        &[e.resident, destination],
    )
}
impl SliceSimulation {
    /// Pure bounded illness extension to the selected resident's state.
    pub fn illness_state(&self, id: EntityId) -> Result<State, SliceError> {
        let routine = self.behavior.routine(id).map_err(SliceError::Behavior)?;
        let e = self.illness.episode(id);
        Ok(State {
            override_kind: routine.map_or(0, |r| r.kind),
            override_root: routine.map_or(0, |r| r.root),
            active: e.is_some_and(|e| !e.recovered),
            own_root: e.map_or(0, |e| e.root.0),
            end: e.filter(|e| !e.recovered).map(|e| e.end),
        })
    }
    /// Binary-searches one root; at most sixteen subsequent event lookups.
    pub fn consequences(&self, root: EventId) -> Result<(&Group, Vec<&Event>), SliceError> {
        let i = self
            .illness
            .groups
            .binary_search_by_key(&root, |g| g.root)
            .map_err(|_| SliceError::Rejected(Rejection::Target))?;
        let g = &self.illness.groups[i];
        let mut events = Vec::new();
        events
            .try_reserve_exact(g.len)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        for id in g.ids() {
            events.push(
                self.events
                    .get(*id)
                    .ok_or(SliceError::History(EventError::Cause))?,
            );
        }
        Ok((g, events))
    }
    /// Allocated derived-index bytes; grows with retained intervention history.
    pub fn consequence_index_capacity_bytes(&self) -> usize {
        self.illness.groups.capacity() * core::mem::size_of::<Group>()
    }
    /// Reconstructs only the derived producer index from typed roots and explicit links.
    /// Canonical episodes, health, clocks and history remain untouched.
    pub fn rebuild_consequence_index(&mut self) -> Result<(), SliceError> {
        let mut groups: Vec<Group> = Vec::new();
        groups
            .try_reserve(
                self.events
                    .records()
                    .iter()
                    .filter(|e| matches!(e.record.payload, Payload::SliceIllnessApplied { .. }))
                    .count(),
            )
            .map_err(|_| SliceError::History(EventError::Resource))?;
        for event in self.events.records() {
            if let Payload::SliceIllnessApplied {
                resident,
                severity,
                duration_us,
                affected,
                ..
            } = event.record.payload
            {
                let end = if affected == 0 {
                    None
                } else {
                    Some(SimTime::from_micros_since_epoch(
                        event
                            .record
                            .time
                            .as_micros_since_epoch()
                            .checked_add(duration_us as i64)
                            .ok_or(SliceError::History(EventError::Order))?,
                    ))
                };
                let mut g = Group {
                    root: event.id,
                    subject: resident,
                    severity,
                    applied: event.record.time,
                    end,
                    ids: [EventId(0); MAX_CONSEQUENCES],
                    len: 0,
                };
                g.append(event.id)?;
                groups.push(g);
            } else if matches!(
                event.record.payload,
                Payload::LifeIllness { .. }
                    | Payload::ReturnedHome { .. }
                    | Payload::MissedDuty { .. }
                    | Payload::NeighbourCheckIn { .. }
                    | Payload::Recovered { .. }
            ) && let Some(root) = self
                .events
                .root_intervention(event.id)
                .filter(|e| matches!(e.record.payload, Payload::SliceIllnessApplied { .. }))
            {
                let i = groups
                    .binary_search_by_key(&root.id, |g| g.root)
                    .map_err(|_| SliceError::History(EventError::Cause))?;
                groups[i].append(event.id)?;
            }
        }
        self.illness.groups = groups;
        Ok(())
    }
    pub(super) fn afflict(
        &mut self,
        resident: EntityId,
        severity: u32,
        duration_us: u64,
    ) -> Result<(), SliceError> {
        let reject = SliceError::Rejected;
        let identity = self
            .population
            .residents()
            .get(resident)
            .ok_or(reject(Rejection::Target))?;
        let home = identity.home;
        let vital = self.vitals.get(resident).ok_or(reject(Rejection::Target))?;
        if vital.life() != Life::Alive {
            return Err(reject(Rejection::Dead));
        }
        if !(1..=10).contains(&severity) {
            return Err(reject(Rejection::Severity));
        }
        if !(100_000..=86_400_000_000).contains(&duration_us)
            || !duration_us.is_multiple_of(100_000)
        {
            return Err(reject(Rejection::Duration));
        }
        let time = self.clock.now();
        let end = time
            .as_micros_since_epoch()
            .checked_add(duration_us as i64)
            .map(SimTime::from_micros_since_epoch)
            .ok_or(reject(Rejection::Deadline))?;
        let duplicate = self
            .illness
            .episode(resident)
            .is_some_and(|e| !e.recovered || e.return_at.is_some() && !e.returned);
        self.illness
            .groups
            .try_reserve(1)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let first = self.events.counters().map_err(SliceError::History)?.0;
        let root = EventId(first);
        let illness = EventId(first + 1);
        let sequence = self.operations.len() as u64;
        let mut group = Group {
            root,
            subject: resident,
            severity,
            applied: time,
            end: (!duplicate).then_some(end),
            ids: [EventId(0); MAX_CONSEQUENCES],
            len: 0,
        };
        let mut facts = Vec::new();
        facts
            .try_reserve(8)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let mut c = Candidate::new(
            time,
            10,
            sequence,
            Payload::SliceIllnessApplied {
                resident,
                sequence,
                severity,
                duration_us,
                affected: u32::from(!duplicate),
            },
            Origin::Command(sequence),
        );
        c.actors
            .push(Actor {
                id: resident,
                role: Role::Subject,
            })
            .map_err(SliceError::History)?;
        facts.push(c);
        if duplicate {
            group.append(root)?;
            self.events
                .prepare(self.clock.steps(), &facts)
                .map_err(SliceError::History)?
                .commit();
            self.illness.groups.push(group);
            return Ok(());
        }
        let mut episodes = self.illness.copied_episodes()?;
        let mut behavior = self.behavior.prepare(time).map_err(SliceError::Behavior)?;
        let old = behavior.common(resident).map_err(SliceError::Behavior)?;
        let routine = behavior
            .afflict(resident, home, end, root.0)
            .map_err(SliceError::Behavior)?;
        let mut e = Episode {
            resident,
            root,
            illness,
            severity,
            start: time,
            end,
            return_at: routine.homeward.map(|j| j.arrival),
            recovered: false,
            returned: routine.homeward.is_none(),
            missed: 0,
            visited: false,
        };
        let mut ill = Candidate::new(
            time,
            12,
            0,
            Payload::LifeIllness {
                resident,
                illness: 0,
                severity: severity as u8,
            },
            Origin::System(12),
        );
        ill.actors
            .push(Actor {
                id: resident,
                role: Role::Subject,
            })
            .map_err(SliceError::History)?;
        ill.causes
            .push(Cause {
                target: Target::Local(0),
                relation: Relation::CausedBy,
                confidence: Confidence::Direct,
            })
            .map_err(SliceError::History)?;
        facts.push(ill);
        if matches!(old.logical.activity, Activity::Work | Activity::School) {
            facts.push(missed(
                time,
                &mut e,
                old.detail,
                old.logical.destination(),
                old.logical.activity,
            )?);
        }
        let lod = self
            .lod
            .prepare(&mut behavior, self.clock.scale().multiplier())
            .map_err(lod_error)?;
        facts.extend(
            lod.changes()
                .enumerate()
                .map(|(i, c)| c.fact(time, i as u64)),
        );
        facts.sort_unstable_by_key(|c| (c.system, c.sequence));
        for (i, c) in facts.iter().enumerate() {
            if c.system != 70 {
                group.append(EventId(first + i as u64))?;
            }
        }
        match episodes.binary_search_by_key(&resident, |e| e.resident) {
            Ok(i) => episodes[i] = e,
            Err(i) => episodes.insert(i, e),
        }
        #[cfg(test)]
        if self.fail_after_behavior_tick == Some(self.clock.steps()) {
            return Err(SliceError::History(EventError::Resource));
        }
        self.events
            .prepare(self.clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        behavior.commit();
        lod.commit();
        assert!(
            self.vitals
                .set_living_health(resident, (100 - 9 * severity) as u8)
        );
        self.illness.episodes = episodes;
        self.illness.groups.push(group);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_and_recovery_history_faults_preserve_all_state_and_replay_prefixes() {
        let config = SliceConfig {
            seed: 42,
            start: SimTime::EPOCH,
            schedule_epoch: SimTime::EPOCH,
        };
        let mut w = SliceSimulation::new(config).unwrap();
        let id = w.population.residents().ids()[0];
        let before = w.checkpoint();
        w.fail_after_behavior_tick = Some(0);
        let report = w
            .apply(Operation::Afflict {
                resident: id,
                severity: 5,
                duration_us: 100_000,
            })
            .unwrap();
        assert_eq!(
            report.error,
            Some(SliceError::History(EventError::Resource))
        );
        assert_eq!(w.checkpoint(), before);
        assert!(w.illness.groups.is_empty());
        assert!(w.illness.episodes.is_empty());
        let (replay, stopped) = SliceSimulation::replay(config, w.operations()).unwrap();
        assert!(stopped);
        assert_eq!(w.checkpoint(), replay.checkpoint());
        assert_eq!(w.operation_digest(), replay.operation_digest());
        let mut w = SliceSimulation::new(config).unwrap();
        w.apply(Operation::Afflict {
            resident: id,
            severity: 5,
            duration_us: 100_000,
        })
        .unwrap();
        let before = w.checkpoint();
        let groups = w.illness.groups.clone();
        let episodes = w.illness.episodes.clone();
        w.fail_after_behavior_tick = Some(1);
        assert!(w.step().is_err());
        assert_eq!(w.checkpoint(), before);
        assert_eq!(w.illness.groups, groups);
        assert_eq!(w.illness.episodes, episodes);
        assert_eq!(w.vitals.get(id).unwrap().health(), 55);
        w.fail_after_behavior_tick = None;
        w.step().unwrap();
        assert_eq!(w.vitals.get(id).unwrap().health(), 100);
    }
    #[test]
    fn two_patients_compete_for_one_visitor_in_stable_patient_order() {
        let config = SliceConfig {
            seed: 42,
            start: SimTime::from_micros_since_epoch(20 * 3_600_000_000),
            schedule_epoch: SimTime::EPOCH,
        };
        let original = SliceSimulation::new(config).unwrap();
        let rows: Vec<_> = original.population.residents().iter().collect();
        let triple = rows
            .iter()
            .find_map(|(a, ra)| {
                rows.iter().find_map(|(b, rb)| {
                    rows.iter().find_map(|(c, rc)| {
                        (*a < *b
                            && *b < *c
                            && ra.home == rb.home
                            && rb.home == rc.home
                            && ra.household != rc.household
                            && rb.household != rc.household)
                            .then_some([*a, *b, *c])
                    })
                })
            })
            .expect("same building, eligible distinct visitor household");
        let run = |reverse: bool| {
            let mut w = original.clone();
            for id in triple {
                w.apply(Operation::SetDetail {
                    resident: id,
                    level: Detail::L1,
                })
                .unwrap();
            }
            let patients = if reverse {
                [triple[1], triple[0]]
            } else {
                [triple[0], triple[1]]
            };
            for resident in patients {
                assert_eq!(
                    w.apply(Operation::Afflict {
                        resident,
                        severity: 5,
                        duration_us: 600_000_000
                    })
                    .unwrap()
                    .error,
                    None
                );
            }
            w.apply(Operation::Steps(600)).unwrap();
            let visits: Vec<_> = w
                .events
                .records()
                .iter()
                .filter_map(|e| {
                    if let Payload::NeighbourCheckIn {
                        visitor, patient, ..
                    } = e.record.payload
                    {
                        Some((visitor, patient))
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(visits, vec![(triple[2], triple[0])]);
            assert_eq!(
                w.resident(triple[2]).unwrap().action,
                ws_life::behavior::Action::Socialize
            );
            visits
        };
        assert_eq!(run(false), run(true));
    }
}
