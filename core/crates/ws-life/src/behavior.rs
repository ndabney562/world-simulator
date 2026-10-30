//! ADR 0028 integer deficits, minute utility decisions and exact L1/L2 projection.
use crate::{
    Activity, Location, Place, Schedules,
    navigation::{MM_PER_TICK, NavError, Navigation, Point},
};
use core::num::NonZeroU64;
use ws_ecs::EntityId;
use ws_rng::{Purpose, SipHasher24, Stream, below};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;

/// Need/rate/action/decision policy version, independent of schedule/navigation versions.
pub const VERSION: u32 = 1;
/// Maximum integer deficit for each need.
pub const MAX_DEFICIT: u32 = 100_000;
/// New independent initial-need stream; draw index identifies the four needs.
pub const INITIAL: Purpose = Purpose::new("ws-life/needs/initial");
/// New independent equal-utility tie stream; address is resident and action.
pub const TIE: Purpose = Purpose::new("ws-life/utility/tie");
mod overrides;
mod storm;
pub use overrides::{Journey, RoutineOverride};
pub use storm::{Deferral, StormHold, StormWindow};

const STEP: i128 = 100_000;
const MINUTE: i128 = 60_000_000;

/// Held micro-action; no movement override, food inventory or relationship creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    /// Stay idle at home.
    Idle = 0,
    /// Reduce the rest deficit at home.
    Rest = 1,
    /// Reduce hunger at home, without modelling inventory.
    Eat = 2,
    /// A synthetic local social activity, without crowds/relationships.
    Socialize = 3,
    /// Fulfil work/school/errands or home obligations.
    Duties = 4,
    /// Follow the mandatory scheduled journey.
    Travel = 5,
    /// Follow mandatory sleep.
    Sleep = 6,
}
impl Action {
    /// Decode an explicit action tag.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Idle),
            1 => Some(Self::Rest),
            2 => Some(Self::Eat),
            3 => Some(Self::Socialize),
            4 => Some(Self::Duties),
            5 => Some(Self::Travel),
            6 => Some(Self::Sleep),
            _ => None,
        }
    }
    /// Base deficit drift plus this action's effect per 100-ms tick.
    pub const fn rates(self) -> [i32; 4] {
        match self {
            Self::Idle | Self::Travel => [1, 2, 1, 2],
            Self::Rest => [-3, 2, 1, 2],
            Self::Eat => [1, -10, 1, 2],
            Self::Socialize => [1, 2, -7, 2],
            Self::Duties => [1, 2, 1, -6],
            Self::Sleep => [-7, 2, 1, 2],
        }
    }
}
/// Bounded deficits in rest/hunger/social/duty order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs([u32; 4]);
impl Needs {
    /// Constructs only deficits inside the versioned range.
    pub fn checked(values: [u32; 4]) -> Option<Self> {
        values
            .iter()
            .all(|v| *v <= MAX_DEFICIT)
            .then_some(Self(values))
    }
    /// Four explicit integer quantities.
    pub const fn values(self) -> [u32; 4] {
        self.0
    }
    /// Initial deterministic needs, addressed by resident ID and four fixed draws.
    pub fn initial(seed: u64, resident: EntityId) -> Self {
        let stream = Stream::new(seed, INITIAL, &[resident.raw()]);
        Self(core::array::from_fn(|i| {
            below(
                stream.draw(i as u64),
                NonZeroU64::new(u64::from(MAX_DEFICIT) + 1).unwrap(),
            ) as u32
        }))
    }
    /// Exact saturated integration for a held action. Wide arithmetic never wraps.
    pub fn advanced(self, action: Action, ticks: u64) -> Self {
        let rates = action.rates();
        Self(core::array::from_fn(|i| {
            (i128::from(self.0[i]) + i128::from(rates[i]) * i128::from(ticks))
                .clamp(0, i128::from(MAX_DEFICIT)) as u32
        }))
    }
}
/// Chooses the highest admissible utility; only equal highest scores use a tie draw.
/// All decisions use the same canonical counter at both detail levels.
pub fn choose(
    seed: u64,
    resident: EntityId,
    counter: u64,
    activity: Activity,
    needs: Needs,
) -> Action {
    match activity {
        Activity::Sleep => return Action::Sleep,
        Activity::Commute => return Action::Travel,
        Activity::Work | Activity::School | Activity::Errands => return Action::Duties,
        Activity::Home => {}
    }
    let [rest, hunger, social, duty] = needs.values();
    [
        (Action::Idle, 0),
        (Action::Rest, 4 * rest),
        (Action::Eat, 4 * hunger),
        (Action::Socialize, 3 * social),
        (Action::Duties, 2 * duty),
    ]
    .into_iter()
    .max_by_key(|(action, score)| {
        (
            *score,
            Stream::new(seed, TIE, &[resident.raw(), *action as u64]).draw(counter),
            core::cmp::Reverse(*action as u8),
        )
    })
    .unwrap()
    .0
}
/// Manual Rust-side detail before camera-driven policy in VS-006.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Detail {
    /// Fine paths and local/off-map projection.
    L1 = 1,
    /// Coarse schedule location, retaining need/decision anchors.
    L2 = 2,
}
impl Detail {
    /// Accepts only the two implemented detail tags.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::L1),
            2 => Some(Self::L2),
            _ => None,
        }
    }
}
/// Exactly one fine physical representation, separate from logical schedule location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FineLocation {
    /// One on-graph point, walking or waiting at a portal.
    Local {
        /// Exact approximate-plane millimetres, not an ENU/elevation vector.
        point: Point,
        /// False after reaching a portal and before the scheduled activity boundary.
        walking: bool,
    },
    /// Unmodelled external segment of a scheduled trip, retaining identity.
    OffMapTransit {
        /// Registered off-map workplace.
        workplace: EntityId,
        /// Registered gateway index.
        exit: u8,
    },
    /// Stationary at the registered off-map workplace/school.
    OffMap {
        /// Registered off-map workplace.
        workplace: EntityId,
        /// Registered gateway index.
        exit: u8,
    },
}
/// Typed domain validation/reservation failure; no partial publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorError {
    /// Navigation construction or endpoint failure.
    Navigation(NavError),
    /// A route does not fit the unchanged scheduled commute interval.
    InfeasibleRoute,
    /// Start/epoch/query is outside the 100-ms grid.
    Grid,
    /// Time is before the anchor or requires an uncommitted minute decision.
    Time,
    /// An actual minute decision would overflow its canonical counter.
    Counter,
    /// Detail/snapshot request refers to no resident in the baseline.
    UnknownResident,
    /// Fallible temporary planning capacity could not be reserved.
    Resource,
}
impl core::fmt::Display for BehaviorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "behavior error: {self:?}")
    }
}
impl std::error::Error for BehaviorError {}
impl From<NavError> for BehaviorError {
    fn from(e: NavError) -> Self {
        Self::Navigation(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResidentState {
    id: EntityId,
    detail: Detail,
    needs: Needs,
    anchor: SimTime,
    action: Action,
    decisions: u64,
    // Derived: all logical transitions are on the same minute grid. Excluded
    // from hashing; a quiet tick need not re-evaluate the timetable for counts.
    logical: crate::Answer,
    routine: Option<RoutineOverride>,
    storm_hold: Option<StormHold>,
}
/// Canonical persistent resident driving state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidentStateSnapshot {
    /// Stable resident identity.
    pub id: EntityId,
    /// Level membership.
    pub detail: Detail,
    /// Bounded need deficits.
    pub needs: Needs,
    /// Physical decision anchor time.
    pub anchor: SimTime,
    /// Held action.
    pub action: Action,
    /// Number of decisions made.
    pub decisions: u64,
    /// Temporary routine override if active.
    pub routine: Option<RoutineOverride>,
    /// Storm hold if deferred or not yet rejoined (D1); never an override.
    pub storm_hold: Option<StormHold>,
}
/// Common current individual quantities, without reconstructing fine geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommonSnapshot {
    /// Stable identity.
    pub resident: EntityId,
    /// Sole level membership.
    pub detail: Detail,
    /// Bounded current need quantities.
    pub needs: Needs,
    /// Held micro-action.
    pub action: Action,
    /// Retained canonical decision count.
    pub decisions: u64,
    /// Unique logical schedule answer.
    pub logical: crate::Answer,
}
/// Read-only view at a committed physical boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Stable identity.
    pub resident: EntityId,
    /// Exactly one level membership.
    pub detail: Detail,
    /// Current bounded needs, evaluated from the canonical anchor.
    pub needs: Needs,
    /// Held action selected at creation or the last model-minute boundary.
    pub action: Action,
    /// Number of actual decisions; a level change never increments it.
    pub decisions: u64,
    /// Unique current logical schedule answer.
    pub logical: crate::Answer,
    /// Present only when fine; L2 has no separate materialized physical location.
    pub fine: Option<FineLocation>,
}
/// Canonical resident anchors/levels plus immutable schedule/navigation inputs.
#[derive(Debug, Clone)]
pub struct Behavior {
    seed: u64,
    schedules: Schedules,
    navigation: Navigation,
    states: Vec<ResidentState>,
    next: Option<SimTime>,
    // Immutable featured buildings (the LOD nodes), derived from the population.
    featured: Vec<EntityId>,
    storm: Option<StormWindow>,
}
/// Borrowed prepared domain transaction. Dropping it changes no canonical state;
/// exclusive borrowing prevents stale/cross-world plans or mutation before commit.
pub struct Prepared<'a> {
    world: &'a mut Behavior,
    planned: Option<(Vec<ResidentState>, Option<SimTime>)>,
    time: SimTime,
    storm: Option<StormWindow>,
    deferrals: Vec<Deferral>,
}
impl Prepared<'_> {
    /// New deferred departures in this prepare, in ascending resident order.
    pub fn deferrals(&self) -> &[Deferral] {
        &self.deferrals
    }
    /// Stages the block storm window. A storm cast now holds departures from the
    /// next re-evaluation on; residents already in transit continue. None clears
    /// it, or, while residents are held, ends it at the next tick (after any row
    /// deferred at this instant) so every hold resumes by the post-storm rule.
    pub fn set_storm(&mut self, window: Option<StormWindow>) {
        let held = self
            .planned
            .as_ref()
            .map_or(&self.world.states, |(s, _)| s)
            .iter()
            .any(|s| s.storm_hold.is_some());
        let tick = micros(self.time) + STEP;
        self.storm = match window {
            None if held => self.storm.map(|w| StormWindow {
                end: i64::try_from(tick)
                    .map_or(w.end, |t| w.end.min(SimTime::from_micros_since_epoch(t))),
                ..w
            }),
            w => w,
        };
    }
    /// Boundary whose common needs/decisions have already been prepared.
    pub const fn time(&self) -> SimTime {
        self.time
    }
    /// Stable complete identity order, independent of planned level changes.
    pub fn residents(&self) -> impl ExactSizeIterator<Item = EntityId> + '_ {
        self.world.residents()
    }
    /// Read-only view of the planned state, before publication.
    /// # Errors
    /// Unknown identity or invalid planned time.
    pub fn snapshot(&self, id: EntityId) -> Result<Snapshot, BehaviorError> {
        let common = self.common(id)?;
        self.world.fine_snapshot(common, self.time)
    }
    /// Current common quantities for count/invariant planning, without geometry.
    /// # Errors
    /// Unknown identity or invalid planned time.
    pub fn common(&self, id: EntityId) -> Result<CommonSnapshot, BehaviorError> {
        let i = self.world.index(id)?;
        let (s, next) = self
            .planned
            .as_ref()
            .map_or((self.world.states[i], self.world.next), |(s, n)| (s[i], *n));
        self.world.common_state(s, self.time, next)
    }
    /// Current reconstructable position, including while a resident is coarse.
    /// # Errors
    /// Unknown identity, time or endpoint.
    pub fn projected_location(&self, id: EntityId) -> Result<FineLocation, BehaviorError> {
        self.world
            .project_answer(self.common(id)?.logical, self.time)
    }
    /// Stages one level change in this same transaction; never changes decisions.
    /// # Errors
    /// Unknown identity, invalid fine projection or failed planning reservation.
    pub fn set_detail(&mut self, id: EntityId, detail: Detail) -> Result<(), BehaviorError> {
        let i = self.world.index(id)?;
        let old = self.common(id)?;
        if old.detail == detail {
            return Ok(());
        }
        if detail == Detail::L1 {
            self.projected_location(id)?;
        }
        if self.planned.is_none() {
            let mut states = Vec::new();
            states
                .try_reserve(self.world.states.len())
                .map_err(|_| BehaviorError::Resource)?;
            states.extend_from_slice(&self.world.states);
            self.planned = Some((states, self.world.next));
        }
        self.planned.as_mut().unwrap().0[i].detail = detail;
        Ok(())
    }
    fn writable(&mut self) -> Result<&mut Vec<ResidentState>, BehaviorError> {
        if self.planned.is_none() {
            let mut states = Vec::new();
            states
                .try_reserve(self.world.states.len())
                .map_err(|_| BehaviorError::Resource)?;
            states.extend_from_slice(&self.world.states);
            self.planned = Some((states, self.world.next));
        }
        Ok(&mut self.planned.as_mut().unwrap().0)
    }
    /// Planned override, including earlier choices in this transaction.
    pub fn routine(&self, id: EntityId) -> Result<Option<RoutineOverride>, BehaviorError> {
        let i = self.world.index(id)?;
        Ok(self
            .planned
            .as_ref()
            .map_or(self.world.states[i].routine, |(s, _)| s[i].routine))
    }
    fn install(&mut self, id: EntityId, routine: RoutineOverride) -> Result<(), BehaviorError> {
        let i = self.world.index(id)?;
        let common = self.common(id)?;
        let time = self.time;
        let s = &mut self.writable()?[i];
        s.needs = common.needs;
        s.anchor = time;
        s.logical = routine.answer(time);
        s.action = routine.action(time);
        s.routine = Some(routine);
        // An override supersedes a storm hold; override legs are never deferred.
        s.storm_hold = None;
        Ok(())
    }
    /// Stages a finite illness routine, preserving any current journey.
    pub fn afflict(
        &mut self,
        id: EntityId,
        home: EntityId,
        until: SimTime,
        root: u64,
    ) -> Result<RoutineOverride, BehaviorError> {
        let routine = RoutineOverride::illness(
            self.world,
            id,
            self.common(id)?.logical,
            self.time,
            until,
            root,
            home,
        )?;
        self.install(id, routine)?;
        Ok(routine)
    }
    /// Stages an actual same-building check-in; callers validate eligibility first.
    pub fn visit(
        &mut self,
        id: EntityId,
        home: EntityId,
        until: SimTime,
        root: u64,
    ) -> Result<(), BehaviorError> {
        let routine = RoutineOverride::visit(self.world, id, self.time, until, root, home)?;
        self.install(id, routine)
    }
    /// Stages a gathering routine: walk to `target`, stay until `until`, walk home.
    /// # Errors
    /// Unknown resident, no route, or `until` not after the prepared time.
    pub fn gather(
        &mut self,
        id: EntityId,
        home: EntityId,
        target: EntityId,
        until: SimTime,
        root: u64,
    ) -> Result<RoutineOverride, BehaviorError> {
        let routine =
            RoutineOverride::gathering(self.world, id, self.time, until, root, home, target)?;
        self.install(id, routine)?;
        Ok(routine)
    }
    /// Publishes already validated/reserved records with no allocation or failure.
    pub fn commit(self) {
        if let Some((states, next)) = self.planned {
            self.world.states = states;
            self.world.next = next;
        }
        self.world.storm = self.storm;
    }
}
fn micros(t: SimTime) -> i128 {
    i128::from(t.as_micros_since_epoch())
}
fn next_minute(epoch: SimTime, now: SimTime) -> Option<SimTime> {
    let next = micros(epoch) + ((micros(now) - micros(epoch)).div_euclid(MINUTE) + 1) * MINUTE;
    i64::try_from(next)
        .ok()
        .map(SimTime::from_micros_since_epoch)
}
fn grid(t: SimTime) -> bool {
    micros(t).rem_euclid(STEP) == 0
}
impl Behavior {
    /// Creates all residents in L2, retaining their exact future need/decision state.
    /// # Errors
    /// Invalid time grid, baseline destinations, geometry or deadline feasibility.
    pub fn new(
        population: &PopulatedSlice,
        schedules: &Schedules,
        start: SimTime,
    ) -> Result<Self, BehaviorError> {
        if !grid(start) || !grid(schedules.epoch()) {
            return Err(BehaviorError::Grid);
        }
        if !population
            .residents()
            .ids()
            .iter()
            .copied()
            .eq(schedules.profiles().iter().map(|p| p.resident))
        {
            return Err(BehaviorError::UnknownResident);
        }
        let navigation = Navigation::new(population)?;
        for p in schedules.profiles() {
            for row in p.weekday().iter().chain(p.weekend()) {
                if let (Some(from), Some(arrive)) = (row.origin, row.arrival_minute) {
                    let path = navigation.route(from, row.destination)?;
                    let duration = i128::from(arrive) - i128::from(row.minute);
                    if duration <= 0 || i128::from(path.walking_ticks()) * STEP > duration * MINUTE
                    {
                        return Err(BehaviorError::InfeasibleRoute);
                    }
                }
            }
        }
        let seed = population.baseline().structural.seed;
        let states = schedules
            .profiles()
            .iter()
            .map(|p| {
                let needs = Needs::initial(seed, p.resident);
                ResidentState {
                    id: p.resident,
                    detail: Detail::L2,
                    needs,
                    anchor: start,
                    action: choose(
                        seed,
                        p.resident,
                        0,
                        p.at(schedules.epoch(), start).activity,
                        needs,
                    ),
                    decisions: 1,
                    logical: p.at(schedules.epoch(), start),
                    routine: None,
                    storm_hold: None,
                }
            })
            .collect();
        Ok(Self {
            seed,
            schedules: schedules.clone(),
            navigation,
            states,
            next: next_minute(schedules.epoch(), start),
            featured: population.world().buildings().ids().to_vec(),
            storm: None,
        })
    }
    /// Canonical resident states in entity ID order, for persistence.
    pub fn resident_states(&self) -> impl Iterator<Item = ResidentStateSnapshot> + '_ {
        self.states.iter().map(|s| ResidentStateSnapshot {
            id: s.id,
            detail: s.detail,
            needs: s.needs,
            anchor: s.anchor,
            action: s.action,
            decisions: s.decisions,
            routine: s.routine,
            storm_hold: s.storm_hold,
        })
    }
    /// Retained storm window, kept after its end until every hold has rejoined.
    pub const fn storm(&self) -> Option<StormWindow> {
        self.storm
    }
    /// Restores behavior directly from canonical driving states without replay.
    /// `storm` is the saved window; every storm hold requires it.
    ///
    /// # Errors
    /// Invalid roster, non-grid anchor, time before anchor, navigation failure, or
    /// (`Time`) a storm hold without a storm window or alongside an override.
    pub fn from_saved_states(
        population: &PopulatedSlice,
        schedules: &Schedules,
        states: &[ResidentStateSnapshot],
        now: SimTime,
        storm: Option<StormWindow>,
    ) -> Result<Self, BehaviorError> {
        let seed = population.baseline().structural.seed;
        let navigation = Navigation::new(population).map_err(BehaviorError::Navigation)?;
        if states.len() != population.residents().len() {
            return Err(BehaviorError::UnknownResident);
        }
        let mut restored_states = Vec::new();
        restored_states
            .try_reserve(states.len())
            .map_err(|_| BehaviorError::Resource)?;
        for (saved, &expected_id) in states.iter().zip(population.residents().ids()) {
            if saved.id != expected_id {
                return Err(BehaviorError::UnknownResident);
            }
            if !grid(saved.anchor) || saved.anchor > now {
                return Err(BehaviorError::Grid);
            }
            let logical = match (
                saved.routine.filter(|r| {
                    saved.anchor >= r.started && r.rejoin.is_none_or(|t| saved.anchor < t)
                }),
                saved.storm_hold,
            ) {
                (_, Some(_)) if saved.routine.is_some() => return Err(BehaviorError::Time),
                (_, Some(h)) => h.answer(saved.anchor, storm.ok_or(BehaviorError::Time)?.end),
                (Some(r), None) => r.answer(saved.anchor),
                (None, None) => schedules
                    .get(saved.id)
                    .ok_or(BehaviorError::UnknownResident)?
                    .at(schedules.epoch(), saved.anchor),
            };
            restored_states.push(ResidentState {
                id: saved.id,
                detail: saved.detail,
                needs: saved.needs,
                anchor: saved.anchor,
                action: saved.action,
                decisions: saved.decisions,
                logical,
                routine: saved.routine,
                storm_hold: saved.storm_hold,
            });
        }
        Ok(Self {
            seed,
            schedules: schedules.clone(),
            navigation,
            states: restored_states,
            next: next_minute(schedules.epoch(), now),
            featured: population.world().buildings().ids().to_vec(),
            storm,
        })
    }
    /// Retained routine, without advancing behavior.
    pub fn routine(&self, id: EntityId) -> Result<Option<RoutineOverride>, BehaviorError> {
        Ok(self.states[self.index(id)?].routine)
    }
    fn effective_answer(
        &self,
        id: EntityId,
        time: SimTime,
    ) -> Result<crate::Answer, BehaviorError> {
        let s = self.states[self.index(id)?];
        let hold = s
            .storm_hold
            .zip(self.storm)
            .filter(|(h, w)| !h.rejoined(time, w.end));
        Ok(
            match (
                s.routine
                    .filter(|r| time >= r.started && r.rejoin.is_none_or(|t| time < t)),
                hold,
            ) {
                (Some(r), _) => r.answer(time),
                (None, Some((h, w))) => h.answer(time, w.end),
                (None, None) => self
                    .schedules
                    .get(id)
                    .ok_or(BehaviorError::UnknownResident)?
                    .at(self.schedules.epoch(), time),
            },
        )
    }
    /// Immutable navigation proxy and derived route cache.
    pub const fn navigation(&self) -> &Navigation {
        &self.navigation
    }
    /// Immutable minute-grid origin shared with the camera controller.
    pub const fn schedule_epoch(&self) -> SimTime {
        self.schedules.epoch()
    }
    /// Canonical resident order; no off-map identity is removed.
    pub fn residents(&self) -> impl ExactSizeIterator<Item = EntityId> + '_ {
        self.states.iter().map(|s| s.id)
    }
    /// First unprocessed minute decision, a derived cache excluded from hashing.
    pub const fn next_decision(&self) -> Option<SimTime> {
        self.next
    }
    fn index(&self, id: EntityId) -> Result<usize, BehaviorError> {
        self.states
            .binary_search_by_key(&id, |s| s.id)
            .map_err(|_| BehaviorError::UnknownResident)
    }
    /// Plans every intervening minute exactly, without mutating the borrowed domain.
    /// Quiet intervals allocate nothing. L2 catch-up drains the same decisions as L1.
    /// # Errors
    /// Invalid/backward grid time, reservation failure or decision-counter overflow.
    pub fn prepare(&mut self, time: SimTime) -> Result<Prepared<'_>, BehaviorError> {
        if !grid(time) {
            return Err(BehaviorError::Grid);
        }
        if self.states.iter().any(|s| time < s.anchor) {
            return Err(BehaviorError::Time);
        }
        let storm = self.storm;
        // Exact re-evaluation times beyond the minute grid: routine phases and holds.
        let phase = move |s: &ResidentState| {
            s.routine
                .and_then(|r| r.next(s.anchor))
                .or_else(|| storm.and_then(|w| s.storm_hold.and_then(|h| h.next(s.anchor, w.end))))
        };
        // An ended storm is dropped once no resident holds.
        let retained = |states: &[ResidentState]| {
            storm.filter(|w| time < w.end || states.iter().any(|s| s.storm_hold.is_some()))
        };
        let first = self.states.iter().filter_map(phase).chain(self.next).min();
        let Some(mut at) = first.filter(|t| *t <= time) else {
            return Ok(Prepared {
                storm: retained(&self.states),
                world: self,
                planned: None,
                time,
                deferrals: Vec::new(),
            });
        };
        let mut states = Vec::new();
        states
            .try_reserve(self.states.len())
            .map_err(|_| BehaviorError::Resource)?;
        states.extend_from_slice(&self.states);
        let mut next = self.next;
        let mut deferrals: Vec<Deferral> = Vec::new();
        loop {
            let minute = next == Some(at);
            for (s, p) in states.iter_mut().zip(self.schedules.profiles()) {
                if !minute && phase(s) != Some(at) {
                    continue;
                }
                let count = if minute {
                    s.decisions.checked_add(1).ok_or(BehaviorError::Counter)?
                } else {
                    s.decisions
                };
                let ticks = ((micros(at) - micros(s.anchor)) / STEP) as u64;
                s.needs = s.needs.advanced(s.action, ticks);
                s.anchor = at;
                if s.routine.is_some_and(|r| r.rejoin.is_some_and(|t| at >= t)) {
                    s.routine = None;
                }
                if let Some(r) = s.routine {
                    s.logical = r.answer(at);
                    s.action = r.action(at);
                } else {
                    let epoch = self.schedules.epoch();
                    s.logical = match storm {
                        Some(w) => {
                            let (answer, new) = storm::step(w, &self.featured, p, epoch, s, at);
                            if let Some(d) = new {
                                deferrals
                                    .try_reserve(1)
                                    .map_err(|_| BehaviorError::Resource)?;
                                // Stable: catch-up across minutes keeps time order per resident.
                                let i = deferrals.partition_point(|x| x.resident <= d.resident);
                                deferrals.insert(i, d);
                            }
                            answer
                        }
                        None => p.at(epoch, at),
                    };
                    s.action = choose(self.seed, s.id, s.decisions, s.logical.activity, s.needs);
                }
                s.decisions = count;
            }
            if minute {
                next = next_minute(self.schedules.epoch(), at);
            }
            let due = states.iter().filter_map(phase).chain(next).min();
            match due {
                Some(t) if t <= time => at = t,
                _ => break,
            }
        }
        Ok(Prepared {
            storm: retained(&states),
            world: self,
            planned: Some((states, next)),
            time,
            deferrals,
        })
    }
    /// Standalone headless catch-up convenience. Clock ownership remains in ws-sim.
    /// # Errors
    /// Same validated/fallible preparation errors as `prepare`.
    pub fn advance_to(&mut self, time: SimTime) -> Result<(), BehaviorError> {
        self.prepare(time)?.commit();
        Ok(())
    }
    /// Changes only level membership at a current committed boundary; no new decision.
    /// # Errors
    /// Unknown resident, invalid/uncommitted time or failed fine projection.
    pub fn set_detail(
        &mut self,
        id: EntityId,
        detail: Detail,
        time: SimTime,
    ) -> Result<(), BehaviorError> {
        let i = self.index(id)?;
        self.snapshot(id, time)?;
        if detail == Detail::L1 {
            self.projected_location(id, time)?;
        }
        self.states[i].detail = detail;
        Ok(())
    }
    /// Current pure view; never drains decisions or mutates indices/canonical state.
    /// # Errors
    /// Unknown identity, invalid grid, backward time or unprocessed minute.
    pub fn snapshot(&self, id: EntityId, time: SimTime) -> Result<Snapshot, BehaviorError> {
        self.fine_snapshot(self.common(id, time)?, time)
    }
    /// Common current quantities, without reconstructing fine geometry.
    /// # Errors
    /// Unknown identity, invalid grid or an uncommitted minute.
    pub fn common(&self, id: EntityId, time: SimTime) -> Result<CommonSnapshot, BehaviorError> {
        if !grid(time) {
            return Err(BehaviorError::Grid);
        }
        self.common_state(self.states[self.index(id)?], time, self.next)
    }
    fn common_state(
        &self,
        s: ResidentState,
        time: SimTime,
        next: Option<SimTime>,
    ) -> Result<CommonSnapshot, BehaviorError> {
        if !grid(time) {
            return Err(BehaviorError::Grid);
        }
        if time < s.anchor || next.is_some_and(|next| time >= next) {
            return Err(BehaviorError::Time);
        }
        let ticks = ((micros(time) - micros(s.anchor)) / STEP) as u64;
        Ok(CommonSnapshot {
            resident: s.id,
            detail: s.detail,
            needs: s.needs.advanced(s.action, ticks),
            action: s.action,
            decisions: s.decisions,
            logical: s.logical,
        })
    }
    fn fine_snapshot(&self, s: CommonSnapshot, time: SimTime) -> Result<Snapshot, BehaviorError> {
        Ok(Snapshot {
            resident: s.resident,
            detail: s.detail,
            needs: s.needs,
            action: s.action,
            decisions: s.decisions,
            logical: s.logical,
            fine: if s.detail == Detail::L1 {
                Some(self.project_answer(s.logical, time)?)
            } else {
                None
            },
        })
    }
    /// Reconstructable physical location from the pinned schedule/path, at a grid time.
    /// Diagnostic projection is available at either level, without materialization.
    /// # Errors
    /// Unknown identity/endpoint, invalid grid or an unsupported journey shape.
    pub fn projected_location(
        &self,
        id: EntityId,
        time: SimTime,
    ) -> Result<FineLocation, BehaviorError> {
        if !grid(time) {
            return Err(BehaviorError::Grid);
        }
        self.index(id)?;
        self.project_answer(self.effective_answer(id, time)?, time)
    }
    fn project_answer(
        &self,
        answer: crate::Answer,
        time: SimTime,
    ) -> Result<FineLocation, BehaviorError> {
        let local = |place| -> Result<_, BehaviorError> {
            Ok(FineLocation::Local {
                point: self.navigation.position(place, place, 0)?,
                walking: false,
            })
        };
        match answer.location {
            Location::AtBuilding(id) => local(Place::Building(id)),
            Location::OffMap(workplace, exit) => Ok(FineLocation::OffMap { workplace, exit }),
            Location::InTransit {
                from,
                to,
                departure,
                arrival,
            } => {
                let path = self.navigation.route(from, to)?;
                // Return walking starts on the grid: ceil(length/140) ticks before
                // the unchanged arrival. Last tick clamps to the exact portal.
                let start = if matches!(from, Place::OffMap(..)) {
                    arrival - i128::from(path.walking_ticks()) * STEP
                } else {
                    departure
                };
                if let Place::OffMap(workplace, exit) = from
                    && micros(time) < start
                {
                    return Ok(FineLocation::OffMapTransit { workplace, exit });
                }
                let ticks = ((micros(time) - start) / STEP).max(0) as u64;
                let distance = ticks.saturating_mul(MM_PER_TICK);
                if let Place::OffMap(workplace, exit) = to
                    && distance >= path.distance_mm()
                {
                    return Ok(FineLocation::OffMapTransit { workplace, exit });
                }
                Ok(FineLocation::Local {
                    point: self.navigation.position(from, to, distance)?,
                    walking: distance < path.distance_mm(),
                })
            }
        }
    }
    /// Read-only outgoing local route heading in the fixed render basis.
    /// Zero when stationary or off-map; this never materializes an L2 resident.
    /// # Errors
    /// Unknown resident, invalid time or failed route/frame conversion.
    pub fn render_heading(&self, id: EntityId, time: SimTime) -> Result<f32, BehaviorError> {
        if !matches!(
            self.projected_location(id, time)?,
            FineLocation::Local { walking: true, .. }
        ) {
            return Ok(0.0);
        }
        let Location::InTransit {
            from,
            to,
            departure,
            arrival,
        } = self.effective_answer(id, time)?.location
        else {
            return Ok(0.0);
        };
        let path = self.navigation.route(from, to)?;
        let start = if matches!(from, Place::OffMap(..)) {
            arrival - i128::from(path.walking_ticks()) * STEP
        } else {
            departure
        };
        let ticks = ((micros(time) - start) / STEP).max(0) as u64;
        self.navigation
            .render_heading(from, to, ticks.saturating_mul(MM_PER_TICK))
            .map_err(BehaviorError::Navigation)
    }
    /// Explicit canonical anchors, levels, rules and geometry; no capacities/caches.
    /// Current simulation time is hashed separately by the owning orchestrator.
    pub fn state_hash(&self) -> u64 {
        let mut h = SipHasher24::new(self.seed, Purpose::new("ws-life/behavior").value());
        h.write_words(&[
            u64::from(VERSION),
            self.schedules.epoch().as_micros_since_epoch() as u64,
            self.navigation.fingerprint(),
            self.states.len() as u64,
        ]);
        for s in &self.states {
            h.write_words(&[
                s.id.raw(),
                s.detail as u64,
                s.anchor.as_micros_since_epoch() as u64,
                s.action as u64,
                s.decisions,
            ]);
            for v in s.needs.values() {
                h.write_u64(u64::from(v));
            }
        }
        let mut hash = h.finish();
        if self.states.iter().any(|s| s.routine.is_some()) {
            let mut h = SipHasher24::new(hash, Purpose::new("ws-life/overrides/v1").value());
            for s in &self.states {
                h.write_words(&[s.id.raw(), u64::from(s.routine.is_some())]);
                if let Some(r) = s.routine {
                    r.hash(&mut h);
                }
            }
            hash = h.finish();
        }
        // Appended only with a storm or a hold, so earlier hashes stay byte-identical.
        if self.storm.is_some() || self.states.iter().any(|s| s.storm_hold.is_some()) {
            let mut h = SipHasher24::new(hash, Purpose::new("ws-life/storm/v1").value());
            match self.storm {
                Some(w) => h.write_words(&[
                    1,
                    w.start.as_micros_since_epoch() as u64,
                    w.end.as_micros_since_epoch() as u64,
                ]),
                None => h.write_u64(0),
            }
            for s in &self.states {
                h.write_words(&[s.id.raw(), u64::from(s.storm_hold.is_some())]);
                if let Some(hold) = s.storm_hold {
                    hold.hash(&mut h);
                }
            }
            hash = h.finish();
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counter_overflow_drops_prepared_work_without_partial_publication() {
        let p = PopulatedSlice::new(42).unwrap();
        let s = Schedules::new(&p, SimTime::EPOCH).unwrap();
        let mut b = Behavior::new(&p, &s, SimTime::EPOCH).unwrap();
        b.states.last_mut().unwrap().decisions = u64::MAX;
        let before = b.state_hash();
        let next = b.next;
        assert!(matches!(
            b.prepare(SimTime::from_micros_since_epoch(60_000_000)),
            Err(BehaviorError::Counter)
        ));
        assert_eq!(b.state_hash(), before);
        assert_eq!(b.next, next);
    }
    #[test]
    fn abandoned_plan_keeps_anchors_levels_and_derived_deadline_unchanged() {
        let p = PopulatedSlice::new(0).unwrap();
        let s = Schedules::new(&p, SimTime::EPOCH).unwrap();
        let mut b = Behavior::new(&p, &s, SimTime::EPOCH).unwrap();
        let hash = b.state_hash();
        let next = b.next;
        drop(
            b.prepare(SimTime::from_micros_since_epoch(3_600_000_000))
                .unwrap(),
        );
        assert_eq!(b.state_hash(), hash);
        assert_eq!(b.next, next);
    }
}
