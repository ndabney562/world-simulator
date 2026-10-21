//! World models: the Technical Prototype toy world and the VS-002 real block.
//!
//! [`slice::SliceWorld`] imports the four ADR 0023 buildings as an integer
//! structural baseline. The toy model below remains available unchanged.
//!
//! A few box buildings at fixed offsets from the Bay Ridge reference point
//! ([`dataset`]) and a few dozen residents who walk between them. It proves
//! the entity architecture, not content:
//!
//! - **Identity (Tier 1).** Buildings and residents have procedural
//!   [`EntityId`]s derived from `(seed, dataset version, address)`
//!   ([`ids`]); residents' counts per building are Tier 1 integer decisions.
//! - **Storage.** Components live in `ws-ecs` columns and every system
//!   visits entities in ascending ID order, so insertion order cannot change
//!   anything.
//! - **Movement (Tier 2).** Agents walk at a fixed speed to a target
//!   building, wait there, and draw their next target from their own
//!   counter-based stream ([`ids::next_target`]). The `k`-th target depends
//!   only on `(seed, agent, k)` and where the agent is, never on the tick it
//!   arrived on. Positions are re-expressed into the containing cell once
//!   per agent at the end of each step, so every agent lies in its own cell
//!   after every step (ADR 0016 §4).
//! - **Simulation LOD.** Each building is a node at L1 (agents stepped
//!   individually) or L2 (agents as discrete-event schedules); agents move
//!   between the stores in validated, atomic transactions ([`lod`],
//!   [ADR 0017]).
//! - **State hash (Tier 2).** [`ToyWorld::state_hash`] covers canonical
//!   state only, in canonical order; it is stable within one build and
//!   platform.
//!
//! [ADR 0016]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0016-entity-identity-storage-and-randomness.md
//! [ADR 0017]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0017-simulation-lod-l1-l2-round-trip.md

pub mod dataset;
pub mod environment;
pub mod ids;
pub mod illness;
pub mod lod;
pub mod slice;
pub mod slice_render;

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use ws_ecs::{Column, DuplicateEntity, EntityId, join, join_mut};
use ws_math::{CellFrame, CoordError, Ecef, Position, SurfacePos};
use ws_rng::SipHasher24;
use ws_time::{SimDuration, SimTime};

pub use dataset::{BuildingSite, Dataset, TOY_BUILDINGS, TOY_DATASET_VERSION, ToyBuildingSpec};
pub use ids::{Population, ResidentAddress};
pub use illness::{Illness, IllnessKind, SEVERITY_MAX, SEVERITY_MIN};
pub use lod::{
    Activity, Background, ConservationSummary, Lod, LodError, LodViolation, Origin, Quantity,
    ScheduleProblem, TransitionReport,
};

/// A building: where it stands and its box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Building {
    /// Index in the dataset (the building's generation address).
    pub index: u32,
    /// Ground-level centre of the footprint; agents walk to this point.
    pub pos: SurfacePos,
    /// Half the side of the square footprint, in metres.
    pub half_extent_m: f32,
    /// Height of the box, in metres.
    pub height_m: f32,
}

/// A resident's identity component: where it was generated and its home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resident {
    /// Generation address, kept so the resident can be regenerated.
    pub address: ResidentAddress,
    /// The home building.
    pub home: EntityId,
}

/// A resident's movement state (L1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Walk {
    /// Velocity in metres per second, in the ENU frame of the agent's
    /// cell: during the last step, or towards the target after a
    /// promotion; zero while waiting. Derived state (ADR 0017 §10): no
    /// system reads it, and it is not part of the state hash.
    pub velocity: [f32; 3],
    /// The building the agent is walking to, or waiting at.
    pub target: EntityId,
    /// Whether the agent is at `target` (waiting) rather than walking.
    pub arrived: bool,
    /// Steps left to wait before drawing the next target.
    pub wait_steps: u32,
    /// How many targets the agent has drawn: the counter of its
    /// next-target stream.
    pub targets_drawn: u32,
}

/// Movement parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkParams {
    /// Walking speed in metres per second.
    pub speed_m_s: f64,
    /// Steps an agent waits at a building before leaving.
    pub wait_steps: u32,
}

