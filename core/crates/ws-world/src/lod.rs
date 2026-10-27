//! Simulation level of detail for the toy world: the L1 Active ↔ L2
//! Background round trip ([ADR 0017]).
//!
//! - **L1** is TP-006's stepped representation: a [`SurfacePos`] and a
//!   [`Walk`] per agent, advanced every base step.
//! - **L2** is a discrete-event schedule per agent ([`Background`]): waiting
//!   at a building until a departure step, or walking a leg with an integer
//!   arrival step. It changes only at events, processed in
//!   `(SimTime, EntityId)` order.
//! - **Nodes** are buildings, each at one [`Lod`]. An agent is governed by
//!   the building it is waiting at or walking to, and its residency (which
//!   store holds it) always follows that building's level after a
//!   reconciliation.
//! - **Transitions** ([`ToyWorld::set_lod`], and the reconciliation at the
//!   start of every [`ToyWorld::tick`]) are validated, atomic transactions at
//!   tick boundaries, applied in ascending ID order with batch column
//!   operations.
//! - **The checker** ([`ToyWorld::check_lod_invariants`] and
//!   [`ConservationSummary`]) reports every violated invariant; debug builds
//!   run it after every transaction and tick.
//!
//! [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use ws_ecs::{EntityId, Namespace, join};
use ws_math::{CellFrame, CellId, CoordError, Ecef, Position, SurfacePos};
use ws_time::{Rate, SimDuration, SimTime, TimeOverflow};

use crate::{ArrivalFact, Movement, ToyWorld, Walk, WorldError, ids};

/// A simulation level (05 §6.2). Only [`Lod::L1`] and [`Lod::L2`] are
/// executable in TP-007; the others exist so the stored value and the
/// state hash already have their final encoding.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lod {
    /// Focused (not executable yet).
    L0 = 0,
    /// Active: individually stepped agents.
    L1 = 1,
    /// Background: agents as discrete-event schedules.
    L2 = 2,
    /// Household / building (not executable yet).
    L3 = 3,
    /// Neighbourhood / district (not executable yet).
    L4 = 4,
    /// Settlement / city (not executable yet).
    L5 = 5,
    /// Region / planet (not executable yet).
    L6 = 6,
    /// Celestial (not executable yet).
    L7 = 7,
}

impl Lod {
    /// Whether TP-007 can run nodes at this level.
    #[must_use]
    pub const fn is_executable(self) -> bool {
        matches!(self, Self::L1 | Self::L2)
    }

    /// The level's number, as stored and hashed.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Where a walking leg started.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    /// The centre of this building: every leg that starts at L2.
    Building(EntityId),
    /// An exact position: the keyframe of a leg during which the agent was
    /// demoted. Stored as the parts of a [`SurfacePos`] so the enum stays
    /// 24 bytes.
    Point {
        /// The position's sector cell.
        cell: CellId,
        /// The position's ENU offset in that cell, in metres.
        offset: [f32; 3],
    },
}

impl Origin {
    /// The keyframe origin of an exact position.
    #[must_use]
    pub const fn point(pos: SurfacePos) -> Self {
        Self::Point {
            cell: pos.cell(),
            offset: pos.offset(),
        }
    }
}

/// What an L2 agent is doing. Every time lies on the base-step grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activity {
    /// Standing at the centre of building `at` until `depart`, the step in
    /// which it draws its next target and takes its first step.
    Waiting {
        /// The building it is at.
        at: EntityId,
        /// The step in which it departs.
        depart: SimTime,
    },
    /// Walking a straight leg from `origin` to the centre of `dest`.
    Walking {
        /// Where the leg started.
        origin: Origin,
        /// The tick boundary at which the agent was at `origin`.
        origin_time: SimTime,
        /// The building it is walking to.
        dest: EntityId,
        /// The step in which it arrives.
        arrive: SimTime,
    },
}

/// An agent's L2 state: canonical, hashed, and enough to reconstruct its L1
/// state at any later boundary of the current activity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Background {
    /// What it is doing.
    pub activity: Activity,
    /// The counter of its next-target stream, exactly as at L1.
    pub targets_drawn: u32,
}

impl Background {
    /// The time of its next event: the departure or the arrival.
    #[must_use]
    pub const fn next_event(&self) -> SimTime {
        match self.activity {
            Activity::Waiting { depart, .. } => depart,
            Activity::Walking { arrive, .. } => arrive,
        }
    }

    /// The building that governs its level: where it waits, or where it
    /// is walking to.
    #[must_use]
    pub const fn governing(&self) -> EntityId {
        match self.activity {
            Activity::Waiting { at, .. } => at,
            Activity::Walking { dest, .. } => dest,
        }
    }
}

/// What a schedule got wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleProblem {
    /// A time is not on the base-step grid, or a duration is not a whole
    /// number of steps.
    OffGrid,
    /// The next event is not after the current time.
    EventNotInFuture,
    /// A leg starts after the current time.
    OriginInFuture,
    /// A leg arrives less than one step after it starts.
    EmptyLeg,
    /// A waiting time does not fit the L1 `wait_steps` counter.
    WaitTooLong,
    /// A walking agent that has never drawn a target.
    WalkingWithoutDraw,
}

/// A counted quantity in a conservation comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    /// Residents across the L1 and L2 stores.
    Population,
    /// Residents whose home is this building.
    Home(EntityId),
    /// Residents governed by this building.
    Governing(EntityId),
}

/// One violated LOD invariant ([ADR 0017] §12).
///
/// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodViolation {
    /// In both the L1 and the L2 store.
    Duplicated(EntityId),
    /// A resident in neither store, or one that a transaction lost.
    Lost(EntityId),
    /// In a state store without an identity entry, or one that appeared
    /// across a transaction.
    Orphan(EntityId),
    /// In only one of the two L1 columns.
    PartialL1(EntityId),
    /// A procedural resident whose ID is not derived from its address.
    IdentityMismatch(EntityId),
    /// An identity entry (address or home) that changed across a
    /// transaction.
    IdentityChanged(EntityId),
    /// A resident's record names a building that does not exist.
    UnknownBuilding {
        /// The resident.
        id: EntityId,
        /// The missing building.
        building: EntityId,
    },
    /// A building with no level, or a level for something that is not a
    /// building.
    MissingNodeLod(EntityId),
    /// A building at a level TP-007 cannot run.
    InvalidNodeLod(EntityId),
    /// A resident whose residency differs from its governing building's
    /// level after a reconciliation.
    ResidencyMismatch {
        /// The resident.
        id: EntityId,
        /// Its governing building.
        governing: EntityId,
        /// That building's level.
        level: Lod,
    },
    /// The event queue differs from its rebuild from the L2 store for
    /// this resident.
    QueueMismatch(EntityId),
    /// The derived index of unreconciled residents differs from its
    /// rebuild (residency against the governing building's level) for this
    /// resident.
    UnreconciledMismatch(EntityId),
    /// An invalid L2 schedule.
    BadSchedule {
        /// The resident.
        id: EntityId,
        /// What is wrong.
        problem: ScheduleProblem,
    },
    /// An arrived L1 agent that is not exactly at its target's centre.
    NotAtCentre(EntityId),
    /// A position outside its own sector cell.
    NotInOwnCell(EntityId),
    /// A count that differs across a transaction or tick.
    CountMismatch {
        /// What was counted.
        quantity: Quantity,
        /// Before.
        before: u64,
        /// After.
        after: u64,
    },
    /// A next-target counter that changed across a transaction, or went
    /// backwards across a tick.
    CounterChanged(EntityId),
}

