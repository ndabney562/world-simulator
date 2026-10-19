//! VS-003 immutable synthetic population. All decisions are integer Tier 1.
//! No schedules, mutable behavior, real-person data, I/O or render state.
use super::{BLOCK_BUILDINGS, Baseline, DATASET_VERSION, GeoPoint, SliceError, SliceWorld};
use crate::ids::{PURPOSE_RESIDENT_ID, ResidentAddress};
use core::{fmt, num::NonZeroU64};
use std::collections::BTreeSet;
use ws_ecs::{Column, EntityId};
use ws_rng::{Purpose, Stream, below, derive};

/// Version of the synthetic population policy; structural loader stays version 1.
pub const GENERATOR_VERSION: u32 = 1;
const HOUSEHOLD_ID: Purpose = Purpose::new("ws-world/id/household");
const WORKPLACE_ID: Purpose = Purpose::new("ws-world/id/workplace");
const SIZE: Purpose = Purpose::new("ws-world/slice/household-size");
const AGE: Purpose = Purpose::new("ws-world/slice/age");
const GIVEN: Purpose = Purpose::new("ws-world/slice/given-name");
const FAMILY: Purpose = Purpose::new("ws-world/slice/family-name");
const GIVEN_NAMES: [&str; 16] = [
    "Alex", "Blair", "Casey", "Drew", "Eden", "Finley", "Gray", "Harper", "Jamie", "Kai", "Morgan",
    "Quinn", "Reese", "Rowan", "Sam", "Taylor",
];
const FAMILY_NAMES: [&str; 16] = [
    "Adams", "Baker", "Chen", "Diaz", "Evans", "Flores", "Green", "Hall", "Ibrahim", "Jones",
    "Kim", "Lee", "Patel", "Rivera", "Singh", "Wong",
];

/// Both independent generator versions needed to reproduce this baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopulationBaseline {
    /// Existing seed, extract version and structural-loader version.
    pub structural: Baseline,
    /// Version of the population rules and name tables.
    pub generator_version: u32,
}
/// An occupied synthetic home, keyed by building BIN and unit slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Household {
    /// Stable real building ID; every member lives here.
    pub home: EntityId,
    /// Source key, not the building's position in a collection.
    pub bin: u32,
    /// Zero-based home within the building's PLUTO residential-unit count.
    pub unit: u32,
    /// Member IDs in generation-slot order; the first member is an adult.
    pub members: Vec<EntityId>,
}
/// Synthetic age-derived role, without a behavioral schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    /// Age 0–4; no school/work assignment.
    Preschool = 0,
    /// Age 5–21; education assigned outside the block.
    Student = 1,
    /// Age 22–64; on-block or off-map job assigned.
    Employed = 2,
    /// Age 65–90; no job/school assignment.
    Retired = 3,
}
impl Role {
    fn for_age(age: u8) -> Self {
        match age {
            0..=4 => Self::Preschool,
            5..=21 => Self::Student,
            22..=64 => Self::Employed,
            _ => Self::Retired,
        }
    }
}
/// Immutable synthetic identity, explicitly unrelated to real residents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticResident {
    /// ADR 0016 resident address: BIN as building key, unit*3 + member slot.
    pub address: ResidentAddress,
    /// Household identity.
    pub household: EntityId,
    /// Home building identity.
    pub home: EntityId,
    /// Age in whole years at baseline creation.
    pub age: u8,
    /// Hand-authored synthetic name-table choice, not sourced from real people.
    pub given_name: &'static str,
    /// All members of this synthetic household share this name-table choice.
    pub family_name: &'static str,
    /// Age-derived baseline role.
    pub role: Role,
    /// Job/school identity for employed residents/students; None otherwise.
    pub workplace: Option<EntityId>,
}
/// Coarse destination purpose; schedules arrive in VS-004.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkplaceKind {
    /// Employment location.
    Employment = 0,
    /// Education location.
    Education = 1,
}
/// A destination is either a live building or a synthetic map-edge gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkplaceLocation {
    /// Building in the selected block.
    Building(EntityId),
    /// Index into the population's two exit points.
    Exit(u8),
}
/// A synthetic workplace/school, never a record of an actual business/person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workplace {
    /// Job or education.
    pub kind: WorkplaceKind,
    /// On-block building or off-map edge.
    pub location: WorkplaceLocation,
    /// Synthetic capacity; off-map destinations accommodate the whole slice.
    pub capacity: u32,
}
/// Failure produces no partially populated world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopulationError {
    /// Structural baseline could not be built.
    Structural(SliceError),
    /// Source class/unit count no longer matches the reviewed slice policy.
    InvalidHomes(u32),
    /// A complete reviewed four-building/ten-home mix is required.
    UnsupportedBlock,
    /// Any cross-kind procedural collision is rejected under the current policy.
    Collision(EntityId),
}
impl fmt::Display for PopulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "slice population error: {self:?}")
    }
}
impl std::error::Error for PopulationError {}