impl WalkParams {
    /// 1.4 m/s, waiting 50 steps (5 s at the 100 ms base step).
    pub const DEFAULT: Self = Self {
        speed_m_s: 1.4,
        wait_steps: 50,
    };
}

impl Default for WalkParams {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a toy world could not be built or stepped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldError {
    /// Two entities got the same ID: a procedural ID collision, or the same
    /// entity inserted twice. Collisions are never resolved silently
    /// (ADR 0016 §1).
    Duplicate(EntityId),
    /// A resident refers to a building that does not exist.
    UnknownBuilding(EntityId),
    /// An illness refers to an entity that is not a resident.
    UnknownResident(EntityId),
    /// A coordinate operation failed.
    Coord(CoordError),
    /// A simulation-LOD transition, L2 event, or LOD invariant failed.
    Lod(LodError),
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => write!(f, "entity ID {id} is used twice"),
            Self::UnknownBuilding(id) => write!(f, "building {id} does not exist"),
            Self::UnknownResident(id) => write!(f, "resident {id} does not exist"),
            Self::Coord(e) => write!(f, "coordinate error: {e}"),
            Self::Lod(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WorldError {}

impl From<DuplicateEntity> for WorldError {
    fn from(e: DuplicateEntity) -> Self {
        Self::Duplicate(e.0)
    }
}

impl From<CoordError> for WorldError {
    fn from(e: CoordError) -> Self {
        Self::Coord(e)
    }
}

impl From<LodError> for WorldError {
    fn from(e: LodError) -> Self {
        Self::Lod(e)
    }
}

/// What a render record shows (ADR 0022). The numbers are the wire tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RenderKind {
    /// A box: `a` is the half extent and `b` the height, in metres.
    Building = 1,
    /// A capsule: `a` is the heading in radians from east towards north.
    Resident = 2,
}

/// One entity of the render snapshot, in metres from the snapshot origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderInstance {
    /// The entity.
    pub id: EntityId,
    /// What to draw.
    pub kind: RenderKind,
    /// Metres east of the snapshot origin.
    pub east: f32,
    /// Metres north of the snapshot origin.
    pub north: f32,
    /// Metres above the snapshot origin.
    pub up: f32,
    /// Half extent (building) or heading (resident).
    pub a: f32,
    /// Height (building) or zero (resident).
    pub b: f32,
}

/// The canonical state of a [`ToyWorld`] as plain rows, for persistence
/// (ADR 0020). Derived indices are not part of it: [`ToyWorld::from_parts`]
/// rebuilds them.
#[derive(Debug, Clone, PartialEq)]
pub struct ToyParts {
    /// The world seed.
    pub seed: u64,
    /// The dataset version the world was generated from.
    pub dataset_version: u64,
    /// Movement parameters.
    pub walk: WalkParams,
    /// Every building with its simulation level.
    pub buildings: Vec<(EntityId, Building, Lod)>,
    /// Identity of every resident.
    pub residents: Vec<(EntityId, Resident)>,
    /// L1 state: position and movement.
    pub l1: Vec<(EntityId, SurfacePos, Walk)>,
    /// L2 state: the schedule.
    pub l2: Vec<(EntityId, Background)>,
    /// Illnesses (ADR 0021), keyed by resident.
    pub illness: Vec<(EntityId, Illness)>,
}

/// The toy world: buildings and residents in `ws-ecs` columns.
///
/// Every column iterates in ascending [`EntityId`] order. Two worlds with
/// the same entities and components hash equally, however the entities
/// were inserted. Every resident has one identity entry and exactly one
/// state: L1 (`positions` and `walks`) or L2 (`background`), as its
/// governing building's level says ([`lod`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ToyWorld {
    seed: u64,
    dataset_version: u64,
    walk: WalkParams,
    buildings: Column<Building>,
    /// Each building's simulation level (canonical).
    node_lod: Column<Lod>,
    /// Identity of every resident, at every level.
    residents: Column<Resident>,
    /// L1 state.
    positions: Column<SurfacePos>,
    /// L1 state.
    walks: Column<Walk>,
    /// L2 state.
    background: Column<Background>,
    /// Illness of the sick residents, at either level (ADR 0021 §3).
    illness: Column<Illness>,
    /// Derived index: each home's residents, ascending. Residents are
    /// never removed, so inserts keep it sorted and it is rebuilt on load.
    by_home: BTreeMap<EntityId, Vec<EntityId>>,
    /// Derived index of `background`: one `(next event, ID)` per L2 agent.
    queue: BTreeSet<(SimTime, EntityId)>,
    /// Derived index: the residents whose store differs from their
    /// governing building's level, which the next tick's reconciliation
    /// moves. Rebuildable from the stores and the node levels, checked
    /// against that rebuild, never hashed. It lets a tick reconcile
    /// without scanning every agent (ADR 0017 §15).
    unreconciled: BTreeSet<EntityId>,
}

