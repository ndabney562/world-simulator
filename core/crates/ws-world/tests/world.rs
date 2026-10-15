//! Toy-world behaviour: same-seed reproduction, insertion-order
//! independence, RNG isolation, arrival-time independence, bounds, and
//! cell-edge stability (#7 acceptance criteria; ADR 0016).

use std::collections::BTreeMap;

use ws_ecs::EntityId;
use ws_math::reference::BAY_RIDGE;
use ws_math::{CellFrame, CellId, EnuFrame, Position, SECTOR_LEVEL, SurfacePos};
use ws_rng::{Purpose, Stream};
use ws_time::SimDuration;
use ws_world::ids::{Population, ResidentAddress, resident_id};
use ws_world::{Building, BuildingSite, Dataset, Resident, ToyWorld, Walk, WalkParams};

const DT: SimDuration = SimDuration::from_micros(100_000);

fn toy(seed: u64) -> ToyWorld {
    ToyWorld::generate(
        seed,
        &Dataset::toy().unwrap(),
        Population::TOY,
        WalkParams::DEFAULT,
    )
    .unwrap()
}

/// A toy world with fast walkers, so target sequences get long in few
/// ticks (a trip at 1.4 m/s can take 7,000 ticks).
fn fast_toy(seed: u64, speed_m_s: f64, wait_steps: u32) -> ToyWorld {
    ToyWorld::generate(
        seed,
        &Dataset::toy().unwrap(),
        Population::TOY,
        WalkParams {
            speed_m_s,
            wait_steps,
        },
    )
    .unwrap()
}

/// The per-tick state hashes of `ticks` steps.
fn hashes(mut w: ToyWorld, ticks: usize) -> Vec<u64> {
    (0..ticks)
        .map(|_| {
            w.step(DT).unwrap();
            w.state_hash()
        })
        .collect()
}

#[test]
fn toy_world_has_a_few_dozen_agents_in_several_cells() {
    let w = toy(42);
    assert_eq!(w.buildings().len(), 5);
    assert!(
        (20..=60).contains(&w.residents().len()),
        "{}",
        w.residents().len()
    );
    let cells: std::collections::BTreeSet<CellId> =
        w.buildings().iter().map(|(_, b)| b.pos.cell()).collect();
    assert!(
        cells.len() >= 2,
        "walks must cross sector-cell edges: {cells:?}"
    );
    for (_, b) in w.buildings().iter() {
        assert_eq!(b.pos.occupied_cell(), Ok(b.pos.cell()));
    }
}

/// #7: two runs with the same seed give identical per-tick state hashes for
/// 10,000 ticks; different seeds differ.
#[test]
fn same_seed_gives_identical_per_tick_hashes_for_10000_ticks() {
    let a = hashes(toy(7), 10_000);
    let b = hashes(toy(7), 10_000);
    assert_eq!(a, b);
    let c = hashes(toy(8), 10_000);
    assert!(
        a.iter().zip(&c).all(|(x, y)| x != y),
        "different seeds differ at every tick"
    );
}

/// Rebuilds `w`'s entities into a new world in the order given by `perm`.
fn rebuilt(w: &ToyWorld, order_seed: u64) -> ToyWorld {
    let mut buildings: Vec<(EntityId, Building)> =
        w.buildings().iter().map(|(i, b)| (i, *b)).collect();
    let mut residents: Vec<(EntityId, Resident, SurfacePos, Walk)> = w
        .residents()
        .iter()
        .map(|(i, r)| {
            (
                i,
                *r,
                *w.positions().get(i).unwrap(),
                *w.walks().get(i).unwrap(),
            )
        })
        .collect();
    shuffle(&mut buildings, order_seed);
    shuffle(&mut residents, order_seed ^ 0xffff);
    let mut out = ToyWorld::empty(w.seed(), w.dataset_version(), WalkParams::DEFAULT);
    for (i, b) in buildings {
        out.insert_building(i, b).unwrap();
    }
    for (i, r, p, s) in residents {
        out.insert_resident_state(i, r, p, s).unwrap();
    }
    out
}

