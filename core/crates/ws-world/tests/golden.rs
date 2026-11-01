//! Tier 1 golden values: procedural building and resident IDs for fixed
//! seeds, identical on Linux x86-64 and macOS arm64 (#7 acceptance
//! criterion; 25-determinism-policy.md §25.2).

use ws_ecs::EntityId;
use ws_world::ids::{ResidentAddress, building_id, resident_id};

const GOLDEN: &str = include_str!("golden/ids.txt");
/// The dataset version the golden rows were generated with.
const DATASET_VERSION: u64 = 1;

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
            building_id(hex(f[1]), DATASET_VERSION, f[2].parse().unwrap()),
            id(f[3]),
            "{f:?}"
        );
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
            resident_id(hex(f[1]), DATASET_VERSION, address),
            id(f[4]),
            "{f:?}"
        );
    }
}
