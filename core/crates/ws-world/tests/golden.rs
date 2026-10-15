//! Tier 1 golden values: procedural IDs, residents per building, and target
//! choices for fixed seeds, identical on Linux x86-64 and macOS arm64
//! (#7 acceptance criterion; 25-determinism-policy.md §25.2).

use ws_ecs::EntityId;
use ws_world::ids::{
    Population, ResidentAddress, building_id, next_target, resident_count, resident_id,
};
use ws_world::{Dataset, TOY_DATASET_VERSION, ToyWorld, WalkParams};

const GOLDEN: &str = include_str!("golden/toy_world.txt");

fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    GOLDEN
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s, 16).unwrap()
}

fn id(s: &str) -> EntityId {
    EntityId::from_raw(hex(s)).unwrap()
}

#[test]
fn golden_building_ids_match_exactly() {
    let r = rows("building");
    assert_eq!(r.len(), 25);
    for f in r {
        assert_eq!(
            building_id(hex(f[1]), TOY_DATASET_VERSION, f[2].parse().unwrap()),
            id(f[3]),
            "{f:?}"
        );
    }
}

#[test]
fn golden_resident_counts_match_exactly() {
    let r = rows("count");
    assert_eq!(r.len(), 25);
    for f in r {
        let n = resident_count(
            hex(f[1]),
            TOY_DATASET_VERSION,
            f[2].parse().unwrap(),
            Population::TOY,
        );
        assert_eq!(n, f[3].parse::<u32>().unwrap(), "{f:?}");
    }
}

#[test]
fn golden_resident_ids_match_exactly() {
    let r = rows("resident");
    assert_eq!(r.len(), 76, "33 residents for seed 0 and 43 for seed 42");
    for f in r {
        let address = ResidentAddress {
            building_index: f[2].parse().unwrap(),
            slot: f[3].parse().unwrap(),
        };
        assert_eq!(
            resident_id(hex(f[1]), TOY_DATASET_VERSION, address),
            id(f[4]),
            "{f:?}"
        );
    }
}

#[test]
fn golden_target_choices_match_exactly() {
    let seed = 42;
    let mut buildings: Vec<EntityId> = (0..5)
        .map(|i| building_id(seed, TOY_DATASET_VERSION, i))
        .collect();
    buildings.sort();
    let index_of = |b: EntityId| {
        (0..5)
            .find(|&i| building_id(seed, TOY_DATASET_VERSION, i) == b)
            .unwrap()
    };
    let world = ToyWorld::generate(
        seed,
        &Dataset::toy().unwrap(),
        Population::TOY,
        WalkParams::DEFAULT,
    )
    .unwrap();
    let r = rows("target");
    assert_eq!(r.len(), 18);
    let mut current = None;
    for f in r {
        assert_eq!(hex(f[1]), seed);
        let agent = id(f[2]);
        let k: u32 = f[3].parse().unwrap();
        if k == 0 {
            current = Some(world.residents().get(agent).expect("resident exists").home);
        }
        let next = next_target(seed, agent, k, &buildings, current.unwrap());
        assert_eq!(index_of(next), f[4].parse::<u32>().unwrap(), "{f:?}");
        current = Some(next);
    }
}

#[test]
fn generated_world_holds_exactly_the_golden_entities() {
    for seed in [0, 42] {
        let world = ToyWorld::generate(
            seed,
            &Dataset::toy().unwrap(),
            Population::TOY,
            WalkParams::DEFAULT,
        )
        .unwrap();
        let mut expected: Vec<EntityId> = rows("resident")
            .into_iter()
            .filter(|f| hex(f[1]) == seed)
            .map(|f| id(f[4]))
            .collect();
        expected.sort();
        assert_eq!(world.residents().ids(), expected.as_slice());
        let buildings: Vec<EntityId> = world.buildings().ids().to_vec();
        let mut golden: Vec<EntityId> = rows("building")
            .into_iter()
            .filter(|f| hex(f[1]) == seed)
            .map(|f| id(f[3]))
            .collect();
        golden.sort();
        assert_eq!(buildings, golden);
        for (rid, r) in world.residents().iter() {
            assert_eq!(resident_id(seed, TOY_DATASET_VERSION, r.address), rid);
            assert_eq!(
                r.home,
                building_id(seed, TOY_DATASET_VERSION, r.address.building_index)
            );
        }
    }
}