/// The planned result of one L1 movement step: each L1 agent's new walk
/// and position, in ascending ID order.
pub(crate) struct Movement(Vec<(EntityId, Walk, SurfacePos)>, Vec<ArrivalFact>);

/// A meaningful arrival produced by a domain plan, not inferred from snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrivalFact {
    /// Resident completing the leg.
    pub resident: EntityId,
    /// Destination building.
    pub destination: EntityId,
    /// Stable target counter identifying this leg.
    pub leg: u32,
    /// Scheduled L2 time, or None for the enclosing L1 tick end.
    pub time: Option<SimTime>,
    /// Producing simulation LOD.
    pub lod: Lod,
    /// Exact destination location.
    pub position: SurfacePos,
}

impl ToyWorld {
    /// An empty world; entities are added with [`ToyWorld::insert_building`]
    /// and [`ToyWorld::insert_resident`].
    #[must_use]
    pub const fn empty(seed: u64, dataset_version: u64, walk: WalkParams) -> Self {
        Self {
            seed,
            dataset_version,
            walk,
            buildings: Column::new(),
            node_lod: Column::new(),
            residents: Column::new(),
            positions: Column::new(),
            walks: Column::new(),
            background: Column::new(),
            illness: Column::new(),
            by_home: BTreeMap::new(),
            queue: BTreeSet::new(),
            unreconciled: BTreeSet::new(),
        }
    }

    /// Generates the world for `seed` from `dataset`: one building per
    /// site, and `ids::resident_count` residents per building, each waiting
    /// at home with no target drawn yet.
    ///
    /// # Errors
    ///
    /// [`WorldError::Duplicate`] if two procedural IDs collide; nothing is
    /// resolved or renumbered.
    pub fn generate(
        seed: u64,
        dataset: &Dataset,
        population: Population,
        walk: WalkParams,
    ) -> Result<Self, WorldError> {
        let mut world = Self::empty(seed, dataset.version, walk);
        let count = u32::try_from(dataset.sites.len()).expect("fewer than 2^32 buildings");
        for (index, site) in (0..count).zip(&dataset.sites) {
            let id = ids::building_id(seed, dataset.version, index);
            world.insert_building(
                id,
                Building {
                    index,
                    pos: site.pos,
                    half_extent_m: site.half_extent_m,
                    height_m: site.height_m,
                },
            )?;
        }
        for (address, id) in ids::residents(seed, dataset.version, count, population) {
            let home = ids::building_id(seed, dataset.version, address.building_index);
            world.insert_resident(id, Resident { address, home })?;
        }
        Ok(world)
    }

    /// The canonical state as plain rows, every list in ascending ID
    /// order.
    #[must_use]
    pub fn to_parts(&self) -> ToyParts {
        ToyParts {
            seed: self.seed,
            dataset_version: self.dataset_version,
            walk: self.walk,
            buildings: self
                .buildings
                .iter()
                .map(|(id, b)| {
                    (
                        id,
                        *b,
                        *self.node_lod.get(id).expect("every building has a level"),
                    )
                })
                .collect(),
            residents: self.residents.iter().map(|(id, r)| (id, *r)).collect(),
            l1: join(&self.positions, &self.walks)
                .map(|(id, p, w)| (id, *p, *w))
                .collect(),
            l2: self.background.iter().map(|(id, b)| (id, *b)).collect(),
            illness: self.illness.iter().map(|(id, i)| (id, *i)).collect(),
        }
    }