fn shuffle<T>(v: &mut [T], seed: u64) {
    let mut x = seed.wrapping_mul(2).wrapping_add(1);
    for i in (1..v.len()).rev() {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        v.swap(i, ((x >> 33) % (i as u64 + 1)) as usize);
    }
}

/// #7: creating entities in a different insertion order yields the same
/// state hash, before and after stepping.
#[test]
fn insertion_order_does_not_change_state_or_hash() {
    let mut w = toy(42);
    for _ in 0..300 {
        w.step(DT).unwrap();
    }
    let reference = hashes(w.clone(), 500);
    for order in 0..20 {
        let r = rebuilt(&w, order);
        assert_eq!(r, w, "order {order}");
        assert_eq!(r.state_hash(), w.state_hash());
        assert_eq!(hashes(r, 500), reference, "order {order}");
    }
    // Generation itself: residents inserted building by building in reverse.
    let ds = Dataset::toy().unwrap();
    let g = toy(9);
    let mut rev = ToyWorld::empty(9, ds.version, WalkParams::DEFAULT);
    for (i, b) in g.buildings().iter().collect::<Vec<_>>().into_iter().rev() {
        rev.insert_building(i, *b).unwrap();
    }
    for (i, r) in g.residents().iter().collect::<Vec<_>>().into_iter().rev() {
        rev.insert_resident(i, *r).unwrap();
    }
    assert_eq!(rev.state_hash(), g.state_hash());
}

/// Every canonical component changes the hash; derived `velocity` does
/// not (ADR 0017 §11).
#[test]
fn state_hash_covers_every_component() {
    let w = toy(3);
    let h = w.state_hash();
    let id = w.residents().ids()[0];
    let r = *w.residents().get(id).unwrap();
    let p = *w.positions().get(id).unwrap();
    let s = *w.walks().get(id).unwrap();
    let variant = |r: Resident, p: SurfacePos, s: Walk| {
        let mut v = ToyWorld::empty(w.seed(), w.dataset_version(), WalkParams::DEFAULT);
        for (i, b) in w.buildings().iter() {
            v.insert_building(i, *b).unwrap();
        }
        for (i, _) in w.residents().iter() {
            if i == id {
                v.insert_resident_state(i, r, p, s).unwrap();
            } else {
                v.insert_resident_state(
                    i,
                    *w.residents().get(i).unwrap(),
                    *w.positions().get(i).unwrap(),
                    *w.walks().get(i).unwrap(),
                )
                .unwrap();
            }
        }
        v.state_hash()
    };
    assert_eq!(variant(r, p, s), h);
    // A position change is visible. Only a walking agent may stand away
    // from its target's centre (ADR 0017 §6).
    let walking = Walk {
        arrived: false,
        targets_drawn: 1,
        ..s
    };
    let moved = SurfacePos::new(
        p.cell(),
        [p.offset()[0] + 0.001, p.offset()[1], p.offset()[2]],
    )
    .unwrap();
    assert_ne!(variant(r, moved, walking), variant(r, p, walking));
    assert_ne!(variant(r, p, walking), h);
    assert_ne!(
        variant(
            r,
            p,
            Walk {
                targets_drawn: 1,
                ..s
            }
        ),
        h
    );
    assert_ne!(variant(r, p, Walk { wait_steps: 3, ..s }), h);
    // Velocity is derived state, not hashed (ADR 0017 §11): worlds that
    // differ only in velocity hash identically (`velocity_is_derived` in
    // tests/lod.rs shows they also evolve identically).
    assert_eq!(
        variant(
            r,
            p,
            Walk {
                velocity: [0.5, 0.0, 0.0],
                ..s
            }
        ),
        h
    );
    assert_ne!(
        variant(
            Resident {
                address: ResidentAddress {
                    slot: 99,
                    ..r.address
                },
                ..r
            },
            p,
            s
        ),
        h
    );
    assert_ne!(toy(4).state_hash(), h, "the seed is part of the state");
}