impl fmt::Display for LodViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicated(id) => write!(f, "{id} is in both the L1 and the L2 store"),
            Self::Lost(id) => write!(f, "{id} is in neither the L1 nor the L2 store"),
            Self::Orphan(id) => write!(f, "{id} has state but no identity"),
            Self::PartialL1(id) => write!(f, "{id} is in only one L1 column"),
            Self::IdentityMismatch(id) => write!(f, "{id} is not derived from its address"),
            Self::IdentityChanged(id) => write!(f, "the identity of {id} changed"),
            Self::UnknownBuilding { id, building } => {
                write!(f, "{id} refers to {building}, which is not a building")
            }
            Self::MissingNodeLod(id) => write!(f, "{id} has no level, or is not a building"),
            Self::InvalidNodeLod(id) => write!(f, "building {id} is at an unsupported level"),
            Self::ResidencyMismatch {
                id,
                governing,
                level,
            } => write!(f, "{id} is not at {level:?}, the level of {governing}"),
            Self::QueueMismatch(id) => write!(f, "the event queue entry of {id} is wrong"),
            Self::UnreconciledMismatch(id) => {
                write!(f, "the unreconciled-residency entry of {id} is wrong")
            }
            Self::BadSchedule { id, problem } => write!(f, "{id} has a bad schedule: {problem:?}"),
            Self::NotAtCentre(id) => write!(f, "{id} has arrived but is not at the centre"),
            Self::NotInOwnCell(id) => write!(f, "{id} is outside its own cell"),
            Self::CountMismatch {
                quantity,
                before,
                after,
            } => write!(f, "{quantity:?} changed from {before} to {after}"),
            Self::CounterChanged(id) => write!(f, "the target counter of {id} changed"),
        }
    }
}

/// Why a transition or an L2 event could not be applied. The world is
/// unchanged whenever one is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodError {
    /// A request names something that is not a building.
    UnknownNode(EntityId),
    /// A request lists a building more than once (a request is a set).
    DuplicateNode(EntityId),
    /// A requested level TP-007 cannot run.
    UnsupportedLevel(Lod),
    /// A time that is not a tick boundary of the base step.
    OffGrid(SimTime),
    /// Reconstructing a position failed.
    Coord(CoordError),
    /// A time would leave the representable range.
    Time(TimeOverflow),
    /// A next-target counter would pass `u32::MAX`.
    CounterExhausted(EntityId),
    /// The state about to change already violates an invariant; it is
    /// reported, never repaired.
    Invariant(LodViolation),
}

impl fmt::Display for LodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(f, "{id} is not a building"),
            Self::DuplicateNode(id) => write!(f, "building {id} is listed twice"),
            Self::UnsupportedLevel(l) => write!(f, "level {l:?} is not supported"),
            Self::OffGrid(t) => write!(f, "{t} is not a base-step boundary"),
            Self::Coord(e) => write!(f, "coordinate error: {e}"),
            Self::Time(e) => write!(f, "{e}"),
            Self::CounterExhausted(id) => write!(f, "the target counter of {id} is exhausted"),
            Self::Invariant(v) => write!(f, "invariant violated: {v}"),
        }
    }
}

impl std::error::Error for LodError {}

impl From<CoordError> for LodError {
    fn from(e: CoordError) -> Self {
        Self::Coord(e)
    }
}

impl From<TimeOverflow> for LodError {
    fn from(e: TimeOverflow) -> Self {
        Self::Time(e)
    }
}

/// How many agents a transaction moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransitionReport {
    /// Agents moved from L1 to L2.
    pub demoted: usize,
    /// Agents moved from L2 to L1.
    pub promoted: usize,
}

/// The conserved quantities of a world, in canonical order, for comparing
/// before and after a transaction or a tick ([ADR 0017] §5).
///
/// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConservationSummary {
    /// Residents across both stores.
    pub population: u64,
    /// Every resident in the identity column, with its address and home.
    pub identities: BTreeMap<EntityId, (ids::ResidentAddress, EntityId)>,
    /// Residents held by a state store (L1 or L2).
    pub resident_ids: BTreeSet<EntityId>,
    /// Residents per home building.
    pub per_home: BTreeMap<EntityId, u64>,
    /// Residents per governing building.
    pub per_governing: BTreeMap<EntityId, u64>,
    /// Each resident's next-target counter.
    pub counters: BTreeMap<EntityId, u32>,
}

impl ConservationSummary {
    /// Violations of the transaction rules: everything exact.
    #[must_use]
    pub fn compare_transaction(&self, after: &Self) -> Vec<LodViolation> {
        let mut out = self.compare_common(after);
        compare_counts(
            &self.per_governing,
            &after.per_governing,
            Quantity::Governing,
            &mut out,
        );
        for (id, before) in &self.counters {
            if after.counters.get(id).is_some_and(|a| a != before) {
                out.push(LodViolation::CounterChanged(*id));
            }
        }
        out
    }

    /// Violations of the tick rules: population, identity, and homes
    /// exact; counters never decrease.
    #[must_use]
    pub fn compare_tick(&self, after: &Self) -> Vec<LodViolation> {
        let mut out = self.compare_common(after);
        for (id, before) in &self.counters {
            if after.counters.get(id).is_some_and(|a| a < before) {
                out.push(LodViolation::CounterChanged(*id));
            }
        }
        out
    }

    fn compare_common(&self, after: &Self) -> Vec<LodViolation> {
        let mut out = Vec::new();
        if self.population != after.population {
            out.push(LodViolation::CountMismatch {
                quantity: Quantity::Population,
                before: self.population,
                after: after.population,
            });
        }
        for id in self.resident_ids.difference(&after.resident_ids) {
            out.push(LodViolation::Lost(*id));
        }
        for id in after.resident_ids.difference(&self.resident_ids) {
            out.push(LodViolation::Orphan(*id));
        }
        for (id, identity) in &self.identities {
            if after.identities.get(id) != Some(identity) {
                out.push(LodViolation::IdentityChanged(*id));
            }
        }
        compare_counts(&self.per_home, &after.per_home, Quantity::Home, &mut out);
        out
    }
}

