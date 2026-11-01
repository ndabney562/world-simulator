//! ADR 0038 reversible slice powers. Bless lives only in history and the
//! consequence index; the storm and the gathering are block-level episodes.
use super::illness::{Group, Rejection, excluded};
use super::*;
use ws_events::{Actor, Cause, Confidence, EventId, Relation, Role, Target};
use ws_life::behavior::{Deferral, Prepared, RoutineOverride, StormWindow};
use ws_life::{Activity, Answer, Location};
use ws_world::slice::SourceBuilding;
use ws_world::slice::vitals::Life;

/// Producer system for bless effects (plan decision D4).
const BLESS_SYSTEM: u16 = 84;
/// Producer system for storm effects (plan decision D4).
const STORM_SYSTEM: u16 = 85;
/// Producer system for gathering effects (plan decision D4); `ReturnedHome` keeps 50.
const GATHER_SYSTEM: u16 = 86;
/// Shortest and longest storm, inclusive, on the 100-ms grid (ADR 0038 §1).
const STORM_DURATION_US: core::ops::RangeInclusive<u64> = 100_000..=3_600_000_000;
/// Shortest and longest gathering, inclusive, on the 100-ms grid (ADR 0038 §1).
const GATHER_DURATION_US: core::ops::RangeInclusive<u64> = 600_000_000..=3_600_000_000;
/// At most seven attendees, so the group is at most 1 + 7 + 7 + 1 = 16 IDs
/// (ADR 0038 §4, `MAX_CONSEQUENCES`).
pub const MAX_ATTENDEES: usize = 7;
/// Inclusive attendee reach from home to the target, in millimetres (ADR 0038 §4).
pub const REACH_MM: i64 = 150_000;

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

/// One attendee's progress through a gathering (ADR 0038 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// Walking to the target; not yet arrived.
    Outbound = 0,
    /// Arrived before `end` and recorded `JoinedGathering`.
    Joined = 1,
    /// Home again; recorded `ReturnedHome` under the gathering root.
    Returned = 2,
    /// An illness replaced the gathering routine (ADR 0034 interruption);
    /// the way home belongs to the illness root.
    Interrupted = 3,
}
impl Phase {
    /// The phase for a saved tag, if any (ADR 0038 §6).
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Outbound),
            1 => Some(Self::Joined),
            2 => Some(Self::Returned),
            3 => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// Block-level gathering episode (ADR 0038 §4). Present from the call until
/// `GatheringEnded` has been recorded (the first tick at or after `end`) and
/// every attendee is `Returned` or `Interrupted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gathering {
    /// Featured residential target building.
    pub building: EntityId,
    /// The `GatheringCalled` root.
    pub root: EventId,
    /// Call time.
    pub start: SimTime,
    /// Exclusive end of the stay.
    pub end: SimTime,
    /// Attendees that recorded `JoinedGathering`, including any later
    /// interrupted; `GatheringEnded.attendees` (ADR 0038 §4).
    pub joined: u32,
    attendees: [(EntityId, Phase); MAX_ATTENDEES],
    len: usize,
}
impl Gathering {
    /// A saved gathering for restore (ADR 0038 §6). None if `attendees` holds
    /// more than [`MAX_ATTENDEES`]; semantic coherence is checked by the caller.
    pub fn restore(
        building: EntityId,
        root: EventId,
        start: SimTime,
        end: SimTime,
        joined: u32,
        attendees: &[(EntityId, Phase)],
    ) -> Option<Self> {
        let mut slots = [(building, Phase::Outbound); MAX_ATTENDEES];
        slots.get_mut(..attendees.len())?.copy_from_slice(attendees);
        Some(Self {
            building,
            root,
            start,
            end,
            joined,
            attendees: slots,
            len: attendees.len(),
        })
    }
    /// Attendees in ascending resident ID order, at most seven.
    pub fn attendees(&self) -> &[(EntityId, Phase)] {
        &self.attendees[..self.len]
    }
    fn settled(&self) -> bool {
        self.attendees()
            .iter()
            .all(|(_, p)| matches!(p, Phase::Returned | Phase::Interrupted))
    }
}