/// Each agent's sequence of target buildings over `ticks` ticks.
fn target_sequences(mut w: ToyWorld, ticks: usize) -> BTreeMap<EntityId, Vec<EntityId>> {
    let mut seq: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
    for _ in 0..ticks {
        w.step(DT).unwrap();
        for (id, walk) in w.walks().iter() {
            let s = seq.entry(id).or_default();
            if !walk.arrived && s.len() < walk.targets_drawn as usize {
                s.push(walk.target);
            }
        }
    }
    seq
}

/// An unrelated RNG draw, or an unrelated extra agent, does not change any
/// agent's target sequence.
#[test]
fn unrelated_draws_and_agents_do_not_change_target_sequences() {
    let base = target_sequences(fast_toy(42, 20.0, 10), 6000);
    assert!(
        base.values().all(|s| s.len() >= 3),
        "every agent made several trips"
    );

    // Unrelated draws between steps, from another purpose and from the
    // next-target purpose of an entity that does not exist.
    let mut w = fast_toy(42, 20.0, 10);
    let mut noisy: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
    let unrelated = Stream::new(42, Purpose::new("ws-world/test/unrelated"), &[]);
    let ghost = Stream::new(
        42,
        ws_world::ids::PURPOSE_NEXT_TARGET,
        &[0x4123_4567_89ab_cdef],
    );
    for t in 0..6000u64 {
        let _ = unrelated.draw(t);
        let _ = ghost.draw(t);
        w.step(DT).unwrap();
        for (id, walk) in w.walks().iter() {
            let s = noisy.entry(id).or_default();
            if !walk.arrived && s.len() < walk.targets_drawn as usize {
                s.push(walk.target);
            }
        }
    }
    assert_eq!(noisy, base);

    // An extra resident in building 0, slot 1000, which draws targets too.
    let mut extra = fast_toy(42, 20.0, 10);
    let address = ResidentAddress {
        building_index: 0,
        slot: 1000,
    };
    let home = extra
        .buildings()
        .iter()
        .find(|(_, b)| b.index == 0)
        .unwrap()
        .0;
    let new_id = resident_id(42, extra.dataset_version(), address);
    extra
        .insert_resident(new_id, Resident { address, home })
        .unwrap();
    let mut with_extra = target_sequences(extra, 6000);
    assert!(with_extra.remove(&new_id).unwrap().len() >= 3);
    assert_eq!(with_extra, base);
}

/// Arrival-time differences do not change which targets are chosen: at a
/// different walking speed, every agent visits the same sequence of
/// buildings (as far as the slower run gets).
#[test]
fn arrival_time_does_not_change_target_choice() {
    let fast = target_sequences(fast_toy(5, 20.0, 10), 6000);
    let slow = target_sequences(fast_toy(5, 13.0, 37), 6000);
    for (id, s) in &slow {
        let f = &fast[id];
        let n = s.len().min(f.len());
        assert!(n >= 3, "agent {id} made {n} trips");
        assert_eq!(&s[..n], &f[..n], "agent {id}");
    }
}

/// #7: agents stay within the toy world's bounds, are always in their own
/// cell, and really cross cell edges.
#[test]
fn agents_stay_in_bounds_and_in_their_own_cell() {
    let mut w = toy(11);
    let frame = EnuFrame::new(BAY_RIDGE);
    // The buildings' extent plus 1 m.
    let local: Vec<Position<ws_math::Enu>> = w
        .buildings()
        .iter()
        .map(|(_, b)| frame.to_enu(b.pos.to_ecef()))
        .collect();
    let (min_e, max_e) = local
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let (min_n, max_n) = local
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let mut cells: BTreeMap<EntityId, CellId> =
        w.positions().iter().map(|(i, p)| (i, p.cell())).collect();
    let mut crossings = 0;
    for _ in 0..6000 {
        w.step(DT).unwrap();
        for (id, p) in w.positions().iter() {
            assert_eq!(p.occupied_cell(), Ok(p.cell()), "{id}");
            let l = frame.to_enu(p.to_ecef());
            assert!(
                l.x >= min_e - 1.0 && l.x <= max_e + 1.0,
                "{id} east {}",
                l.x
            );
            assert!(
                l.y >= min_n - 1.0 && l.y <= max_n + 1.0,
                "{id} north {}",
                l.y
            );
            assert!(l.z.abs() < 1.0, "{id} up {}", l.z);
            let c = cells.insert(id, p.cell()).unwrap();
            if c != p.cell() {
                crossings += 1;
            }
        }
    }
    assert!(
        crossings > 50,
        "agents crossed cell edges {crossings} times"
    );
}

