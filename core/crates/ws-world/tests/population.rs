//! VS-003 Tier 1 fixtures and household/reference invariants.
use proptest::prelude::*;
use std::{collections::BTreeSet, fmt::Write};
use ws_world::slice::population::{PopulatedSlice, Role, WorkplaceKind, WorkplaceLocation};
use ws_world::slice::{BLOCK_BUILDINGS, DATASET_VERSION};

fn canonical(world: &PopulatedSlice) -> String {
    let b = world.baseline();
    let mut out = format!(
        "world {:016x} {:016x} {} {}\n",
        b.structural.seed,
        b.structural.dataset_version,
        b.structural.generator_version,
        b.generator_version
    );
    for (i, p) in world.exits().iter().enumerate() {
        writeln!(out, "exit {i} {} {}", p.lon_pdeg, p.lat_pdeg).unwrap();
    }
    for (id, w) in world.workplaces().iter() {
        let (home, exit) = match w.location {
            WorkplaceLocation::Building(h) => (h.raw(), 255),
            WorkplaceLocation::Exit(e) => (0, e),
        };
        writeln!(
            out,
            "workplace {:016x} {} {} {home:016x} {exit}",
            id.raw(),
            w.kind as u8,
            w.capacity
        )
        .unwrap();
    }
    for (id, h) in world.households().iter() {
        let members = h
            .members
            .iter()
            .map(|id| format!("{:016x}", id.raw()))
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            out,
            "household {:016x} {:016x} {} {} {members}",
            id.raw(),
            h.home.raw(),
            h.bin,
            h.unit
        )
        .unwrap();
    }
    for (id, p) in world.residents().iter() {
        writeln!(
            out,
            "resident {:016x} {:016x} {:016x} {} {} {} {} {} {} {:016x}",
            id.raw(),
            p.household.raw(),
            p.home.raw(),
            p.address.building_index,
            p.address.slot,
            p.age,
            p.role as u8,
            p.given_name,
            p.family_name,
            p.workplace.map_or(0, |w| w.raw())
        )
        .unwrap();
    }
    out
}
#[test]
fn independent_population_goldens_match_every_id_attribute_and_destination() {
    let actual: String = [0, 42, u64::MAX]
        .iter()
        .map(|seed| canonical(&PopulatedSlice::new(*seed).unwrap()))
        .collect();
    assert_eq!(actual, include_str!("golden/block_6049_population.txt"));
}
fn check(seed: u64) {
    let world = PopulatedSlice::new(seed).unwrap();
    assert_eq!(world, PopulatedSlice::new(seed).unwrap());
    assert_eq!(world.baseline().structural.dataset_version, DATASET_VERSION);
    assert_eq!(world.baseline().generator_version, 1);
    assert_eq!(world.households().len(), 10);
    assert!((10..=30).contains(&world.residents().len()));
    let mut all = BTreeSet::new();
    for id in world
        .world()
        .buildings()
        .ids()
        .iter()
        .chain(world.households().ids())
        .chain(world.residents().ids())
        .chain(world.workplaces().ids())
    {
        assert!(all.insert(*id));
    }
    let mut membership = BTreeSet::new();
    for (building_id, building) in world.world().buildings().iter() {
        let homes: Vec<_> = world
            .households()
            .iter()
            .filter(|(_, h)| h.home == building_id)
            .collect();
        assert_eq!(homes.len(), building.residential_units as usize);
        let units: BTreeSet<_> = homes.iter().map(|(_, h)| h.unit).collect();
        assert_eq!(units, (0..building.residential_units).collect());
        for (hid, h) in homes {
            assert!((1..=3).contains(&h.members.len()));
            let head = world.residents().get(h.members[0]).unwrap();
            assert!(head.age >= 22);
            for (member_slot, rid) in h.members.iter().enumerate() {
                assert!(membership.insert(*rid));
                let r = world.residents().get(*rid).unwrap();
                assert_eq!(
                    (r.home, r.household, r.address.building_index),
                    (building_id, hid, building.bin)
                );
                assert_eq!(r.address.slot, h.unit * 3 + member_slot as u32);
                assert_eq!(
                    *rid,
                    ws_world::ids::resident_id(seed, DATASET_VERSION, r.address)
                );
                assert_eq!(r.family_name, head.family_name);
                assert!(!r.given_name.is_empty());
                assert!(r.age <= 90);
                let expected_role = match r.age {
                    0..=4 => Role::Preschool,
                    5..=21 => Role::Student,
                    22..=64 => Role::Employed,
                    _ => Role::Retired,
                };
                assert_eq!(r.role, expected_role);
                match r.role {
                    Role::Student | Role::Employed => {
                        let workplace = world.workplaces().get(r.workplace.unwrap()).unwrap();
                        assert_eq!(
                            workplace.kind,
                            if r.role == Role::Student {
                                WorkplaceKind::Education
                            } else {
                                WorkplaceKind::Employment
                            }
                        );
                        match workplace.location {
                            WorkplaceLocation::Building(id) => assert_eq!(
                                world.world().buildings().get(id).unwrap().building_class,
                                "S1"
                            ),
                            WorkplaceLocation::Exit(e) => {
                                assert!((e as usize) < world.exits().len())
                            }
                        }
                    }
                    _ => assert_eq!(r.workplace, None),
                }
            }
        }
    }
    assert_eq!(membership.len(), world.residents().len());
    for (id, workplace) in world.workplaces().iter() {
        assert!(
            world
                .residents()
                .iter()
                .filter(|(_, r)| r.workplace == Some(id))
                .count()
                <= workplace.capacity as usize
        );
    }
    let points: Vec<_> = BLOCK_BUILDINGS
        .iter()
        .flat_map(|b| b.footprint.polygons)
        .flat_map(|p| p.rings)
        .flat_map(|r| r.points)
        .collect();
    assert_eq!(
        world.exits()[0].lon_pdeg,
        points.iter().map(|p| p.lon_pdeg).min().unwrap()
    );
    assert_eq!(
        world.exits()[1].lon_pdeg,
        points.iter().map(|p| p.lon_pdeg).max().unwrap()
    );
    assert_eq!(world.exits()[0].lat_pdeg, world.exits()[1].lat_pdeg);
}
proptest! {
    #![proptest_config(ProptestConfig { cases: 256, rng_seed: proptest::test_runner::RngSeed::Fixed(0x5753_0067_6049_0001), failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn households_membership_destinations_and_regeneration_are_consistent(seed in any::<u64>()) { check(seed); }
}
#[test]
fn all_roles_and_local_and_off_map_assignments_are_exercised() {
    let mut roles = BTreeSet::new();
    let mut local_and_remote = BTreeSet::new();
    for seed in 0..64 {
        check(seed);
        let w = PopulatedSlice::new(seed).unwrap();
        for (_, r) in w.residents().iter() {
            roles.insert(r.role as u8);
            if let Some(id) = r.workplace {
                local_and_remote.insert(matches!(
                    w.workplaces().get(id).unwrap().location,
                    WorkplaceLocation::Exit(_)
                ));
            }
        }
    }
    assert_eq!(roles, BTreeSet::from([0, 1, 2, 3]));
    assert_eq!(local_and_remote, BTreeSet::from([false, true]));
}
