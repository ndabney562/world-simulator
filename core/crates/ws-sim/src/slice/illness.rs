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
    /// Not a featured residential building (ADR 0038 gathering target).
    Building,
    /// A storm or a gathering is already active (ADR 0038).
    Busy,
}
/// Payload tags recorded in history but never indexed under any root, applied
/// identically by the live producers and [`SliceSimulation::rebuild_consequence_index`].
/// Tag 17 is `StayedInside` (ADR 0038 §3).
pub const INDEX_EXCLUDED: &[u16] = &[17];
pub(super) fn excluded(payload: &Payload) -> bool {
    u16::try_from(payload.tag()).is_ok_and(|t| INDEX_EXCLUDED.contains(&t))
}
/// Canonical persistent illness episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Episode {
    /// Afflicted resident entity identity.
    pub resident: EntityId,
    /// Root intervention event that produced this episode.
    pub root: EventId,
    /// Initial LifeIllness event ID.
    pub illness: EventId,
    /// Severity level, 1 through 10.
    pub severity: u32,
    /// Episode start time.
    pub start: SimTime,
    /// Episode scheduled end time.
    pub end: SimTime,
    /// Absolute microseconds when the resident returns home, if scheduled.
    pub return_at: Option<i128>,
    /// Whether health has recovered.
    pub recovered: bool,
    /// Whether the resident has returned home.
    pub returned: bool,
    /// Count of duties missed.
    pub missed: u8,
    /// Whether a neighbour visited during this episode.
    pub visited: bool,
}
/// Fixed-size index row. Derived solely from committed typed history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    /// Intervention root.
    pub root: EventId,
    /// Target resident or gathering building; None for a storm (no subject).
    pub subject: Option<EntityId>,
    /// The root's payload tag: 13 illness, 14 bless, 16 storm, 20 gathering.
    pub tag: u16,
    /// Requested severity; 0 for every non-illness root.
    pub severity: u32,
    /// Applied occurrence time.
    pub applied: SimTime,
    /// Illness: episode end, None for an accepted duplicate no-op. Bless: the
    /// applied time. Storm and gathering: the episode end.
    pub end: Option<SimTime>,
    ids: [EventId; MAX_CONSEQUENCES],
    len: usize,
}
impl Group {
    /// At most sixteen retained produced records in ascending EventId order.
    pub fn ids(&self) -> &[EventId] {
        &self.ids[..self.len]
    }
    /// The group a slice root starts, derived only from the root event's typed
    /// payload and time, so the live producers and the rebuild agree. None for
    /// a payload that is not a slice root.
    pub(super) fn for_root(
        id: EventId,
        time: SimTime,
        payload: Payload,
    ) -> Result<Option<Self>, SliceError> {
        let after = |duration_us: u64| {
            i64::try_from(duration_us)
                .ok()
                .and_then(|d| time.as_micros_since_epoch().checked_add(d))
                .map(SimTime::from_micros_since_epoch)
                .ok_or(SliceError::History(EventError::Order))
        };
        let (subject, severity, end) = match payload {
            Payload::SliceIllnessApplied {
                resident,
                severity,
                duration_us,
                affected,
                ..
            } => (
                Some(resident),
                severity,
                if affected == 0 {
                    None
                } else {
                    Some(after(duration_us)?)
                },
            ),
            Payload::BlessApplied { resident, .. } => (Some(resident), 0, Some(time)),
            Payload::StormApplied { duration_us, .. } => (None, 0, Some(after(duration_us)?)),
            Payload::GatheringCalled {
                building,
                duration_us,
                ..
            } => (Some(building), 0, Some(after(duration_us)?)),
            _ => return Ok(None),
        };
        let mut g = Self {
            root: id,
            subject,
            tag: payload.tag() as u16,
            severity,
            applied: time,
            end,
            ids: [EventId(0); MAX_CONSEQUENCES],
            len: 0,
        };
        g.append(id)?;
        Ok(Some(g))
    }
    pub(super) fn append(&mut self, id: EventId) -> Result<(), SliceError> {
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
    /// Every slice root's consequence group (ADR 0034, generalised per root by
    /// ADR 0038), in ascending root order.
    pub(super) groups: Vec<Group>,
}
pub(super) struct Affliction {
    home: EntityId,
    end: SimTime,
    duplicate: bool,
}
/// Current authoritative inspection extension; no query changes the episode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    /// Override kind: 0 none, 1 illness/home hold, 2 visit, 3 gathering, 4 storm
    /// hold (ADR 0038 D1 as amended). A routine (1..=3) and a hold never coexist.
    pub override_kind: u32,
    /// Whether health is currently affected.
    pub active: bool,
    /// Producing root for a current routine, zero without one.
    pub override_root: u64,
    /// Most recent intervention that actually affected this resident.
    pub own_root: u64,
    /// Active deadline, None after recovery.
    pub end: Option<SimTime>,
    /// The storm hold driving the answer, exactly when `override_kind` is 4.
    pub hold: Option<ws_life::behavior::StormHold>,
}
impl Illness {
    pub(super) fn episodes(&self) -> &[Episode] {
        &self.episodes
    }
    pub(super) fn from_episodes(episodes: Vec<Episode>) -> Self {
        Self {
            episodes,
            groups: Vec::new(),
        }
    }
    pub(super) fn episode(&self, id: EntityId) -> Option<&Episode> {
        self.episodes
            .binary_search_by_key(&id, |e| e.resident)
            .ok()
            .map(|i| &self.episodes[i])
    }
    pub(super) fn episode_mut(&mut self, id: EntityId) -> Option<&mut Episode> {
        self.episodes
            .binary_search_by_key(&id, |e| e.resident)
            .ok()
            .map(|i| &mut self.episodes[i])
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
    /// Stages index rows for one tick's facts. Each fact is placed exactly as
    /// the rebuild places its committed event: its preferred link (the first
    /// `CausedBy`, else the first `ContributedTo`) is walked with
    /// `EventStore::root_intervention`, and a fact that reaches a slice root is
    /// appended to that root's group. Excluded tags are never indexed. Tick
    /// producers link by committed ID; an in-batch local link is refused.
    pub(super) fn stage_index(
        &self,
        events: &EventStore,
        facts: &[Candidate],
        first: u64,
    ) -> Result<Vec<(usize, Group)>, SliceError> {
        let mut updates: Vec<(usize, Group)> = Vec::new();
        updates
            .try_reserve(facts.len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        for (offset, c) in facts.iter().enumerate() {
            if excluded(&c.payload) || c.payload.is_slice_root() {
                continue;
            }
            let link = c
                .causes
                .iter()
                .find(|l| l.relation == Relation::CausedBy)
                .or_else(|| {
                    c.causes
                        .iter()
                        .find(|l| l.relation == Relation::ContributedTo)
                });
            let cause = match link.map(|l| l.target) {
                None => continue,
                Some(Target::Id(id)) => id,
                Some(Target::Local(_)) => return Err(SliceError::History(EventError::Cause)),
            };
            let Some(root) = events
                .root_intervention(cause)
                .filter(|e| e.record.payload.is_slice_root())
            else {
                continue;
            };
            let i = self
                .groups
                .binary_search_by_key(&root.id, |g| g.root)
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
        // A routine wins; `storm_hold` is None whenever one is retained.
        let hold = self
            .behavior
            .storm_hold(id, self.clock.now())
            .map_err(SliceError::Behavior)?;
        let e = self.illness.episode(id);
        Ok(State {
            override_kind: routine.map_or(hold.map_or(0, |_| 4), |r| r.kind),
            override_root: routine.map_or(hold.map_or(0, |h| h.root), |r| r.root),
            hold,
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
    /// Root of the most recently accepted slice intervention (the newest group,
    /// since groups are in ascending root order), None before any. O(1); the
    /// index is rebuilt from history on load, so the answer survives save/load.
    pub fn last_root(&self) -> Option<EventId> {
        self.illness.groups.last().map(|g| g.root)
    }
    /// Allocated derived-index bytes; grows with retained intervention history.
    pub fn consequence_index_capacity_bytes(&self) -> usize {
        self.illness.groups.capacity() * core::mem::size_of::<Group>()
    }
    /// Reconstructs only the derived producer index from typed roots and explicit links.
    /// Every slice root starts a group; every other non-excluded event whose
    /// `root_intervention` is a slice root is appended to that root's group.
    /// Canonical episodes, health, clocks and history remain untouched.
    pub fn rebuild_consequence_index(&mut self) -> Result<(), SliceError> {
        let mut groups: Vec<Group> = Vec::new();
        groups
            .try_reserve(
                self.events
                    .records()
                    .iter()
                    .filter(|e| e.record.payload.is_slice_root())
                    .count(),
            )
            .map_err(|_| SliceError::History(EventError::Resource))?;
        for event in self.events.records() {
            let payload = event.record.payload;
            if let Some(g) = Group::for_root(event.id, event.record.time, payload)? {
                groups.push(g);
            } else if !excluded(&payload)
                && let Some(root) = self
                    .events
                    .root_intervention(event.id)
                    .filter(|e| e.record.payload.is_slice_root())
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
    /// Deterministic semantic validation shared by execution and stop-fence replay.
    pub(super) fn afflict_preflight(
        &self,
        resident: EntityId,
        severity: u32,
        duration_us: u64,
    ) -> Result<Affliction, SliceError> {
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
        Ok(Affliction {
            home,
            end,
            duplicate,
        })
    }
    pub(super) fn afflict(
        &mut self,
        resident: EntityId,
        severity: u32,
        duration_us: u64,
    ) -> Result<(), SliceError> {
        let Affliction {
            home,
            end,
            duplicate,
        } = self.afflict_preflight(resident, severity, duration_us)?;
        let time = self.clock.now();
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
            subject: Some(resident),
            tag: 13,
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
        // ADR 0038 §4: an attendee still bound for or at a gathering leaves it.
        let powers = self.powers.interrupt(
            resident,
            behavior.routine(resident).map_err(SliceError::Behavior)?,
        );
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
        self.powers = powers;
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