fn compare_counts(
    before: &BTreeMap<EntityId, u64>,
    after: &BTreeMap<EntityId, u64>,
    quantity: fn(EntityId) -> Quantity,
    out: &mut Vec<LodViolation>,
) {
    let keys: BTreeSet<EntityId> = before.keys().chain(after.keys()).copied().collect();
    for k in keys {
        let (b, a) = (
            before.get(&k).copied().unwrap_or(0),
            after.get(&k).copied().unwrap_or(0),
        );
        if a != b {
            out.push(LodViolation::CountMismatch {
                quantity: quantity(k),
                before: b,
                after: a,
            });
        }
    }
}

/// Agents to move, with their new states: the planned part of a
/// transaction.
#[derive(Debug, Default)]
pub(crate) struct Moves {
    to_l2: Vec<(EntityId, Background)>,
    to_l1: Vec<(EntityId, SurfacePos, Walk)>,
}

/// The records a committed set of moves replaced, to undo it exactly.
#[derive(Debug, Default)]
pub(crate) struct Undo {
    old_l1: Vec<(EntityId, SurfacePos, Walk)>,
    old_l2: Vec<(EntityId, Background)>,
}

/// The planned result of draining L2 events to a horizon.
#[derive(Debug, Default)]
pub(crate) struct Drain {
    arrivals: Vec<ArrivalFact>,
    /// Queue entries to remove (each touched agent's entry before the
    /// drain).
    stale: Vec<(SimTime, EntityId)>,
    /// Touched agents' new records.
    updated: Vec<(EntityId, Background)>,
}

/// Bit-for-bit equality of two positions (`-0.0` and `0.0` differ).
pub(crate) fn same_bits(a: SurfacePos, b: SurfacePos) -> bool {
    a.cell() == b.cell()
        && a.offset()
            .iter()
            .zip(b.offset())
            .all(|(x, y)| x.to_bits() == y.to_bits())
}

/// `t + k·step`, checked.
fn after_steps(t: SimTime, k: i64, step: Rate) -> Result<SimTime, TimeOverflow> {
    t.checked_add(step.step().checked_mul(k)?)
}

/// The number of whole steps from `from` to `to` (negative if `to` is
/// earlier), or `None` if the difference is not a whole number of steps.
fn steps_between(from: SimTime, to: SimTime, step: Rate) -> Result<Option<i64>, TimeOverflow> {
    let d = to.checked_duration_since(from)?.as_micros();
    let s = step.step().as_micros();
    Ok((d % s == 0).then_some(d / s))
}

/// Steps a straight leg of `dist` metres takes at `step_m` metres per
/// step: `max(1, ⌈dist / step_m⌉)`, as L1's arrival rule gives (arrive in
/// the step that starts within one step of the target).
fn leg_steps(dist: f64, step_m: f64) -> Result<i64, TimeOverflow> {
    let q = (dist / step_m).ceil();
    // i64::MAX as f64 rounds up to 2^63, which no i64 reaches.
    if !q.is_finite() || q >= i64::MAX as f64 {
        return Err(TimeOverflow);
    }
    // In range and integral, so the cast is exact.
    Ok((q as i64).max(1))
}

/// The ENU velocity, in `pos`'s cell frame, of an agent at `pos` walking
/// to `goal` at `speed`; zero at the goal. Derived state (ADR 0017 §10).
fn velocity_towards(pos: SurfacePos, goal: Position<Ecef>, speed: f64) -> [f32; 3] {
    let frame = CellFrame::new(pos.cell());
    let to_goal = frame.frame().to_enu(goal) - pos.offset_enu();
    let dist = to_goal.length();
    if dist > 0.0 && dist.is_finite() {
        let v = to_goal * (speed / dist);
        [v.x as f32, v.y as f32, v.z as f32]
    } else {
        [0.0; 3]
    }
}

impl ToyWorld {
    /// The level of building `id`, if it is a building.
    #[must_use]
    pub fn node_lod(&self, id: EntityId) -> Option<Lod> {
        self.node_lod.get(id).copied()
    }

    /// Building levels, by building ID.
    #[must_use]
    pub const fn node_lods(&self) -> &ws_ecs::Column<Lod> {
        &self.node_lod
    }

    /// L2 agents' schedules, by ID.
    #[must_use]
    pub const fn background(&self) -> &ws_ecs::Column<Background> {
        &self.background
    }

    /// The L2 event queue: one `(next event time, ID)` per L2 agent, in
    /// processing order. Derived from [`ToyWorld::background`]; not hashed.
    #[must_use]
    pub const fn event_queue(&self) -> &BTreeSet<(SimTime, EntityId)> {
        &self.queue
    }

    /// The level an agent is represented at: [`Lod::L1`] or [`Lod::L2`],
    /// or `None` if it is in neither store.
    #[must_use]
    pub fn residency(&self, id: EntityId) -> Option<Lod> {
        if self.walks.contains(id) {
            Some(Lod::L1)
        } else if self.background.contains(id) {
            Some(Lod::L2)
        } else {
            None
        }
    }

    /// The building governing an agent's level: the one it waits at or
    /// walks to.
    #[must_use]
    pub fn governing_building(&self, id: EntityId) -> Option<EntityId> {
        self.walks
            .get(id)
            .map(|w| w.target)
            .or_else(|| self.background.get(id).map(Background::governing))
    }

    /// Metres per base step, computed exactly as the L1 movement system
    /// computes it.
    pub(crate) fn step_m(&self, step: SimDuration) -> f64 {
        let dt_s = step.as_micros() as f64 * 1e-6;
        self.walk.speed_m_s * dt_s
    }

    fn centre(&self, id: EntityId, building: EntityId) -> Result<SurfacePos, LodError> {
        self.buildings
            .get(building)
            .map(|b| b.pos)
            .ok_or(LodError::Invariant(LodViolation::UnknownBuilding {
                id,
                building,
            }))
    }

    fn origin_pos(&self, id: EntityId, origin: Origin) -> Result<SurfacePos, LodError> {
        match origin {
            Origin::Building(b) => self.centre(id, b),
            Origin::Point { cell, offset } => Ok(SurfacePos::new(cell, offset)?),
        }
    }

    fn check_grid(now: SimTime, step: Rate) -> Result<(), LodError> {
        if step.is_tick_start(now) {
            Ok(())
        } else {
            Err(LodError::OffGrid(now))
        }
    }