    /// Rebuilds a world from its canonical rows (loading a save). The
    /// event queue and the unreconciled-residency index are derived from
    /// the rows. Rows may arrive in any order.
    ///
    /// This checks only what building the columns needs: no ID appears
    /// twice within a table, and every building has exactly one level.
    /// Callers run [`ToyWorld::check_lod_invariants`] for the rest.
    ///
    /// # Errors
    ///
    /// [`WorldError::Duplicate`] for a repeated ID, and
    /// [`WorldError::UnknownBuilding`] if a resident's home is missing.
    pub fn from_parts(parts: ToyParts) -> Result<Self, WorldError> {
        let mut world = Self::empty(parts.seed, parts.dataset_version, parts.walk);
        let mut levels = Vec::with_capacity(parts.buildings.len());
        let mut buildings = Vec::with_capacity(parts.buildings.len());
        for (id, b, level) in parts.buildings {
            levels.push((id, level));
            buildings.push((id, b));
        }
        world.buildings.insert_batch(buildings)?;
        world.node_lod.insert_batch(levels)?;
        for (_, r) in &parts.residents {
            if !world.buildings.contains(r.home) {
                return Err(WorldError::UnknownBuilding(r.home));
            }
        }
        for (id, r) in &parts.residents {
            world.by_home.entry(r.home).or_default().push(*id);
        }
        for ids in world.by_home.values_mut() {
            ids.sort_unstable();
        }
        world.residents.insert_batch(parts.residents)?;
        let mut positions = Vec::with_capacity(parts.l1.len());
        let mut walks = Vec::with_capacity(parts.l1.len());
        for (id, p, w) in parts.l1 {
            positions.push((id, p));
            walks.push((id, w));
        }
        world.positions.insert_batch(positions)?;
        world.walks.insert_batch(walks)?;
        world.background.insert_batch(parts.l2)?;
        if let Some((id, _)) = parts
            .illness
            .iter()
            .find(|(id, _)| !world.residents.contains(*id))
        {
            return Err(WorldError::UnknownResident(*id));
        }
        world.illness.insert_batch(parts.illness)?;
        let queue: Vec<_> = world
            .background
            .iter()
            .map(|(id, bg)| (bg.next_event(), id))
            .collect();
        world.queue.extend(queue);
        let unreconciled: Vec<_> = world
            .walks
            .iter()
            .map(|(id, w)| (id, w.target, Lod::L1))
            .chain(
                world
                    .background
                    .iter()
                    .map(|(id, bg)| (id, bg.governing(), Lod::L2)),
            )
            .filter(|(_, b, residency)| world.node_lod.get(*b) != Some(residency))
            .map(|(id, _, _)| id)
            .collect();
        world.unreconciled.extend(unreconciled);
        Ok(world)
    }

    fn index_home(&mut self, id: EntityId, home: EntityId) {
        let ids = self.by_home.entry(home).or_default();
        if let Err(at) = ids.binary_search(&id) {
            ids.insert(at, id);
        }
    }

    /// The residents of `building`, ascending: O(1) lookup, no scan.
    #[must_use]
    pub fn residents_of(&self, building: EntityId) -> &[EntityId] {
        self.by_home.get(&building).map_or(&[], Vec::as_slice)
    }

    /// The illnesses (ADR 0021).
    #[must_use]
    pub const fn illness(&self) -> &Column<Illness> {
        &self.illness
    }

    /// The simulation level of the building that governs resident `id`
    /// (its home), if `id` is a resident.
    #[must_use]
    pub fn resident_level(&self, id: EntityId) -> Option<Lod> {
        let home = self.residents.get(id)?.home;
        self.node_lod.get(home).copied()
    }

    /// Makes residents ill. Callers have validated the batch: each ID is a
    /// resident and not yet ill (ADR 0021 §3).
    ///
    /// # Errors
    ///
    /// [`WorldError::UnknownResident`] or [`WorldError::Duplicate`]; the
    /// world is then unchanged.
    pub fn afflict(&mut self, batch: Vec<(EntityId, Illness)>) -> Result<(), WorldError> {
        if let Some((id, _)) = batch.iter().find(|(id, _)| !self.residents.contains(*id)) {
            return Err(WorldError::UnknownResident(*id));
        }
        self.illness.insert_batch(batch)?;
        Ok(())
    }

    /// If resident `id` is ill and `destination` is its home, the raw ID of
    /// its `life.illness` event: an arrival that is a return home.
    #[must_use]
    pub fn illness_root_if_home(&self, id: EntityId, destination: EntityId) -> Option<u64> {
        let ill = self.illness.get(id)?;
        (self.residents.get(id)?.home == destination).then_some(ill.root_event)
    }

