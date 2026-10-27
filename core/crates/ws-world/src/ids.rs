//! Procedural identities and generator decisions (ADR 0016 §1, §3). All
//! Tier 1: integer-only functions of the world seed, the dataset version,
//! and a generation address.

use core::num::NonZeroU64;

use ws_ecs::EntityId;
use ws_rng::{Purpose, Stream, below, derive};

/// Purpose of building IDs; address `[dataset_version, building_index]`.
pub const PURPOSE_BUILDING_ID: Purpose = Purpose::new("ws-world/id/building");
/// Purpose of resident IDs; address `[dataset_version, building_index, slot]`.
pub const PURPOSE_RESIDENT_ID: Purpose = Purpose::new("ws-world/id/resident");
/// Purpose of the residents-per-building decision; address
/// `[dataset_version, building_index]`.
pub const PURPOSE_RESIDENT_COUNT: Purpose = Purpose::new("ws-world/gen/resident-count");
/// Purpose of each agent's next-target stream; address `[entity_id]`.
pub const PURPOSE_NEXT_TARGET: Purpose = Purpose::new("ws-world/agent/next-target");
/// Key of the canonical state hash (with key half 0).
pub const PURPOSE_STATE_HASH: Purpose = Purpose::new("ws-world/state-hash");
/// Key of the canonical baseline structural digest (ADR 0025 §1).
pub const PURPOSE_STRUCTURAL_DIGEST: Purpose = Purpose::new("ws-world/structural-digest");

/// Every purpose this crate uses; a test checks they are distinct.
pub const PURPOSES: [Purpose; 6] = [
    PURPOSE_BUILDING_ID,
    PURPOSE_RESIDENT_ID,
    PURPOSE_RESIDENT_COUNT,
    PURPOSE_NEXT_TARGET,
    PURPOSE_STATE_HASH,
    PURPOSE_STRUCTURAL_DIGEST,
];

/// How many residents each building gets: a uniform integer in
/// `min_per_building..=max_per_building`, decided per building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Population {
    /// Fewest residents per building.
    pub min_per_building: u32,
    /// Most residents per building (at least `min_per_building`).
    pub max_per_building: u32,
}

impl Population {
    /// The toy world's default: 4 to 12 residents per building, so the five
    /// toy buildings hold a few dozen agents.
    pub const TOY: Self = Self {
        min_per_building: 4,
        max_per_building: 12,
    };

    /// Exactly `n` residents per building (benchmarks).
    #[must_use]
    pub const fn exactly(n: u32) -> Self {
        Self {
            min_per_building: n,
            max_per_building: n,
        }
    }
}

impl Default for Population {
    fn default() -> Self {
        Self::TOY
    }
}

/// Where a resident was generated: its home building and slot. The address
/// is stored with the entity because a hashed ID cannot be inverted
/// (ADR 0016 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResidentAddress {
    /// Index of the home building in the dataset.
    pub building_index: u32,
    /// Slot within the building, from 0.
    pub slot: u32,
}

/// The procedural ID of building `index`.
#[must_use]
pub fn building_id(seed: u64, dataset_version: u64, index: u32) -> EntityId {
    EntityId::procedural(derive(
        seed,
        PURPOSE_BUILDING_ID,
        &[dataset_version, u64::from(index)],
    ))
}

/// The procedural ID of the resident at `address`.
#[must_use]
pub fn resident_id(seed: u64, dataset_version: u64, address: ResidentAddress) -> EntityId {
    EntityId::procedural(derive(
        seed,
        PURPOSE_RESIDENT_ID,
        &[
            dataset_version,
            u64::from(address.building_index),
            u64::from(address.slot),
        ],
    ))
}