    /// L1 → L2 for one agent at boundary `now` ([ADR 0017] §2). Draws
    /// nothing and reads no other agent.
    ///
    /// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
    fn demote(
        &self,
        id: EntityId,
        pos: SurfacePos,
        walk: Walk,
        now: SimTime,
        step: Rate,
    ) -> Result<Background, LodError> {
        let centre = self.centre(id, walk.target)?;
        let activity = if walk.arrived {
            if !same_bits(pos, centre) {
                return Err(LodError::Invariant(LodViolation::NotAtCentre(id)));
            }
            Activity::Waiting {
                at: walk.target,
                depart: after_steps(now, i64::from(walk.wait_steps) + 1, step)?,
            }
        } else {
            let rem = (centre.to_ecef() - pos.to_ecef()).length();
            let n = leg_steps(rem, self.step_m(step.step()))?;
            Activity::Walking {
                origin: Origin::point(pos),
                origin_time: now,
                dest: walk.target,
                arrive: after_steps(now, n, step)?,
            }
        };
        Ok(Background {
            activity,
            targets_drawn: walk.targets_drawn,
        })
    }

    /// L2 → L1 for one agent at boundary `now` ([ADR 0017] §2).
    ///
    /// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
    fn promote(
        &self,
        id: EntityId,
        bg: Background,
        now: SimTime,
        step: Rate,
    ) -> Result<(SurfacePos, Walk), LodError> {
        let bad = |problem| LodError::Invariant(LodViolation::BadSchedule { id, problem });
        match bg.activity {
            Activity::Waiting { at, depart } => {
                let k = steps_between(now, depart, step)?.ok_or(bad(ScheduleProblem::OffGrid))?;
                if k < 1 {
                    return Err(bad(ScheduleProblem::EventNotInFuture));
                }
                let wait_steps =
                    u32::try_from(k - 1).map_err(|_| bad(ScheduleProblem::WaitTooLong))?;
                Ok((
                    self.centre(id, at)?,
                    Walk {
                        velocity: [0.0; 3],
                        target: at,
                        arrived: true,
                        wait_steps,
                        targets_drawn: bg.targets_drawn,
                    },
                ))
            }
            Activity::Walking {
                origin,
                origin_time,
                dest,
                arrive,
            } => {
                let m =
                    steps_between(origin_time, now, step)?.ok_or(bad(ScheduleProblem::OffGrid))?;
                let n = steps_between(origin_time, arrive, step)?
                    .ok_or(bad(ScheduleProblem::OffGrid))?;
                if m < 0 {
                    return Err(bad(ScheduleProblem::OriginInFuture));
                }
                if m >= n {
                    return Err(bad(ScheduleProblem::EventNotInFuture));
                }
                let goal = self.centre(id, dest)?.to_ecef();
                let start = self.origin_pos(id, origin)?;
                let pos = if m == 0 {
                    start
                } else {
                    let o = start.to_ecef();
                    let d = goal - o;
                    let u = d * (1.0 / d.length());
                    SurfacePos::from_ecef(o + u * (m as f64 * self.step_m(step.step())))?
                };
                Ok((
                    pos,
                    Walk {
                        velocity: velocity_towards(pos, goal, self.walk.speed_m_s),
                        target: dest,
                        arrived: false,
                        wait_steps: 0,
                        targets_drawn: bg.targets_drawn,
                    },
                ))
            }
        }
    }

    /// Plans the moves that make every agent's residency equal its
    /// governing building's level under `level_of`, in ascending ID order.
    /// Changes nothing.
    fn plan_reconcile(
        &self,
        now: SimTime,
        step: Rate,
        level_of: impl Fn(EntityId) -> Option<Lod>,
    ) -> Result<Moves, LodError> {
        let mut moves = Moves::default();
        for (id, walk, pos) in join(&self.walks, &self.positions) {
            self.plan_l1_agent(&mut moves, id, *walk, *pos, now, step, &level_of)?;
        }
        for (id, bg) in self.background.iter() {
            self.plan_l2_agent(&mut moves, id, *bg, now, step, &level_of)?;
        }
        Ok(moves)
    }