    /// How many records [`ToyWorld::render_instances`] yields: every
    /// building plus every L1 resident.
    #[must_use]
    pub fn render_instance_count(&self) -> usize {
        self.buildings.len() + self.walks.len()
    }

    /// Exact ENU basis used by [`Self::render_instances`], independent of its
    /// translational origin. None only for a world with no buildings.
    #[must_use]
    pub fn render_basis(&self) -> Option<ws_math::EnuFrame> {
        self.buildings
            .iter()
            .next()
            .map(|(_, first)| *CellFrame::new(first.pos.cell()).frame())
    }

    /// Calls `f` with the render record of every building (ascending ID),
    /// then every L1 resident (ascending ID), in metres east, north and up
    /// of the ground centre of the lowest-ID building, in that building's
    /// cell frame (ADR 0022). Residents at L2 have no position and are
    /// skipped. Read-only.
    pub fn render_instances(&self, mut f: impl FnMut(RenderInstance)) {
        let Some((_, first)) = self.buildings.iter().next() else {
            return;
        };
        let frame = CellFrame::new(first.pos.cell());
        let origin = first.pos.offset_enu();
        let to_local = |p: &SurfacePos| -> [f32; 3] {
            let e = if p.cell() == first.pos.cell() {
                p.offset_enu()
            } else {
                frame.frame().to_enu(p.to_ecef())
            };
            [
                (e.x - origin.x) as f32,
                (e.y - origin.y) as f32,
                (e.z - origin.z) as f32,
            ]
        };
        for (id, b) in self.buildings.iter() {
            let [east, north, up] = to_local(&b.pos);
            f(RenderInstance {
                id,
                kind: RenderKind::Building,
                east,
                north,
                up,
                a: b.half_extent_m,
                b: b.height_m,
            });
        }
        for (id, pos, walk) in join(&self.positions, &self.walks) {
            let [east, north, up] = to_local(pos);
            let [vx, vy, _] = walk.velocity;
            let heading = if vx.abs() + vy.abs() > 1e-6 {
                vy.atan2(vx)
            } else {
                0.0
            };
            f(RenderInstance {
                id,
                kind: RenderKind::Resident,
                east,
                north,
                up,
                a: heading,
                b: 0.0,
            });
        }
    }

    /// The movement parameters.
    #[must_use]
    pub const fn walk_params(&self) -> WalkParams {
        self.walk
    }

    /// Adds a building at [`Lod::L1`].
    ///
    /// # Errors
    ///
    /// [`WorldError::Duplicate`] if `id` is already a building.
    pub fn insert_building(&mut self, id: EntityId, building: Building) -> Result<(), WorldError> {
        self.buildings.insert(id, building)?;
        self.node_lod
            .insert(id, Lod::L1)
            .expect("buildings and levels have the same IDs");
        Ok(())
    }

    /// Adds a resident waiting at its home building, with no target drawn.
    ///
    /// # Errors
    ///
    /// [`WorldError::UnknownBuilding`] if the home is not a building, and
    /// [`WorldError::Duplicate`] if `id` is already a resident.
    pub fn insert_resident(&mut self, id: EntityId, resident: Resident) -> Result<(), WorldError> {
        let home = self
            .buildings
            .get(resident.home)
            .ok_or(WorldError::UnknownBuilding(resident.home))?;
        let pos = home.pos;
        let walk = Walk {
            velocity: [0.0; 3],
            target: resident.home,
            arrived: true,
            wait_steps: 0,
            targets_drawn: 0,
        };
        self.insert_resident_state(id, resident, pos, walk)
    }

