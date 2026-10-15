//! The Technical Prototype toy world (TP-006, #7; [ADR 0016]).
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
//! - **State hash (Tier 2).** [`ToyWorld::state_hash`] covers canonical
//!   state only, in canonical order; it is stable within one build and
//!   platform.
//!
//! [ADR 0016]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0016-entity-identity-storage-and-randomness.md

pub mod dataset;
pub mod ids;

use core::fmt;

use ws_ecs::{Column, DuplicateEntity, EntityId, join, join_mut};
use ws_math::{CellFrame, CoordError, Ecef, Position, SurfacePos};
use ws_rng::SipHasher24;
use ws_time::SimDuration;

pub use dataset::{BuildingSite, Dataset, TOY_BUILDINGS, TOY_DATASET_VERSION, ToyBuildingSpec};
pub use ids::{Population, ResidentAddress};

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
    /// Velocity in metres per second during the last step, in the ENU
    /// frame of the cell the agent started that step in; zero while
    /// waiting.
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
    /// A coordinate operation failed.
    Coord(CoordError),
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => write!(f, "entity ID {id} is used twice"),
            Self::UnknownBuilding(id) => write!(f, "building {id} does not exist"),
            Self::Coord(e) => write!(f, "coordinate error: {e}"),
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

/// The toy world: buildings and residents in `ws-ecs` columns.
///
/// Every column iterates in ascending [`EntityId`] order. Two worlds with
/// the same entities and components are equal, and hash equally, however
/// the entities were inserted.
#[derive(Debug, Clone, PartialEq)]
pub struct ToyWorld {
    seed: u64,
    dataset_version: u64,
    walk: WalkParams,
    buildings: Column<Building>,
    residents: Column<Resident>,
    positions: Column<SurfacePos>,
    walks: Column<Walk>,
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
            residents: Column::new(),
            positions: Column::new(),
            walks: Column::new(),
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

    /// Adds a building.
    ///
    /// # Errors
    ///
    /// [`WorldError::Duplicate`] if `id` is already a building.
    pub fn insert_building(&mut self, id: EntityId, building: Building) -> Result<(), WorldError> {
        Ok(self.buildings.insert(id, building)?)
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

    /// Adds a resident with explicit position and movement state.
    ///
    /// # Errors
    ///
    /// [`WorldError::UnknownBuilding`] if the home or target is not a
    /// building, and [`WorldError::Duplicate`] if `id` is already a
    /// resident. The world is unchanged on error.
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
        self.residents.insert(id, resident)?;
        self.positions.insert(id, pos)?;
        self.walks.insert(id, walk)?;
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
        if self.walks.is_empty() {
            return Ok(());
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
        let mut updates = Vec::with_capacity(self.walks.len());
        for (id, walk, pos) in join(&self.walks, &self.positions) {
            let mut walk = *walk;
            let mut pos = *pos;
            if walk.arrived {
                if walk.wait_steps > 0 {
                    walk.wait_steps -= 1;
                    updates.push((id, walk, pos));
                    continue;
                }
                walk.target = ids::next_target(
                    self.seed,
                    id,
                    walk.targets_drawn,
                    &building_ids,
                    walk.target,
                );
                walk.targets_drawn += 1;
                walk.arrived = false;
            }
            let t = building_ids
                .binary_search(&walk.target)
                .map_err(|_| WorldError::UnknownBuilding(walk.target))?;
            let frame = CellFrame::new(pos.cell());
            let goal = frame.frame().to_enu(centres[t]);
            let here = pos.offset_enu();
            let to_goal = goal - here;
            let dist = to_goal.length();
            if dist <= step_m {
                pos = self.buildings.get(walk.target).expect("found above").pos;
                walk.arrived = true;
                walk.wait_steps = self.walk.wait_steps;
                walk.velocity = [0.0; 3];
            } else {
                let dir = to_goal * (1.0 / dist);
                let next = here + dir * step_m;
                let v = dir * self.walk.speed_m_s;
                walk.velocity = [v.x as f32, v.y as f32, v.z as f32];
                pos = SurfacePos::new(pos.cell(), [next.x as f32, next.y as f32, next.z as f32])?
                    .reexpress()?;
            }
            updates.push((id, walk, pos));
        }
        let mut updates = updates.into_iter();
        join_mut(&mut self.walks, &mut self.positions, |id, walk, pos| {
            let (uid, w, p) = updates.next().expect("one update per resident");
            debug_assert_eq!(uid, id);
            *walk = w;
            *pos = p;
        });
        Ok(())
    }

    /// Hash of the canonical state, in canonical order. Tier 2: stable
    /// within one build and platform.
    ///
    /// Covers the seed, the dataset version, the walk parameters, and every
    /// component of every entity: IDs, integer fields, cell IDs, and float
    /// fields by their bits, column by column in ascending ID order, each
    /// column prefixed by a tag and its length. Nothing else: no storage
    /// indices, capacities, addresses, or derived values.
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
            for v in w.velocity {
                h.write_u64(u64::from(v.to_bits()));
            }
            h.write_words(&[
                w.target.raw(),
                u64::from(w.arrived),
                u64::from(w.wait_steps),
                u64::from(w.targets_drawn),
            ]);
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
