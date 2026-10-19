//! VS-002 Tier 1 source and identity goldens plus coordinate invariants.
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use std::collections::BTreeSet;
use ws_math::Geodetic;
use ws_world::slice::{BLOCK_BUILDINGS, DATASET_VERSION, GENERATOR_VERSION, GeoPoint, SliceWorld};

#[test]
fn golden_ids_and_real_positions_match_on_every_platform() {
    let fixture = include_str!("golden/block_6049.txt");
    assert!(fixture.contains(&format!("# dataset {DATASET_VERSION:016x}")));
    let mut count = 0;
    for row in fixture
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<_> = row.split_whitespace().collect();
        let seed = u64::from_str_radix(f[0], 16).unwrap();
        let bin: u32 = f[1].parse().unwrap();
        let world = SliceWorld::new(seed).unwrap();
        assert_eq!(world.buildings().len(), 4);
        let (id, building) = world
            .buildings()
            .iter()
            .find(|(_, b)| b.bin == bin)
            .unwrap();
        assert_eq!(id.raw(), u64::from_str_radix(f[2], 16).unwrap());
        assert_eq!(building.centroid.lon_pdeg, f[3].parse::<i64>().unwrap());
        assert_eq!(building.centroid.lat_pdeg, f[4].parse::<i64>().unwrap());
        assert_eq!(building.height_mm, f[5].parse::<u32>().unwrap());
        assert_eq!(building.building_class, f[6]);
        assert_eq!(world.baseline().seed, seed);
        assert_eq!(world.baseline().dataset_version, DATASET_VERSION);
        assert_eq!(world.baseline().generator_version, GENERATOR_VERSION);
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
fn extract_preserves_every_context_footprint_and_only_four_featured_entities() {
    assert_eq!(BLOCK_BUILDINGS.len(), 58);
    assert_eq!(
        BLOCK_BUILDINGS
            .iter()
            .filter(|b| b.feature_code == 5110)
            .count(),
        6
    );
    assert!(BLOCK_BUILDINGS.windows(2).all(|b| b[0].bin < b[1].bin));
    for source in BLOCK_BUILDINGS {
        assert!(source.footprint.is_valid());
        assert!(!source.address.is_empty());
        assert!(source.bbl > 3_060_490_000 && source.bbl < 3_060_500_000);
        assert_eq!(source.bbl % 10_000, u64::from(source.lot));
    }
    let world = SliceWorld::new(42).unwrap();
    let bins: BTreeSet<_> = world.buildings().iter().map(|(_, b)| b.bin).collect();
    assert_eq!(bins, BTreeSet::from([3153706, 3153708, 3153715, 3153734]));
    for (_, building) in world.buildings().iter() {
        assert_eq!(
            Some(building),
            BLOCK_BUILDINGS.iter().find(|b| b.bin == building.bin)
        );
    }
}

#[test]
fn seeded_ids_are_unique_and_do_not_alias_the_toy_dataset() {
    let mut all = BTreeSet::new();
    for seed in 0..10_000 {
        for (id, _) in SliceWorld::new(seed).unwrap().buildings().iter() {
            assert!(all.insert(id), "collision across 10,000 seeds: {id}");
            for toy_index in 0..5 {
                assert_ne!(id, ws_world::ids::building_id(seed, 1, toy_index));
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, rng_seed: RngSeed::Fixed(0x5753_0066_6049_0001), ..ProptestConfig::default() })]
    #[test]
    fn fixed_point_positions_convert_to_owning_cells_and_round_trip(
        index in 0usize..58, ellipsoidal_mm in -100_000i64..100_000,
    ) {
        let source = BLOCK_BUILDINGS[index];
        for point in std::iter::once(source.centroid).chain(
            source.footprint.polygons.iter().flat_map(|p| p.rings).flat_map(|r| r.points).copied()
        ) {
            let position = point.surface_position(ellipsoidal_mm).unwrap();
            prop_assert_eq!(position.occupied_cell(), Ok(position.cell()));
            prop_assert_eq!(position.reexpress().unwrap(), position);
            let expected = Geodetic::from_degrees(point.lat_pdeg as f64 / 1e12,
                point.lon_pdeg as f64 / 1e12, ellipsoidal_mm as f64 / 1000.0).unwrap().to_ecef();
            prop_assert!(position.to_ecef().distance(expected) < 0.0001);
        }
    }
}

#[test]
fn source_navd88_height_is_not_used_as_ellipsoidal_height() {
    let source = BLOCK_BUILDINGS[0];
    assert_eq!(source.ground_navd88_mm, 22_560);
    let zero = source.centroid.surface_position(0).unwrap();
    let raised = source.centroid.surface_position(10_000).unwrap();
    assert!((zero.to_ecef().distance(raised.to_ecef()) - 10.0).abs() < 0.0001);
    assert!(
        GeoPoint {
            lat_pdeg: 91_000_000_000_000,
            lon_pdeg: 0
        }
        .surface_position(0)
        .is_err()
    );
}