    /// Adds a resident at L1 with explicit position and movement state.
    /// An arrived agent must stand exactly at its target's centre
    /// (ADR 0017 §6). Residency is not reconciled until the next
    /// transaction or tick.
    ///
    /// # Errors
    ///
    /// [`WorldError::UnknownBuilding`] if the home or target is not a
    /// building, [`WorldError::Duplicate`] if `id` is already a resident,
    /// and [`WorldError::Lod`] with [`LodViolation::NotAtCentre`] if the
    /// agent has arrived but is not bit-exactly at its target's centre. The
    /// world is unchanged on error.
    pub fn insert_resident_state(
        &mut self,
        id: EntityId,
        resident: Resident,
        pos: SurfacePos,
        walk: Walk,
    ) -> Result<(), WorldError> {
        for b in [resident.home, walk.target] {
            if !self.buildings.contains(b) {
                return Err(WorldError::UnknownBuilding(b));
            }
        }
        if self.residents.contains(id) {
            return Err(WorldError::Duplicate(id));
        }
        let centre = self.buildings.get(walk.target).expect("checked above").pos;
        if walk.arrived && !lod::same_bits(pos, centre) {
            return Err(LodError::Invariant(LodViolation::NotAtCentre(id)).into());
        }
        self.residents.insert(id, resident)?;
        self.index_home(id, resident.home);
        self.positions.insert(id, pos)?;
        self.walks.insert(id, walk)?;
        self.note_residency(id, walk.target, Lod::L1);
        Ok(())
    }

    /// Adds a resident directly at L2. Residency is not reconciled, and the
    /// schedule is not checked against a time, until the next transaction
    /// or tick ([`ToyWorld::check_lod_invariants`] checks it).
    ///
    /// # Errors
    ///
    /// [`WorldError::UnknownBuilding`] if the home or a building in the
    /// schedule does not exist, and [`WorldError::Duplicate`] if `id` is
    /// already a resident. The world is unchanged on error.
    pub fn insert_resident_background(
        &mut self,
        id: EntityId,
        resident: Resident,
        background: Background,
    ) -> Result<(), WorldError> {
        let mut buildings = vec![resident.home, background.governing()];
        if let Activity::Walking {
            origin: Origin::Building(b),
            ..
        } = background.activity
        {
            buildings.push(b);
        }
        for b in buildings {
            if !self.buildings.contains(b) {
                return Err(WorldError::UnknownBuilding(b));
            }
        }
        if self.residents.contains(id) {
            return Err(WorldError::Duplicate(id));
        }
        self.residents.insert(id, resident)?;
        self.index_home(id, resident.home);
        self.background.insert(id, background)?;
        self.queue.insert((background.next_event(), id));
        self.note_residency(id, background.governing(), Lod::L2);
        Ok(())
    }

    /// Runs the movement system for one step of length `dt`. Tier 2.
    ///
    /// Each resident, in ascending ID order:
    /// - waiting with steps left: counts one down;
    /// - waiting with none left: draws its next target (draw number
    ///   `targets_drawn`) and starts walking;
    /// - walking: moves `speed × dt` straight towards the target's centre in
    ///   its cell's ENU frame, arriving (snapped to the centre, waiting
    ///   `wait_steps`) when that is within one step; then its position is
    ///   re-expressed into the cell containing it.
    ///
    /// The step is atomic: on error the world is unchanged.
    ///
    /// # Errors
    ///
    /// [`WorldError::Coord`] from re-expression (never expected), and
    /// [`WorldError::UnknownBuilding`] if a target is missing.
    pub fn step(&mut self, dt: SimDuration) -> Result<(), WorldError> {
        let movement = self.plan_movement(dt)?;
        self.commit_movement(movement);
        Ok(())
    }

