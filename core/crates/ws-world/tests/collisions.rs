//! Procedural ID collision sweep (#7 acceptance criterion, ADR 0016 §1):
//! no two IDs collide within any of 10,000 seeds' toy worlds, nor across
//! all of them. A collision fails the test; nothing resolves it.

use std::collections::BTreeMap;

use ws_ecs::EntityId;
use ws_world::ids::{Population, building_id, residents};
use ws_world::{TOY_BUILDINGS, TOY_DATASET_VERSION};

#[test]
fn no_procedural_id_collisions_across_10000_seeds() {
    let buildings = u32::try_from(TOY_BUILDINGS.len()).unwrap();
    // Every ID seen, with the (seed, entity) that produced it.
    let mut all: BTreeMap<EntityId, (u64, String)> = BTreeMap::new();
    let mut per_world_total = 0usize;
    for seed in 0..10_000u64 {
        let mut ids: Vec<(EntityId, String)> = (0..buildings)
            .map(|i| {
                (
                    building_id(seed, TOY_DATASET_VERSION, i),
                    format!("building {i}"),
                )
            })
            .collect();
        ids.extend(
            residents(seed, TOY_DATASET_VERSION, buildings, Population::TOY)
                .into_iter()
                .map(|(a, id)| (id, format!("resident {}/{}", a.building_index, a.slot))),
        );
        per_world_total += ids.len();
        for (id, what) in ids {
            if let Some(prev) = all.insert(id, (seed, what.clone())) {
                panic!(
                    "procedural ID collision: {id} for seed {seed} {what} and seed {} {}",
                    prev.0, prev.1
                );
            }
        }
    }
    assert_eq!(all.len(), per_world_total);
    // About 45 IDs per seed: 5 buildings and 20 to 60 residents.
    assert!(per_world_total > 400_000, "{per_world_total} IDs checked");
    eprintln!("collision sweep: {per_world_total} procedural IDs over 10,000 seeds, no collisions");
}