/// Integer equirectangular offset from `a` to `b` in millimetres (east, north),
/// from the source pico-degree centroids (plan decision D2; the f32 render
/// position is never used). Each axis truncates toward zero, so the offset
/// from `b` to `a` is the exact negation.
pub fn offset_mm(a: &SourceBuilding, b: &SourceBuilding) -> (i64, i64) {
    let axis = |from: i64, to: i64, mm_per_degree: i128| {
        // Any i64 difference (< 2^65) times 111 133 fits i128, and dividing by
        // 10^9 leaves < 2^52, so the conversion back to i64 cannot fail.
        let mm = (i128::from(to) - i128::from(from)) * mm_per_degree / 1_000_000_000;
        i64::try_from(mm).expect("|mm| < 2^52")
    };
    (
        axis(a.centroid.lon_pdeg, b.centroid.lon_pdeg, 84_383),
        axis(a.centroid.lat_pdeg, b.centroid.lat_pdeg, 111_133),
    )
}
/// Whether `b` lies within [`REACH_MM`] of `a`, inclusive, in integer arithmetic.
pub fn distance_ok(a: &SourceBuilding, b: &SourceBuilding) -> bool {
    let (dx, dy) = offset_mm(a, b);
    let (dx, dy) = (i128::from(dx), i128::from(dy));
    dx * dx + dy * dy <= i128::from(REACH_MM) * i128::from(REACH_MM)
}