    /// Computes one movement step for every L1 agent without changing the
    /// world.
    pub(crate) fn plan_movement(&self, dt: SimDuration) -> Result<Movement, WorldError> {
        if self.walks.is_empty() {
            return Ok(Movement(Vec::new(), Vec::new()));
        }
        // Microseconds to seconds; Tier 2, so floating point is allowed.
        let dt_s = dt.as_micros() as f64 * 1e-6;
        let step_m = self.walk.speed_m_s * dt_s;
        let building_ids = self.buildings.ids().to_vec();
        // Target centres in ECEF, once per step (index-aligned with the IDs).
        let centres: Vec<Position<Ecef>> = self
            .buildings
            .iter()
            .map(|(_, b)| b.pos.to_ecef())
            .collect();
        // Derived, step-local cache: one exact frame per occupied cell,
        // shared by goal projection and re-expression. No canonical state
        // or iteration order depends on the cache; L2 never builds it.
        let mut frames = BTreeMap::new();
        let mut updates = Vec::with_capacity(self.walks.len());
        let mut arrivals = Vec::new();
        for (id, walk, pos) in join(&self.walks, &self.positions) {
            let mut walk = *walk;
            let mut pos = *pos;
            if walk.arrived {
                if walk.wait_steps > 0 {
                    walk.wait_steps -= 1;
                    updates.push((id, walk, pos));
                    continue;
                }
                if self.illness.contains(id) {
                    // ADR 0021 §4: an ill agent draws no target. Away from
                    // home it walks home; at home it keeps waiting.
                    let home = self
                        .residents
                        .get(id)
                        .ok_or(LodError::UnknownNode(id))?
                        .home;
                    if walk.target == home {
                        walk.wait_steps = self.walk.wait_steps;
                        updates.push((id, walk, pos));
                        continue;
                    }
                    walk.target = home;
                } else {
                    walk.target = ids::next_target(
                        self.seed,
                        id,
                        walk.targets_drawn,
                        &building_ids,
                        walk.target,
                    );
                    walk.targets_drawn = walk
                        .targets_drawn
                        .checked_add(1)
                        .ok_or(LodError::CounterExhausted(id))?;
                }
                walk.arrived = false;
            }
            let t = building_ids
                .binary_search(&walk.target)
                .map_err(|_| WorldError::UnknownBuilding(walk.target))?;
            let frame = frames
                .entry(pos.cell())
                .or_insert_with(|| CellFrame::with_inside_test(pos.cell()));
            let goal = frame.frame().to_enu(centres[t]);
            let here = pos.offset_enu();
            let to_goal = goal - here;
            let dist = to_goal.length();
            if dist <= step_m {
                pos = self.buildings.get(walk.target).expect("found above").pos;
                arrivals.push(ArrivalFact {
                    resident: id,
                    destination: walk.target,
                    leg: walk.targets_drawn,
                    time: None,
                    lod: Lod::L1,
                    position: pos,
                });
                walk.arrived = true;
                walk.wait_steps = self.walk.wait_steps;
                walk.velocity = [0.0; 3];
            } else {
                let dir = to_goal * (1.0 / dist);
                let next = here + dir * step_m;
                let v = dir * self.walk.speed_m_s;
                walk.velocity = [v.x as f32, v.y as f32, v.z as f32];
                pos = SurfacePos::new(pos.cell(), [next.x as f32, next.y as f32, next.z as f32])?
                    .reexpress_with(frame)?;
            }
            updates.push((id, walk, pos));
        }
        Ok(Movement(updates, arrivals))
    }

    /// Writes a planned movement step.
    pub(crate) fn commit_movement(&mut self, movement: Movement) {
        let mut departed = Vec::new();
        let mut updates = movement.0.into_iter();
        join_mut(&mut self.walks, &mut self.positions, |id, walk, pos| {
            let (uid, w, p) = updates.next().expect("one update per resident");
            debug_assert_eq!(uid, id);
            if w.targets_drawn != walk.targets_drawn {
                departed.push((id, w.target));
            }
            *walk = w;
            *pos = p;
        });
        for (id, target) in departed {
            self.note_residency(id, target, Lod::L1);
        }
    }