/// The 64-bit sector cell of a point, and a point on an edge of `cell`
/// found by bisection along a ray from its centre.
fn edge_point(cell: CellId, angle: f64) -> Position<ws_math::Ecef> {
    let f = cell.enu_frame();
    let occ = |t: f64| {
        let p = f.to_ecef(Position::new(t * angle.cos(), t * angle.sin(), 0.0));
        CellId::from_ecef(p).unwrap().parent(SECTOR_LEVEL).unwrap()
    };
    let (mut lo, mut hi) = (0.0_f64, 2000.0_f64);
    for _ in 0..80 {
        let m = 0.5 * (lo + hi);
        if occ(m) == cell { lo = m } else { hi = m }
    }
    f.to_ecef(Position::new(lo * angle.cos(), lo * angle.sin(), 0.0))
}

/// Agents waiting at, or walking to and from, buildings that stand exactly
/// on sector-cell edges never change cell without moving, and are always
/// in their own cell. Before ADR 0016 such a building's centre could not
/// even be stored stably: re-expressing it could alternate between cells.
#[test]
fn agents_at_cell_edges_do_not_oscillate() {
    let home = CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap();
    // Four buildings on four different edge points, plus the ADR 0016 case.
    let mut sites: Vec<BuildingSite> = [0.3, 1.9, 3.5, 5.0]
        .iter()
        .map(|&a| BuildingSite {
            pos: SurfacePos::from_ecef(edge_point(home, a)).unwrap(),
            half_extent_m: 5.0,
            height_m: 5.0,
        })
        .collect();
    let adr_case = SurfacePos::new(
        CellId::from_raw(0x89c2_455c_0000_0000).unwrap(),
        [
            f32::from_bits(0xc29b_4918),
            f32::from_bits(0xc405_dcb4),
            0.0,
        ],
    )
    .unwrap();
    sites.push(BuildingSite {
        pos: SurfacePos::from_ecef(adr_case.to_ecef()).unwrap(),
        half_extent_m: 5.0,
        height_m: 5.0,
    });
    for s in &sites {
        assert_eq!(s.pos.occupied_cell(), Ok(s.pos.cell()));
        assert_eq!(s.pos.reexpress(), Ok(s.pos));
    }
    let ds = Dataset {
        version: 900,
        sites,
    };
    let mut w = ToyWorld::generate(
        1,
        &ds,
        Population::exactly(6),
        WalkParams {
            speed_m_s: 1.4,
            wait_steps: 200,
        },
    )
    .unwrap();
    let mut last: BTreeMap<EntityId, SurfacePos> =
        w.positions().iter().map(|(i, p)| (i, *p)).collect();
    let mut parked_ticks = 0;
    for _ in 0..8000 {
        w.step(DT).unwrap();
        for (id, p) in w.positions().iter() {
            assert_eq!(p.occupied_cell(), Ok(p.cell()), "{id}");
            let before = last.insert(id, *p).unwrap();
            if before.to_ecef() == p.to_ecef() {
                // Not moving: same cell, same bits.
                assert_eq!(before, *p, "{id} changed cell while standing still");
                parked_ticks += 1;
            }
        }
    }
    assert!(
        parked_ticks > 5_000,
        "agents waited at edge buildings for {parked_ticks} agent-ticks"
    );
    // Every waiting agent is exactly at a building centre on an edge.
    for (id, walk) in w.walks().iter() {
        if walk.arrived {
            let b = w.buildings().get(walk.target).unwrap();
            assert_eq!(w.positions().get(id), Some(&b.pos));
            let frame = CellFrame::new(b.pos.cell());
            assert_eq!(b.pos.to_ecef_with(&frame), Ok(b.pos.to_ecef()));
        }
    }
}