    /// As [`Self::plan_reconcile`] under the current node levels, but only
    /// for the agents in the derived `unreconciled` index, which holds
    /// exactly the agents whose residency differs from their governing
    /// building's level. A tick with nothing to reconcile costs nothing
    /// here (ADR 0017 §15).
    fn plan_reconcile_pending(&self, now: SimTime, step: Rate) -> Result<Moves, LodError> {
        let level_of = |b: EntityId| self.node_lod.get(b).copied();
        let mut moves = Moves::default();
        for &id in &self.unreconciled {
            match (
                self.walks.get(id),
                self.positions.get(id),
                self.background.get(id),
            ) {
                (Some(walk), Some(pos), None) => {
                    self.plan_l1_agent(&mut moves, id, *walk, *pos, now, step, &level_of)?;
                }
                (None, None, Some(bg)) => {
                    self.plan_l2_agent(&mut moves, id, *bg, now, step, &level_of)?;
                }
                (None, None, None) => {
                    return Err(LodError::Invariant(LodViolation::Lost(id)));
                }
                (Some(_), None, _) | (None, Some(_), _) => {
                    return Err(LodError::Invariant(LodViolation::PartialL1(id)));
                }
                (Some(_), Some(_), Some(_)) => {
                    return Err(LodError::Invariant(LodViolation::Duplicated(id)));
                }
            }
        }
        Ok(moves)
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_l1_agent(
        &self,
        moves: &mut Moves,
        id: EntityId,
        walk: Walk,
        pos: SurfacePos,
        now: SimTime,
        step: Rate,
        level_of: &impl Fn(EntityId) -> Option<Lod>,
    ) -> Result<(), LodError> {
        let level =
            level_of(walk.target).ok_or(LodError::Invariant(LodViolation::UnknownBuilding {
                id,
                building: walk.target,
            }))?;
        if level == Lod::L2 {
            if self.background.contains(id) {
                return Err(LodError::Invariant(LodViolation::Duplicated(id)));
            }
            moves
                .to_l2
                .push((id, self.demote(id, pos, walk, now, step)?));
        }
        Ok(())
    }

    fn plan_l2_agent(
        &self,
        moves: &mut Moves,
        id: EntityId,
        bg: Background,
        now: SimTime,
        step: Rate,
        level_of: &impl Fn(EntityId) -> Option<Lod>,
    ) -> Result<(), LodError> {
        let building = bg.governing();
        let level =
            level_of(building).ok_or(LodError::Invariant(LodViolation::UnknownBuilding {
                id,
                building,
            }))?;
        if level == Lod::L1 {
            if self.walks.contains(id) || self.positions.contains(id) {
                return Err(LodError::Invariant(LodViolation::Duplicated(id)));
            }
            let (pos, walk) = self.promote(id, bg, now, step)?;
            moves.to_l1.push((id, pos, walk));
        }
        Ok(())
    }

    /// Keeps the derived `unreconciled` index current for one agent whose
    /// governing building or store has just been set.
    pub(crate) fn note_residency(&mut self, id: EntityId, governing: EntityId, residency: Lod) {
        if self.node_lod.get(governing).copied() == Some(residency) {
            self.unreconciled.remove(&id);
        } else {
            self.unreconciled.insert(id);
        }
    }

    /// Moves agents into the L2 store, returning the L1 records removed.
    /// The plan has checked that each is in L1 and not in L2.
    fn move_to_l2(
        &mut self,
        to_l2: Vec<(EntityId, Background)>,
    ) -> Vec<(EntityId, SurfacePos, Walk)> {
        if to_l2.is_empty() {
            return Vec::new();
        }
        let ids: Vec<EntityId> = to_l2.iter().map(|(id, _)| *id).collect();
        let positions = self.positions.remove_batch(&ids);
        let walks = self.walks.remove_batch(&ids);
        assert!(
            positions.len() == ids.len() && walks.len() == ids.len(),
            "LOD commit: every demoted agent was in both L1 columns"
        );
        for (id, bg) in &to_l2 {
            self.queue.insert((bg.next_event(), *id));
        }
        self.background
            .insert_batch(to_l2)
            .expect("LOD commit: no demoted agent was in L2");
        positions
            .into_iter()
            .zip(walks)
            .map(|((id, p), (_, w))| (id, p, w))
            .collect()
    }

    /// Moves agents into the L1 store, returning the L2 records removed.
    /// The plan has checked that each is in L2 and not in L1.
    fn move_to_l1(
        &mut self,
        to_l1: Vec<(EntityId, SurfacePos, Walk)>,
    ) -> Vec<(EntityId, Background)> {
        if to_l1.is_empty() {
            return Vec::new();
        }
        let ids: Vec<EntityId> = to_l1.iter().map(|(id, _, _)| *id).collect();
        let removed = self.background.remove_batch(&ids);
        assert!(
            removed.len() == ids.len(),
            "LOD commit: every promoted agent was in L2"
        );
        for (id, bg) in &removed {
            let found = self.queue.remove(&(bg.next_event(), *id));
            assert!(found, "LOD commit: the event queue held {id}");
        }
        let (positions, walks): (Vec<_>, Vec<_>) = to_l1
            .into_iter()
            .map(|(id, p, w)| ((id, p), (id, w)))
            .unzip();
        self.positions
            .insert_batch(positions)
            .expect("LOD commit: no promoted agent was in L1");
        self.walks
            .insert_batch(walks)
            .expect("LOD commit: no promoted agent was in L1");
        removed
    }

    fn commit_moves(&mut self, moves: Moves) -> Undo {
        Undo {
            old_l1: self.move_to_l2(moves.to_l2),
            old_l2: self.move_to_l1(moves.to_l1),
        }
    }

    fn undo_moves(&mut self, undo: Undo) {
        self.move_to_l1(undo.old_l1);
        self.move_to_l2(undo.old_l2);
    }

    /// Applies a transition request at tick boundary `now` as one
    /// transaction ([ADR 0017] §2): sets the listed buildings' levels and
    /// moves every agent whose residency then differs from its governing
    /// building's level (in ascending ID order). Restating a building's
    /// current level is a no-op. Request order does not matter.
    ///
    /// # Errors
    ///
    /// The world is unchanged on every error:
    /// [`LodError::OffGrid`] if `now` is not a boundary of `step`;
    /// [`LodError::DuplicateNode`] if a building is listed twice (even with
    /// the same level); [`LodError::UnknownNode`] and
    /// [`LodError::UnsupportedLevel`] per entry, in ascending ID order;
    /// [`LodError::Invariant`] if an affected agent's state is already
    /// invalid; [`LodError::Coord`] and [`LodError::Time`] if a
    /// reconstruction fails.
    ///
    /// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
    pub fn set_lod(
        &mut self,
        now: SimTime,
        step: Rate,
        request: &[(EntityId, Lod)],
    ) -> Result<TransitionReport, LodError> {
        Ok(self.prepare_lod(now, step, request)?.commit())
    }

    /// Plans an atomic LOD transaction and typed per-building changes.
    pub fn prepare_lod(
        &mut self,
        now: SimTime,
        step: Rate,
        request: &[(EntityId, Lod)],
    ) -> Result<PreparedLod<'_>, LodError> {
        Self::check_grid(now, step)?;
        let mut req = request.to_vec();
        req.sort_unstable_by_key(|(id, _)| *id);
        if let Some(w) = req.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(LodError::DuplicateNode(w[0].0));
        }
        for (b, level) in &req {
            if !self.buildings.contains(*b) {
                return Err(LodError::UnknownNode(*b));
            }
            if !level.is_executable() {
                return Err(LodError::UnsupportedLevel(*level));
            }
        }
        let level_of = |b: EntityId| match req.binary_search_by_key(&b, |(id, _)| *id) {
            Ok(i) => Some(req[i].1),
            Err(_) => self.node_lod.get(b).copied(),
        };
        let moves = self.plan_reconcile(now, step, level_of)?;