/// Block-level power episodes in canonical slice state. At most one is active:
/// each is rejected as `Busy` while either is in progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Powers {
    pub(super) storm: Option<Storm>,
    pub(super) gathering: Option<Gathering>,
}
impl Powers {
    /// Whether the powers hash enters `state_hash`. False whenever no episode
    /// is active, so worlds that never used a block power keep their goldens.
    pub(super) fn active(&self) -> bool {
        self.busy()
    }
    /// ADR 0038 §1: a storm or a gathering is in progress.
    fn busy(&self) -> bool {
        self.storm.is_some() || self.gathering.is_some()
    }
    /// Canonical words of every block-level episode. The gathering's words are
    /// written only while one is active, so a storm-only hash is unchanged.
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
        if let Some(g) = self.gathering {
            h.write_words(&[
                2,
                g.building.raw(),
                g.root.0,
                g.start.as_micros_since_epoch() as u64,
                g.end.as_micros_since_epoch() as u64,
                u64::from(g.joined),
                g.len as u64,
            ]);
            for (id, phase) in g.attendees() {
                h.write_words(&[id.raw(), *phase as u64]);
            }
        }
        h.finish()
    }
    /// ADR 0038 §4 interruption: an illness cast on an attendee still bound for
    /// or at the gathering (its retained routine is this gathering's kind 3)
    /// ends their attendance. Returns the next state, published on commit.
    pub(super) fn interrupt(
        mut self,
        resident: EntityId,
        routine: Option<RoutineOverride>,
    ) -> Self {
        if let Some(g) = self.gathering.as_mut()
            && routine.is_some_and(|r| r.kind == 3 && r.root == g.root.0)
            && let Some(a) = g.attendees[..g.len]
                .iter_mut()
                .find(|(id, p)| *id == resident && matches!(p, Phase::Outbound | Phase::Joined))
        {
            a.1 = Phase::Interrupted;
        }
        self
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
        let god = super::god_pass(
            &self.god,
            self.config,
            &self.population,
            &self.vitals,
            &mut facts,
            time,
        )?;
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
        self.god = god;
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
    /// The active storm, if any; persisted by `ws-persist`.
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
        prepared.set_storm(Some(StormWindow {
            start: time,
            end,
            root: root.0,
        }));
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
        let god = super::god_pass(
            &self.god,
            self.config,
            &self.population,
            &self.vitals,
            &mut facts,
            time,
        )?;
        self.events
            .prepare(self.clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        prepared.commit();
        plan.commit();
        self.god = god;
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

impl SliceSimulation {
    /// The active gathering, if any; persisted by `ws-persist`.
    pub fn gathering(&self) -> Option<&Gathering> {
        self.powers.gathering.as_ref()
    }
    /// Deterministic semantic validation shared by execution and stop-fence
    /// replay. Returns the gathering's end.
    pub(super) fn gather_preflight(
        &self,
        building: EntityId,
        duration_us: u64,
    ) -> Result<SimTime, SliceError> {
        let reject = SliceError::Rejected;
        // Every featured building is residential today; the unit check keeps
        // the ADR's "featured residential" rule explicit.
        if !self
            .population
            .world()
            .buildings()
            .get(building)
            .is_some_and(|b| b.residential_units > 0)
        {
            return Err(reject(Rejection::Building));
        }
        if !GATHER_DURATION_US.contains(&duration_us) || !duration_us.is_multiple_of(100_000) {
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
    /// Gathering (ADR 0038 §4) at the current committed boundary: one root
    /// whose `attendees` is the number chosen, the episode, and a kind-3
    /// routine per attendee, chosen in ascending ID order from the planned
    /// behaviour at this boundary. Zero eligible is accepted with
    /// `attendees = 0`. All fallible work precedes the first mutation.
    pub(super) fn gather(
        &mut self,
        building: EntityId,
        duration_us: u64,
    ) -> Result<(), SliceError> {
        let end = self.gather_preflight(building, duration_us)?;
        let time = self.clock.now();
        self.illness
            .groups
            .try_reserve(1)
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let first = self.events.counters().map_err(SliceError::History)?.0;
        let root = EventId(first);
        let sequence = self.operations.len() as u64;
        let buildings = self.population.world().buildings();
        let target = buildings
            .get(building)
            .ok_or(SliceError::Lod(LodError::Invariant))?;
        let mut gathering = Gathering {
            building,
            root,
            start: time,
            end,
            joined: 0,
            attendees: [(building, Phase::Outbound); MAX_ATTENDEES],
            len: 0,
        };
        let mut prepared = self.behavior.prepare(time).map_err(SliceError::Behavior)?;
        // No storm is active, so this boundary can defer nothing.
        if !prepared.deferrals().is_empty() {
            return Err(SliceError::Lod(LodError::Invariant));
        }
        let at_home = |home| Answer {
            activity: Activity::Home,
            location: Location::AtBuilding(home),
        };
        for (id, resident) in self.population.residents().iter() {
            if gathering.len == MAX_ATTENDEES {
                break;
            }
            let home = resident.home;
            // The planned answer at this boundary, never a stale stored field:
            // a resident held at home by an illness is under an override, and
            // one held by a storm answers at home only while actually there.
            let state = prepared.common(id).map_err(SliceError::Behavior)?;
            let eligible = state.detail == Detail::L1
                && state.logical == at_home(home)
                && buildings.get(home).is_some_and(|b| distance_ok(b, target))
                && self
                    .vitals
                    .get(id)
                    .is_some_and(|v| v.life() == Life::Alive && v.health() == 100)
                && self.illness.episode(id).is_none_or(|e| e.recovered)
                && prepared
                    .routine(id)
                    .map_err(SliceError::Behavior)?
                    .is_none();
            if !eligible {
                continue;
            }
            prepared
                .gather(id, home, building, end, root.0)
                .map_err(SliceError::Behavior)?;
            gathering.attendees[gathering.len] = (id, Phase::Outbound);
            gathering.len += 1;
        }
        let payload = Payload::GatheringCalled {
            building,
            sequence,
            duration_us,
            attendees: gathering.len as u32,
        };
        let group =
            Group::for_root(root, time, payload)?.ok_or(SliceError::History(EventError::Order))?;
        let mut facts = Vec::new();
        facts
            .try_reserve(1 + self.lod.nodes().len())
            .map_err(|_| SliceError::History(EventError::Resource))?;
        let plan = self
            .lod
            .prepare(&mut prepared, self.clock.scale().multiplier())
            .map_err(lod_error)?;
        let mut c = Candidate::new(time, 10, sequence, payload, Origin::Command(sequence));
        c.actors
            .push(Actor {
                id: building,
                role: Role::Subject,
            })
            .map_err(SliceError::History)?;
        facts.push(c);
        facts.extend(
            plan.changes()
                .enumerate()
                .map(|(i, c)| c.fact(time, i as u64)),
        );
        facts.sort_unstable_by_key(|c| (c.system, c.sequence));
        let god = super::god_pass(
            &self.god,
            self.config,
            &self.population,
            &self.vitals,
            &mut facts,
            time,
        )?;
        self.events
            .prepare(self.clock.steps(), &facts)
            .map_err(SliceError::History)?
            .commit();
        prepared.commit();
        plan.commit();
        self.god = god;
        self.powers.gathering = Some(gathering);
        self.illness.groups.push(group);
        Ok(())
    }
}

/// The retained routine of every attendee still on the way, read from the
/// committed behaviour before a tick is prepared: preparation clears a
/// routine at its rejoin, which may coincide with the homeward arrival.
pub(super) fn attendee_routines(
    powers: &Powers,
    behavior: &Behavior,
) -> Result<[Option<RoutineOverride>; MAX_ATTENDEES], SliceError> {
    let mut out = [None; MAX_ATTENDEES];
    if let Some(g) = &powers.gathering {
        for (slot, (id, _)) in out.iter_mut().zip(g.attendees()) {
            *slot = behavior.routine(*id).map_err(SliceError::Behavior)?;
        }
    }
    Ok(out)
}
/// One tick of the gathering, after `storm_tick`. Per attendee in ascending
/// ID order: an `Outbound` attendee whose outbound leg arrived before `end`
/// records `JoinedGathering`; one not `Interrupted` whose homeward leg has
/// arrived records `ReturnedHome` (system 50). At the first tick at or after
/// `end` (`before < end <= time`) `GatheringEnded` counts the joins. Every
/// fact is `CausedBy/Direct` the committed root. The episode clears once it
/// has ended and every attendee is `Returned` or `Interrupted`. Returns the
/// next powers state, published only after the tick commits.
pub(super) fn gathering_tick(
    mut powers: Powers,
    routines: &[Option<RoutineOverride>; MAX_ATTENDEES],
    behavior: &Prepared<'_>,
    before: SimTime,
    time: SimTime,
) -> Result<(Powers, Vec<Candidate>), SliceError> {
    let mut facts = Vec::new();
    let Some(mut g) = powers.gathering else {
        return Ok((powers, facts));
    };
    facts
        .try_reserve(2 * g.len + 1)
        .map_err(|_| SliceError::History(EventError::Resource))?;
    let now = i128::from(time.as_micros_since_epoch());
    let end = i128::from(g.end.as_micros_since_epoch());
    for (i, slot) in routines.iter().enumerate().take(g.len) {
        let (id, mut phase) = g.attendees[i];
        if matches!(phase, Phase::Returned | Phase::Interrupted) {
            continue;
        }
        // Anything else replacing the routine would be an unrecorded interruption.
        let (Some(outbound), Some(homeward), home) = slot
            .filter(|r| r.kind == 3 && r.root == g.root.0)
            .map(|r| (r.original, r.homeward, r.home))
            .ok_or(SliceError::Lod(LodError::Invariant))?
        else {
            return Err(SliceError::Lod(LodError::Invariant));
        };
        let detail = behavior.common(id).map_err(SliceError::Behavior)?.detail;
        if phase == Phase::Outbound && now >= outbound.arrival && outbound.arrival < end {
            let mut c = gathering_fact(
                time,
                (GATHER_SYSTEM, id.raw()),
                Payload::JoinedGathering {
                    resident: id,
                    building: g.building,
                },
                g.root,
                &[id, g.building],
            )?;
            c.lod = detail as u8;
            facts.push(c);
            g.joined = g
                .joined
                .checked_add(1)
                .ok_or(SliceError::History(EventError::Limit))?;
            phase = Phase::Joined;
        }
        if now >= homeward.arrival {
            let mut c = gathering_fact(
                time,
                (50, id.raw()),
                Payload::ReturnedHome { resident: id, home },
                g.root,
                &[id, home],
            )?;
            c.lod = detail as u8;
            facts.push(c);
            phase = Phase::Returned;
        }
        g.attendees[i].1 = phase;
    }
    if before < g.end && time >= g.end {
        // After every per-resident row of this tick.
        facts.push(gathering_fact(
            time,
            (GATHER_SYSTEM, u64::MAX),
            Payload::GatheringEnded {
                building: g.building,
                attendees: g.joined,
            },
            g.root,
            &[g.building],
        )?);
    }
    powers.gathering = (time < g.end || !g.settled()).then_some(g);
    Ok((powers, facts))
}
/// One gathering effect linked `CausedBy/Direct` to the committed root by ID;
/// the first actor is the subject, the rest objects.
fn gathering_fact(
    time: SimTime,
    producer: (u16, u64),
    payload: Payload,
    root: EventId,
    actors: &[EntityId],
) -> Result<Candidate, SliceError> {
    let (system, sequence) = producer;
    let mut c = Candidate::new(time, system, sequence, payload, Origin::System(system));
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
            target: Target::Id(root),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .map_err(SliceError::History)?;
    Ok(c)
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
