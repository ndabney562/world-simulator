//! Procedural identities (ADR 0016 §1). All Tier 1: integer-only functions
//! of the world seed, the dataset version, and a generation address.

use ws_ecs::EntityId;
use ws_rng::{Purpose, derive};

/// Purpose of building IDs; address `[dataset_version, building_index]`.
pub const PURPOSE_BUILDING_ID: Purpose = Purpose::new("ws-world/id/building");
/// Purpose of resident IDs; address `[dataset_version, building_index, slot]`.
pub const PURPOSE_RESIDENT_ID: Purpose = Purpose::new("ws-world/id/resident");
/// Purpose of the deleted toy residents-per-building decision (ADR 0040);
/// kept reserved so it is never reused.
pub const PURPOSE_RESIDENT_COUNT: Purpose = Purpose::new("ws-world/gen/resident-count");
/// Purpose of the deleted toy next-target stream (ADR 0040); kept reserved.
pub const PURPOSE_NEXT_TARGET: Purpose = Purpose::new("ws-world/agent/next-target");
/// Key of the deleted toy state hash (ADR 0040); kept reserved.
pub const PURPOSE_STATE_HASH: Purpose = Purpose::new("ws-world/state-hash");
/// Key of the deleted toy structural digest (ADR 0040); kept reserved.
pub const PURPOSE_STRUCTURAL_DIGEST: Purpose = Purpose::new("ws-world/structural-digest");

/// Every purpose this crate has used; a test checks they are distinct.
pub const PURPOSES: [Purpose; 6] = [
    PURPOSE_BUILDING_ID,
    PURPOSE_RESIDENT_ID,
    PURPOSE_RESIDENT_COUNT,
    PURPOSE_NEXT_TARGET,
    PURPOSE_STATE_HASH,
    PURPOSE_STRUCTURAL_DIGEST,
];

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
}