        let mut changes = Vec::new();
        for (building, after) in &req {
            let before = self.node_lod(*building).expect("validated building");
            if before != *after {
                changes.push(LodChange {
                    building: *building,
                    before,
                    after: *after,
                    demoted: moves
                        .to_l2
                        .iter()
                        .filter(|(_, bg)| bg.governing() == *building)
                        .count(),
                    promoted: moves
                        .to_l1
                        .iter()
                        .filter(|(_, _, w)| w.target == *building)
                        .count(),
                    position: self.buildings.get(*building).expect("validated").pos,
                });
            }
        }
        Ok(PreparedLod {
            world: self,
            req,
            moves,
            changes,
            #[cfg(debug_assertions)]
            now,
            #[cfg(debug_assertions)]
            step,
        })
    }

    /// Plans the L2 events with times up to and including `to`, in
    /// `(time, ID)` order, including events they schedule within the
    /// horizon. Changes nothing.
    fn plan_drain(&self, to: SimTime, step: Rate) -> Result<Drain, LodError> {
        let mut pending: BTreeSet<(SimTime, EntityId)> = self
            .queue
            .iter()
            .take_while(|(t, _)| *t <= to)
            .copied()
            .collect();
        let mut drain = Drain::default();
        let mut working: BTreeMap<EntityId, Background> = BTreeMap::new();
        while let Some((t, id)) = pending.pop_first() {
            let bg = match working.get(&id) {
                Some(bg) => *bg,
                None => {
                    let bg = *self
                        .background
                        .get(id)
                        .ok_or(LodError::Invariant(LodViolation::QueueMismatch(id)))?;
                    drain.stale.push((t, id));
                    bg
                }
            };
            if bg.next_event() != t {
                return Err(LodError::Invariant(LodViolation::QueueMismatch(id)));
            }
            let next = self.process_event(id, bg, step)?;
            if let Activity::Walking { dest, arrive, .. } = bg.activity {
                drain.arrivals.push(ArrivalFact {
                    resident: id,
                    destination: dest,
                    leg: bg.targets_drawn,
                    time: Some(arrive),
                    lod: Lod::L2,
                    position: self.centre(id, dest)?,
                });
            }
            if next.next_event() <= to {
                pending.insert((next.next_event(), id));
            }
            working.insert(id, next);
        }
        drain.updated = working.into_iter().collect();
        Ok(drain)
    }

    /// One L2 event: a departure draws the next target (the agent's own
    /// counter stream, exactly as L1) and starts a leg; an arrival starts a
    /// wait ([ADR 0017] §7).
    ///
    /// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
    fn process_event(
        &self,
        id: EntityId,
        bg: Background,
        step: Rate,
    ) -> Result<Background, LodError> {
        match bg.activity {
            Activity::Waiting { at, depart } => {
                let k = bg.targets_drawn;
                let (dest, targets_drawn) = if self.illness.contains(id) {
                    // ADR 0021 §4: the same rule as the L1 movement system.
                    let home = self
                        .residents
                        .get(id)
                        .ok_or(LodError::UnknownNode(id))?
                        .home;
                    if home == at {
                        return Ok(Background {
                            activity: Activity::Waiting {
                                at,
                                depart: after_steps(
                                    depart,
                                    i64::from(self.walk.wait_steps) + 1,
                                    step,
                                )?,
                            },
                            targets_drawn: k,
                        });
                    }
                    (home, k)
                } else {
                    (
                        ids::next_target(self.seed, id, k, self.buildings.ids(), at),
                        k.checked_add(1).ok_or(LodError::CounterExhausted(id))?,
                    )
                };
                let origin_time = after_steps(depart, -1, step)?;
                let o = self.centre(id, at)?.to_ecef();
                let g = self.centre(id, dest)?.to_ecef();
                let n = leg_steps((g - o).length(), self.step_m(step.step()))?;
                Ok(Background {
                    activity: Activity::Walking {
                        origin: Origin::Building(at),
                        origin_time,
                        dest,
                        arrive: after_steps(origin_time, n, step)?,
                    },
                    targets_drawn,
                })
            }
            Activity::Walking { dest, arrive, .. } => Ok(Background {
                activity: Activity::Waiting {
                    at: dest,
                    depart: after_steps(arrive, i64::from(self.walk.wait_steps) + 1, step)?,
                },
                targets_drawn: bg.targets_drawn,
            }),
        }
    }

    fn commit_drain(&mut self, drain: Drain) {
        for entry in &drain.stale {
            let found = self.queue.remove(entry);
            assert!(found, "L2 drain commit: the event queue held {entry:?}");
        }
        for (id, bg) in drain.updated {
            self.queue.insert((bg.next_event(), id));
            *self
                .background
                .get_mut(id)
                .expect("L2 drain commit: the agent is in L2") = bg;
            self.note_residency(id, bg.governing(), Lod::L2);
        }
    }

    /// Processes every L2 event with a time up to and including `to`, in
    /// `(time, ID)` order. The result depends only on the event times, not
    /// on how a span is split into calls (cadence independence, ADR 0017
    /// §7). Does not reconcile residency.
    ///
    /// # Errors
    ///
    /// As the events ([`LodError::CounterExhausted`],
    /// [`LodError::Time`], [`LodError::Invariant`]); the world is then
    /// unchanged.
    pub fn advance_background(&mut self, to: SimTime, step: Rate) -> Result<(), LodError> {
        let drain = self.plan_drain(to, step)?;
        self.commit_drain(drain);
        Ok(())
    }

    /// Runs one tick from boundary `now` to `now + step` in the canonical
    /// order ([ADR 0017] §8): reconcile residency at `now`, run the L1
    /// movement system, then drain L2 events to `now + step`. Atomic: on
    /// error the world is unchanged.
    ///
    /// # Errors
    ///
    /// [`WorldError::Lod`] from reconciliation or the drain (including
    /// [`LodError::OffGrid`] if `now` is not a boundary of `step`), and the
    /// movement system's errors.
    ///
    /// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
    pub fn tick(&mut self, now: SimTime, step: Rate) -> Result<(), WorldError> {
        self.prepare_tick(now, step)?.commit();
        Ok(())
    }

    /// Prepares a tick and its arrival facts; dropping it rolls back reconciliation.
    /// Reserve external history storage before committing this plan.
    pub fn prepare_tick(
        &mut self,
        now: SimTime,
        step: Rate,
    ) -> Result<PreparedTick<'_>, WorldError> {
        Self::check_grid(now, step)?;
        let to = now.checked_add(step.step()).map_err(LodError::Time)?;
        #[cfg(debug_assertions)]
        let before = self.conservation_summary();
        let moves = self.plan_reconcile_pending(now, step)?;
        let undo = self.commit_moves(moves);
        let pending = core::mem::take(&mut self.unreconciled);
        let movement = match self.plan_movement(step.step()) {
            Ok(m) => m,
            Err(e) => {
                self.undo_moves(undo);
                self.unreconciled = pending;
                return Err(e);
            }
        };
        let drain = match self.plan_drain(to, step) {
            Ok(d) => d,
            Err(e) => {
                self.undo_moves(undo);
                self.unreconciled = pending;
                return Err(e.into());
            }
        };
        Ok(PreparedTick {
            world: self,
            movement: Some(movement),
            drain: Some(drain),
            undo: Some(undo),
            pending: Some(pending),
            #[cfg(debug_assertions)]
            to,
            #[cfg(debug_assertions)]
            step,
            #[cfg(debug_assertions)]
            before,
        })
    }

    /// The conserved quantities, for comparing before and after a
    /// transaction or tick.
    #[must_use]
    pub fn conservation_summary(&self) -> ConservationSummary {
        let mut s = ConservationSummary {
            population: 0,
            identities: self
                .residents
                .iter()
                .map(|(id, r)| (id, (r.address, r.home)))
                .collect(),
            resident_ids: BTreeSet::new(),
            per_home: BTreeMap::new(),
            per_governing: BTreeMap::new(),
            counters: BTreeMap::new(),
        };
        let l1 = self
            .walks
            .iter()
            .map(|(id, w)| (id, w.target, w.targets_drawn));
        let l2 = self
            .background
            .iter()
            .map(|(id, bg)| (id, bg.governing(), bg.targets_drawn));
        for (id, governing, counter) in l1.chain(l2) {
            s.population += 1;
            s.resident_ids.insert(id);
            *s.per_governing.entry(governing).or_default() += 1;
            s.counters.insert(id, counter);
            if let Some(r) = self.residents.get(id) {
                *s.per_home.entry(r.home).or_default() += 1;
            }
        }
        s
    }

    /// Checks every LOD invariant of the world at boundary `now` and
    /// reports all violations, in ascending ID order within each kind
    /// ([ADR 0017] §12). With `reconciled`, also checks that every agent's
    /// residency equals its governing building's level, which holds after
    /// [`ToyWorld::set_lod`] and after the reconciliation at the start of a
    /// tick, but not necessarily after the movement and drain phases.
    ///
    /// O(n log n) in the number of agents: debug builds run it after every
    /// transaction and tick; release builds only when asked.
    ///
    /// # Errors
    ///
    /// Every violation found.
    ///
    /// [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md
    pub fn check_lod_invariants(
        &self,
        now: SimTime,
        step: Rate,
        reconciled: bool,
    ) -> Result<(), Vec<LodViolation>> {
        let mut v = Vec::new();
        self.check_nodes(&mut v);
        self.check_membership(&mut v);
        self.check_identity(&mut v);
        self.check_l1(&mut v);
        self.check_l2(now, step, &mut v);
        self.check_queue(&mut v);
        self.check_unreconciled(&mut v);
        if reconciled {
            self.check_residency(&mut v);
        }
        if v.is_empty() { Ok(()) } else { Err(v) }
    }

    fn check_nodes(&self, v: &mut Vec<LodViolation>) {
        let buildings: BTreeSet<EntityId> = self.buildings.ids().iter().copied().collect();
        let levels: BTreeSet<EntityId> = self.node_lod.ids().iter().copied().collect();
        for id in buildings.symmetric_difference(&levels) {
            v.push(LodViolation::MissingNodeLod(*id));
        }
        for (id, level) in self.node_lod.iter() {
            if !level.is_executable() {
                v.push(LodViolation::InvalidNodeLod(id));
            }
        }
    }

    fn check_membership(&self, v: &mut Vec<LodViolation>) {
        for (id, _) in self.residents.iter() {
            let (p, w, b) = (
                self.positions.contains(id),
                self.walks.contains(id),
                self.background.contains(id),
            );
            if p != w {
                v.push(LodViolation::PartialL1(id));
            }
            if (p || w) && b {
                v.push(LodViolation::Duplicated(id));
            }
            if !p && !w && !b {
                v.push(LodViolation::Lost(id));
            }
        }
        let stored: BTreeSet<EntityId> = self
            .positions
            .ids()
            .iter()
            .chain(self.walks.ids())
            .chain(self.background.ids())
            .copied()
            .collect();
        for id in stored {
            if !self.residents.contains(id) {
                v.push(LodViolation::Orphan(id));
            }
        }
    }

    fn check_identity(&self, v: &mut Vec<LodViolation>) {
        for (id, b) in self.buildings.iter() {
            if id.namespace() == Namespace::Spawned && id.payload() >= self.next_spawned_id {
                v.push(LodViolation::IdentityMismatch(id));
            }
            if b.pos.occupied_cell() != Ok(b.pos.cell()) {
                v.push(LodViolation::NotInOwnCell(id));
            }
        }
        for (id, r) in self.residents.iter() {
            if (id.namespace() == Namespace::Procedural
                && ids::resident_id(self.seed, self.dataset_version, r.address) != id)
                || (id.namespace() == Namespace::Spawned && id.payload() >= self.next_spawned_id)
            {
                v.push(LodViolation::IdentityMismatch(id));
            }
            if !self.buildings.contains(r.home) {
                v.push(LodViolation::UnknownBuilding {
                    id,
                    building: r.home,
                });
            }
        }
        for &id in &self.notables {
            if id.namespace() != Namespace::Procedural
                || (!self.buildings.contains(id) && !self.residents.contains(id))
                || self.tombstones.contains(id)
            {
                v.push(LodViolation::IdentityMismatch(id));
            }
        }
        for (id, t) in self.tombstones.iter() {
            let expected = match t.address {
                crate::BaselineAddress::Building(index) => {
                    ids::building_id(self.seed, self.dataset_version, index)
                }
                crate::BaselineAddress::Resident(addr) => {
                    ids::resident_id(self.seed, self.dataset_version, addr)
                }
            };
            if id.namespace() != Namespace::Procedural
                || expected != id
                || self.buildings.contains(id)
                || self.residents.contains(id)
            {
                v.push(LodViolation::IdentityMismatch(id));
            }
        }
    }

    fn check_l1(&self, v: &mut Vec<LodViolation>) {
        for (id, walk, pos) in join(&self.walks, &self.positions) {
            match self.buildings.get(walk.target) {
                None => v.push(LodViolation::UnknownBuilding {
                    id,
                    building: walk.target,
                }),
                Some(b) if walk.arrived && !same_bits(*pos, b.pos) => {
                    v.push(LodViolation::NotAtCentre(id));
                }
                Some(_) => {}
            }
            if !walk.arrived && walk.targets_drawn == 0 {
                v.push(LodViolation::BadSchedule {
                    id,
                    problem: ScheduleProblem::WalkingWithoutDraw,
                });
            }
            if pos.occupied_cell() != Ok(pos.cell()) {
                v.push(LodViolation::NotInOwnCell(id));
            }
        }
    }

    fn check_l2(&self, now: SimTime, step: Rate, v: &mut Vec<LodViolation>) {
        for (id, bg) in self.background.iter() {
            let mut problems = Vec::new();
            let on_grid = |t: SimTime| step.is_tick_start(t);
            match bg.activity {
                Activity::Waiting { depart, .. } => {
                    if !on_grid(depart) {
                        problems.push(ScheduleProblem::OffGrid);
                    } else if depart <= now {
                        problems.push(ScheduleProblem::EventNotInFuture);
                    }
                }
                Activity::Walking {
                    origin,
                    origin_time,
                    arrive,
                    ..
                } => {
                    if !on_grid(origin_time) || !on_grid(arrive) {
                        problems.push(ScheduleProblem::OffGrid);
                    } else if arrive <= now {
                        problems.push(ScheduleProblem::EventNotInFuture);
                    } else if origin_time > now {
                        problems.push(ScheduleProblem::OriginInFuture);
                    } else if arrive.checked_duration_since(origin_time).ok() < Some(step.step()) {
                        problems.push(ScheduleProblem::EmptyLeg);
                    }
                    if bg.targets_drawn == 0 {
                        problems.push(ScheduleProblem::WalkingWithoutDraw);
                    }
                    if let Origin::Point { cell, offset } = origin {
                        match SurfacePos::new(cell, offset) {
                            Ok(p) if p.occupied_cell() == Ok(cell) => {}
                            _ => v.push(LodViolation::NotInOwnCell(id)),
                        }
                    }
                }
            }
            v.extend(
                problems
                    .into_iter()
                    .map(|problem| LodViolation::BadSchedule { id, problem }),
            );
            let mut buildings = vec![bg.governing()];
            if let Activity::Walking {
                origin: Origin::Building(b),
                ..
            } = bg.activity
            {
                buildings.push(b);
            }
            for building in buildings {
                if !self.buildings.contains(building) {
                    v.push(LodViolation::UnknownBuilding { id, building });
                }
            }
        }
    }

    fn check_queue(&self, v: &mut Vec<LodViolation>) {
        let rebuilt: BTreeSet<(SimTime, EntityId)> = self
            .background
            .iter()
            .map(|(id, bg)| (bg.next_event(), id))
            .collect();
        let wrong: BTreeSet<EntityId> = rebuilt
            .symmetric_difference(&self.queue)
            .map(|(_, id)| *id)
            .collect();
        v.extend(wrong.into_iter().map(LodViolation::QueueMismatch));
    }

    fn check_unreconciled(&self, v: &mut Vec<LodViolation>) {
        let l1 = self.walks.iter().map(|(id, w)| (id, w.target, Lod::L1));
        let l2 = self
            .background
            .iter()
            .map(|(id, bg)| (id, bg.governing(), Lod::L2));
        let rebuilt: BTreeSet<EntityId> = l1
            .chain(l2)
            .filter(|(_, b, residency)| self.node_lod.get(*b) != Some(residency))
            .map(|(id, _, _)| id)
            .collect();
        v.extend(
            rebuilt
                .symmetric_difference(&self.unreconciled)
                .map(|id| LodViolation::UnreconciledMismatch(*id)),
        );
    }

    fn check_residency(&self, v: &mut Vec<LodViolation>) {
        let l1 = self.walks.iter().map(|(id, w)| (id, w.target, Lod::L1));
        let l2 = self
            .background
            .iter()
            .map(|(id, bg)| (id, bg.governing(), Lod::L2));
        let mut rows: Vec<_> = l1.chain(l2).collect();
        rows.sort_unstable_by_key(|(id, _, _)| *id);
        for (id, governing, residency) in rows {
            if let Some(level) = self.node_lod.get(governing).copied()
                && level != residency
            {
                v.push(LodViolation::ResidencyMismatch {
                    id,
                    governing,
                    level,
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "lod_tests.rs"]
mod tests;

/// A prepared tick, exclusively borrowing its world until commit or rollback.
pub struct PreparedTick<'a> {
    world: &'a mut ToyWorld,
    movement: Option<Movement>,
    drain: Option<Drain>,
    undo: Option<Undo>,
    pending: Option<BTreeSet<EntityId>>,
    #[cfg(debug_assertions)]
    to: SimTime,
    #[cfg(debug_assertions)]
    step: Rate,
    #[cfg(debug_assertions)]
    before: ConservationSummary,
}
impl PreparedTick<'_> {
    /// See [`ToyWorld::illness_root_if_home`].
    #[must_use]
    pub fn illness_root_if_home(&self, id: EntityId, destination: EntityId) -> Option<u64> {
        self.world.illness_root_if_home(id, destination)
    }
    /// Facts generated directly by the L1 and L2 domain plans.
    pub fn arrivals(&self) -> impl Iterator<Item = &ArrivalFact> {
        self.movement
            .as_ref()
            .expect("live plan")
            .1
            .iter()
            .chain(self.drain.as_ref().expect("live plan").arrivals.iter())
    }
    /// Commits the already validated domain plans, with no fallible history work.
    pub fn commit(mut self) {
        self.world
            .commit_movement(self.movement.take().expect("live plan"));
        self.world
            .commit_drain(self.drain.take().expect("live plan"));
        self.undo = None;
        self.pending = None;
        #[cfg(debug_assertions)]
        {
            let mut v = self.before.compare_tick(&self.world.conservation_summary());
            // The ending time is aligned; the original checker uses this grid.
            if let Err(more) = self.world.check_lod_invariants(self.to, self.step, false) {
                v.extend(more);
            }
            assert!(v.is_empty(), "LOD invariants violated by a tick: {v:?}");
        }
    }
}
impl Drop for PreparedTick<'_> {
    fn drop(&mut self) {
        if let Some(undo) = self.undo.take() {
            self.world.undo_moves(undo);
            self.world.unreconciled = self.pending.take().expect("rollback pending");
        }
    }
}
/// One building's actual level change, with exact affected-resident counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodChange {
    /// Building identity.
    pub building: EntityId,
    /// Previous level.
    pub before: Lod,
    /// Requested new level.
    pub after: Lod,
    /// Agents demoted at this building.
    pub demoted: usize,
    /// Agents promoted at this building.
    pub promoted: usize,
    /// Building position.
    pub position: SurfacePos,
}
/// A validated LOD transaction with no mutation before commit.
pub struct PreparedLod<'a> {
    world: &'a mut ToyWorld,
    req: Vec<(EntityId, Lod)>,
    moves: Moves,
    changes: Vec<LodChange>,
    #[cfg(debug_assertions)]
    now: SimTime,
    #[cfg(debug_assertions)]
    step: Rate,
}
impl PreparedLod<'_> {
    /// Actual changes, sorted by building ID; unchanged requests emit nothing.
    pub fn changes(&self) -> &[LodChange] {
        &self.changes
    }
    /// Counts the planned demotions and promotions.
    pub fn report(&self) -> TransitionReport {
        TransitionReport {
            demoted: self.moves.to_l2.len(),
            promoted: self.moves.to_l1.len(),
        }
    }
    /// Commits the validated transition.
    pub fn commit(self) -> TransitionReport {
        #[cfg(debug_assertions)]
        let before = self.world.conservation_summary();
        let report = self.report();
        for (b, level) in self.req {
            *self.world.node_lod.get_mut(b).expect("validated") = level;
        }
        self.world.commit_moves(self.moves);
        self.world.unreconciled.clear();
        #[cfg(debug_assertions)]
        {
            let mut v = before.compare_transaction(&self.world.conservation_summary());
            if let Err(more) = self.world.check_lod_invariants(self.now, self.step, true) {
                v.extend(more);
            }
            assert!(v.is_empty(), "LOD invariants violated by set_lod: {v:?}");
        }
        report
    }
}