    /// Hash of the canonical state, in canonical order. Tier 2: stable
    /// within one build and platform.
    ///
    /// Covers the seed, the dataset version, the walk parameters, and every
    /// canonical component of every entity: IDs, integer fields, cell IDs,
    /// times, and float fields by their bits, column by column in ascending
    /// ID order, each column prefixed by a tag and its length. That
    /// includes building levels, which store holds each agent (its L1 or
    /// L2 columns), and every field of an L2 schedule (ADR 0017 §11).
    /// Nothing else: no storage indices, capacities, addresses, the L2
    /// event queue, or derived values such as `Walk::velocity`.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        let mut h = SipHasher24::new(0, ids::PURPOSE_STATE_HASH.value());
        h.write_words(&[
            self.seed,
            self.dataset_version,
            self.walk.speed_m_s.to_bits(),
            u64::from(self.walk.wait_steps),
        ]);
        h.write_words(&[1, self.buildings.len() as u64]);
        for (id, b) in self.buildings.iter() {
            h.write_words(&[id.raw(), u64::from(b.index)]);
            write_pos(&mut h, b.pos);
            h.write_words(&[
                u64::from(b.half_extent_m.to_bits()),
                u64::from(b.height_m.to_bits()),
            ]);
        }
        h.write_words(&[2, self.residents.len() as u64]);
        for (id, r) in self.residents.iter() {
            h.write_words(&[
                id.raw(),
                u64::from(r.address.building_index),
                u64::from(r.address.slot),
                r.home.raw(),
            ]);
        }
        h.write_words(&[3, self.positions.len() as u64]);
        for (id, p) in self.positions.iter() {
            h.write_u64(id.raw());
            write_pos(&mut h, *p);
        }
        h.write_words(&[4, self.walks.len() as u64]);
        for (id, w) in self.walks.iter() {
            h.write_u64(id.raw());
            h.write_words(&[
                w.target.raw(),
                u64::from(w.arrived),
                u64::from(w.wait_steps),
                u64::from(w.targets_drawn),
            ]);
        }
        h.write_words(&[5, self.node_lod.len() as u64]);
        for (id, level) in self.node_lod.iter() {
            h.write_words(&[id.raw(), u64::from(level.as_u8())]);
        }
        h.write_words(&[6, self.background.len() as u64]);
        for (id, bg) in self.background.iter() {
            h.write_words(&[id.raw(), u64::from(bg.targets_drawn)]);
            match bg.activity {
                Activity::Waiting { at, depart } => {
                    h.write_words(&[0, at.raw(), time_word(depart)]);
                }
                Activity::Walking {
                    origin,
                    origin_time,
                    dest,
                    arrive,
                } => {
                    h.write_words(&[1, dest.raw(), time_word(origin_time), time_word(arrive)]);
                    match origin {
                        Origin::Building(b) => h.write_words(&[0, b.raw()]),
                        Origin::Point { cell, offset } => {
                            h.write_words(&[1, cell.raw()]);
                            for c in offset {
                                h.write_u64(u64::from(c.to_bits()));
                            }
                        }
                    }
                }
            }
        }
        if !self.illness.is_empty() {
            // Appended only when someone is ill, so worlds without
            // illness hash exactly as before (format-1 fixtures).
            h.write_words(&[7, self.illness.len() as u64]);
            for (id, i) in self.illness.iter() {
                h.write_words(&[
                    id.raw(),
                    u64::from(i.kind.tag()),
                    u64::from(i.severity),
                    i.root_event,
                ]);
            }
        }
        h.finish()
    }

    /// The world seed.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// The dataset version the world was generated from.
    #[must_use]
    pub const fn dataset_version(&self) -> u64 {
        self.dataset_version
    }

    /// Buildings, by ID.
    #[must_use]
    pub const fn buildings(&self) -> &Column<Building> {
        &self.buildings
    }

    /// Residents' identity components, by ID.
    #[must_use]
    pub const fn residents(&self) -> &Column<Resident> {
        &self.residents
    }

    /// Residents' positions, by ID.
    #[must_use]
    pub const fn positions(&self) -> &Column<SurfacePos> {
        &self.positions
    }

    /// Residents' movement state, by ID.
    #[must_use]
    pub const fn walks(&self) -> &Column<Walk> {
        &self.walks
    }

    /// The ECEF position of a resident, if it exists.
    #[must_use]
    pub fn resident_ecef(&self, id: EntityId) -> Option<Position<Ecef>> {
        self.positions.get(id).map(SurfacePos::to_ecef)
    }
}

/// A time as a hash word: its `i64` microseconds, two's complement.
const fn time_word(t: SimTime) -> u64 {
    t.as_micros_since_epoch() as u64
}

fn write_pos(h: &mut SipHasher24, p: SurfacePos) {
    h.write_u64(p.cell().raw());
    for c in p.offset() {
        h.write_u64(u64::from(c.to_bits()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L1 memory per agent: one entry in each of the three resident
    /// columns (component plus its ID copy). 09 §10.8 targets 0.5–1 KB per
    /// L1 agent; TP-006 agents carry far less state than that budget
    /// assumes (no needs, plans, or perception yet).
    #[test]
    fn l1_agent_memory_per_agent() {
        let id = size_of::<EntityId>();
        let per_agent =
            size_of::<Resident>() + size_of::<SurfacePos>() + size_of::<Walk>() + 3 * id;
        assert_eq!(size_of::<Resident>(), 16);
        assert_eq!(size_of::<SurfacePos>(), 24);
        assert_eq!(size_of::<Walk>(), 32);
        assert_eq!(per_agent, 96);
    }
}