/// How many residents building `index` gets.
///
/// # Panics
///
/// If `population.max_per_building < population.min_per_building`.
#[must_use]
pub fn resident_count(seed: u64, dataset_version: u64, index: u32, population: Population) -> u32 {
    let span = population
        .max_per_building
        .checked_sub(population.min_per_building)
        .expect("max_per_building >= min_per_building");
    let span = NonZeroU64::new(u64::from(span) + 1).expect("span + 1 > 0");
    let x = derive(
        seed,
        PURPOSE_RESIDENT_COUNT,
        &[dataset_version, u64::from(index)],
    );
    // below() < span <= u32::MAX + 1, so the sum fits after the check above.
    population.min_per_building + u32::try_from(below(x, span)).expect("below(x, n) < n")
}

/// Every resident of a dataset with `buildings` buildings: its address and
/// ID, in address order. The whole Tier 1 output of the toy generator
/// besides building IDs.
#[must_use]
pub fn residents(
    seed: u64,
    dataset_version: u64,
    buildings: u32,
    population: Population,
) -> Vec<(ResidentAddress, EntityId)> {
    (0..buildings)
        .flat_map(|building_index| {
            let count = resident_count(seed, dataset_version, building_index, population);
            (0..count).map(move |slot| {
                let address = ResidentAddress {
                    building_index,
                    slot,
                };
                (address, resident_id(seed, dataset_version, address))
            })
        })
        .collect()
}

/// The building an agent walks to after draw number `draw` of its
/// next-target stream: one of `buildings` (in ascending ID order) other
/// than `current`, chosen as `below(draw, candidates)`. Integer-only, so the
/// same on every platform, and independent of when the agent arrived.
///
/// Returns `current` if it is the only building.
#[must_use]
pub fn next_target(
    seed: u64,
    agent: EntityId,
    draw: u32,
    buildings: &[EntityId],
    current: EntityId,
) -> EntityId {
    let candidates = buildings.iter().filter(|b| **b != current).count() as u64;
    let Some(n) = NonZeroU64::new(candidates) else {
        return current;
    };
    let x = Stream::new(seed, PURPOSE_NEXT_TARGET, &[agent.raw()]).draw(u64::from(draw));
    let k = below(x, n) as usize;
    *buildings
        .iter()
        .filter(|b| **b != current)
        .nth(k)
        .expect("k < candidates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purposes_are_distinct() {
        for (i, a) in PURPOSES.iter().enumerate() {
            for b in &PURPOSES[i + 1..] {
                assert_ne!(a.value(), b.value(), "{} / {}", a.label(), b.label());
            }
        }
    }

    #[test]
    fn same_address_and_seed_give_the_same_id() {
        let a = ResidentAddress {
            building_index: 2,
            slot: 3,
        };
        assert_eq!(resident_id(7, 1, a), resident_id(7, 1, a));
        assert_ne!(resident_id(7, 1, a), resident_id(8, 1, a));
        assert_ne!(resident_id(7, 1, a), resident_id(7, 2, a));
        assert_ne!(building_id(7, 1, 2), building_id(7, 1, 3));
    }

    #[test]
    fn counts_stay_in_range() {
        for seed in 0..200 {
            for b in 0..5 {
                let n = resident_count(seed, 1, b, Population::TOY);
                assert!((4..=12).contains(&n));
                assert_eq!(resident_count(seed, 1, b, Population::exactly(9)), 9);
            }
        }
    }

    #[test]
    fn next_target_never_picks_the_current_building() {
        let buildings: Vec<EntityId> = (0..5).map(|i| building_id(3, 1, i)).collect();
        let mut sorted = buildings.clone();
        sorted.sort();
        let agent = resident_id(
            3,
            1,
            ResidentAddress {
                building_index: 0,
                slot: 0,
            },
        );
        let mut current = sorted[0];
        for draw in 0..100 {
            let next = next_target(3, agent, draw, &sorted, current);
            assert_ne!(next, current);
            assert!(sorted.contains(&next));
            current = next;
        }
        assert_eq!(next_target(3, agent, 0, &sorted[..1], sorted[0]), sorted[0]);
    }
}