/// Complete VS-003 baseline: four buildings, ten households, 10–30 people.
/// Existing SliceWorld and toy constructors retain their original contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulatedSlice {
    world: SliceWorld,
    households: Column<Household>,
    residents: Column<SyntheticResident>,
    workplaces: Column<Workplace>,
    exits: [GeoPoint; 2],
}
impl PopulatedSlice {
    /// Generates the complete block without ambient inputs or mutable RNG state.
    /// # Errors
    /// Invalid source/home mix or any detected collision, across all entity kinds.
    pub fn new(seed: u64) -> Result<Self, PopulationError> {
        let world = SliceWorld::new(seed).map_err(PopulationError::Structural)?;
        Self::generate(world, |purpose, words| {
            EntityId::procedural(derive(seed, purpose, words))
        })
    }
    fn generate(
        world: SliceWorld,
        id_for: impl Fn(Purpose, &[u64]) -> EntityId,
    ) -> Result<Self, PopulationError> {
        let seed = world.baseline().seed;
        let mut occupied: BTreeSet<_> = world.buildings().ids().iter().copied().collect();
        let mut allocate = |purpose, words: &[u64]| {
            let id = id_for(purpose, words);
            if occupied.insert(id) {
                Ok(id)
            } else {
                Err(PopulationError::Collision(id))
            }
        };
        let mut household_rows = Vec::new();
        let mut resident_rows = Vec::new();
        let mut shop = None;
        for (home, building) in world.buildings().iter() {
            let expected = match building.building_class {
                "S1" | "A5" => 1,
                "B2" => 2,
                "C2" => 6,
                _ => return Err(PopulationError::InvalidHomes(building.bin)),
            };
            if building.residential_units != expected || building.floors_milli == 0 {
                return Err(PopulationError::InvalidHomes(building.bin));
            }
            if building.building_class == "S1" {
                if shop.is_some() {
                    return Err(PopulationError::UnsupportedBlock);
                }
                shop = Some((
                    home,
                    building.bin,
                    (building.floors_milli / 1000).clamp(1, 3),
                ));
            }
            for unit in 0..building.residential_units {
                let hh = allocate(
                    HOUSEHOLD_ID,
                    &[DATASET_VERSION, u64::from(building.bin), u64::from(unit)],
                )?;
                let size = 1 + draw(seed, SIZE, hh, 3);
                let family_name = FAMILY_NAMES[draw(seed, FAMILY, hh, 16) as usize];
                let mut members = Vec::new();
                for member in 0..size {
                    let address = ResidentAddress {
                        building_index: building.bin,
                        slot: unit * 3 + member,
                    };
                    let id = allocate(
                        PURPOSE_RESIDENT_ID,
                        &[
                            DATASET_VERSION,
                            u64::from(address.building_index),
                            u64::from(address.slot),
                        ],
                    )?;
                    let age = if member == 0 {
                        22 + draw(seed, AGE, id, 59)
                    } else {
                        draw(seed, AGE, id, 91)
                    } as u8;
                    members.push(id);
                    resident_rows.push((
                        id,
                        SyntheticResident {
                            address,
                            household: hh,
                            home,
                            age,
                            given_name: GIVEN_NAMES[draw(seed, GIVEN, id, 16) as usize],
                            family_name,
                            role: Role::for_age(age),
                            workplace: None,
                        },
                    ));
                }
                household_rows.push((
                    hh,
                    Household {
                        home,
                        bin: building.bin,
                        unit,
                        members,
                    },
                ));
            }
        }
        if world.buildings().len() != 4 || household_rows.len() != 10 {
            return Err(PopulationError::UnsupportedBlock);
        }
        let (shop_home, bin, capacity) = shop.ok_or(PopulationError::UnsupportedBlock)?;
        let local = allocate(WORKPLACE_ID, &[DATASET_VERSION, u64::from(bin), 0])?;
        let remote_job = allocate(WORKPLACE_ID, &[DATASET_VERSION, 0, 0])?;
        let remote_school = allocate(WORKPLACE_ID, &[DATASET_VERSION, 0, 1])?;
        // Resolve limited shop capacity in canonical resident-ID order, never
        // input/worker order. All other workers use the west gateway.
        resident_rows.sort_unstable_by_key(|(id, _)| *id);
        let mut local_jobs = 0;
        for (_, resident) in &mut resident_rows {
            resident.workplace = match resident.role {
                Role::Employed if local_jobs < capacity => {
                    local_jobs += 1;
                    Some(local)
                }
                Role::Employed => Some(remote_job),
                Role::Student => Some(remote_school),
                _ => None,
            };
        }
        let mut households = Column::new();
        let mut residents = Column::new();
        let mut workplaces = Column::new();
        households
            .insert_batch(household_rows)
            .map_err(|e| PopulationError::Collision(e.0))?;
        residents
            .insert_batch(resident_rows)
            .map_err(|e| PopulationError::Collision(e.0))?;
        workplaces
            .insert_batch(vec![
                (
                    local,
                    Workplace {
                        kind: WorkplaceKind::Employment,
                        location: WorkplaceLocation::Building(shop_home),
                        capacity,
                    },
                ),
                (
                    remote_job,
                    Workplace {
                        kind: WorkplaceKind::Employment,
                        location: WorkplaceLocation::Exit(0),
                        capacity: 30,
                    },
                ),
                (
                    remote_school,
                    Workplace {
                        kind: WorkplaceKind::Education,
                        location: WorkplaceLocation::Exit(1),
                        capacity: 30,
                    },
                ),
            ])
            .map_err(|e| PopulationError::Collision(e.0))?;
        // Synthetic gateways on the pinned footprint extent, not surveyed
        // sidewalks or terrain. Geometry is presentation-independent.
        let points: Vec<_> = BLOCK_BUILDINGS
            .iter()
            .flat_map(|b| b.footprint.polygons)
            .flat_map(|p| p.rings)
            .flat_map(|r| r.points)
            .collect();
        let west = points
            .iter()
            .map(|p| p.lon_pdeg)
            .min()
            .expect("validated extract");
        let east = points
            .iter()
            .map(|p| p.lon_pdeg)
            .max()
            .expect("validated extract");
        let south = points
            .iter()
            .map(|p| p.lat_pdeg)
            .min()
            .expect("validated extract");
        let north = points
            .iter()
            .map(|p| p.lat_pdeg)
            .max()
            .expect("validated extract");
        let lat_pdeg = (south + north) / 2;
        Ok(Self {
            world,
            households,
            residents,
            workplaces,
            exits: [
                GeoPoint {
                    lon_pdeg: west,
                    lat_pdeg,
                },
                GeoPoint {
                    lon_pdeg: east,
                    lat_pdeg,
                },
            ],
        })
    }
    /// Source buildings and structural baseline.
    pub const fn world(&self) -> &SliceWorld {
        &self.world
    }
    /// Independently versioned structure and population generation inputs.
    pub const fn baseline(&self) -> PopulationBaseline {
        PopulationBaseline {
            structural: self.world.baseline(),
            generator_version: GENERATOR_VERSION,
        }
    }
    /// Households in ascending stable-ID order.
    pub const fn households(&self) -> &Column<Household> {
        &self.households
    }
    /// Synthetic residents in ascending stable-ID order.
    pub const fn residents(&self) -> &Column<SyntheticResident> {
        &self.residents
    }
    /// Workplaces/school in ascending stable-ID order.
    pub const fn workplaces(&self) -> &Column<Workplace> {
        &self.workplaces
    }
    /// West employment and east school gateways on the footprint extent.
    pub const fn exits(&self) -> &[GeoPoint; 2] {
        &self.exits
    }
}
fn draw(seed: u64, purpose: Purpose, id: EntityId, count: u64) -> u32 {
    below(
        Stream::new(seed, purpose, &[id.raw()]).draw(0),
        NonZeroU64::new(count).expect("nonzero policy range"),
    ) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn purposes_are_distinct_from_all_existing_world_purposes() {
        let mut all = crate::ids::PURPOSES.to_vec();
        all.extend([HOUSEHOLD_ID, WORKPLACE_ID, SIZE, AGE, GIVEN, FAMILY]);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.value(), b.value());
            }
        }
    }
    #[test]
    fn cross_kind_collisions_fail_without_renumbering_or_partial_population() {
        let world = SliceWorld::new(42).unwrap();
        let building_id = world.buildings().ids()[0];
        assert_eq!(
            PopulatedSlice::generate(world, |_, _| building_id),
            Err(PopulationError::Collision(building_id))
        );
        let id = EntityId::procedural(123);
        assert_eq!(
            PopulatedSlice::generate(SliceWorld::new(42).unwrap(), |_, _| id),
            Err(PopulationError::Collision(id))
        );
    }
    #[test]
    fn population_does_not_depend_on_extract_order() {
        let mut source = BLOCK_BUILDINGS.to_vec();
        source.reverse();
        let world = SliceWorld::load(42, &source, |bin| {
            crate::ids::building_id(42, DATASET_VERSION, bin)
        })
        .unwrap();
        let result =
            PopulatedSlice::generate(world, |p, w| EntityId::procedural(derive(42, p, w))).unwrap();
        assert_eq!(result, PopulatedSlice::new(42).unwrap());
    }
    #[test]
    fn changed_source_home_counts_are_not_silently_accepted() {
        let mut world = SliceWorld::new(42).unwrap();
        let id = world.buildings().ids()[0];
        let bin = world.buildings.get(id).unwrap().bin;
        world.buildings.get_mut(id).unwrap().residential_units = 999;
        assert_eq!(
            PopulatedSlice::generate(world, |p, w| EntityId::procedural(derive(42, p, w))),
            Err(PopulationError::InvalidHomes(bin))
        );
    }
}
