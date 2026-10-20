//! Camera policy, independent every-tick counts and exact observation conservation.
use proptest::prelude::*;
use ws_events::Activity;
use ws_life::{
    Location, Place, Schedules,
    behavior::{Behavior, Detail, FineLocation},
    lod::{CameraInterest, Controller, LodError, Mode, OBSERVATION_WINDOW_US, Summary},
    navigation::Point,
};
use ws_rng::{Purpose, Stream};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;
const STEP: i64 = 100_000;
fn t(us: i64) -> SimTime {
    SimTime::from_micros_since_epoch(us)
}
fn setup(seed: u64, start: i64, epoch: i64) -> (PopulatedSlice, Schedules, Behavior, Controller) {
    let p = PopulatedSlice::new(seed).unwrap();
    let s = Schedules::new(&p, t(epoch)).unwrap();
    let b = Behavior::new(&p, &s, t(start)).unwrap();
    let c = Controller::new(&p, &b, t(start), t(epoch)).unwrap();
    (p, s, b, c)
}
fn wide() -> CameraInterest {
    CameraInterest {
        center: Point { east: 0, north: 0 },
        radius_mm: 1_000_000_000,
    }
}
fn overview() -> CameraInterest {
    CameraInterest {
        radius_mm: 0,
        ..wide()
    }
}
fn advance(b: &mut Behavior, c: &mut Controller, us: i64, scale: u32) -> usize {
    let mut bp = b.prepare(t(us)).unwrap();
    let cp = c.prepare(&mut bp, scale).unwrap();
    let changes = cp.changes().count();
    assert_eq!(
        cp.summary().l1 + cp.summary().l2,
        bp.residents().len() as u32
    );
    bp.commit();
    cp.commit();
    changes
}
fn independent_counts(
    p: &PopulatedSlice,
    s: &Schedules,
    b: &Behavior,
    c: &Controller,
    us: i64,
) -> Summary {
    let got = c.summary(b, t(us)).unwrap();
    let mut rows = [(0u32, [0u32; 6], 0u32); 4];
    let mut transit = 0;
    let mut off = 0;
    for (id, r) in p.residents().iter() {
        let answer = s.get(id).unwrap().at(s.epoch(), t(us));
        assert_eq!(b.common(id, t(us)).unwrap().logical, answer);
        let owner = match answer.location {
            Location::AtBuilding(id)
            | Location::InTransit {
                to: Place::Building(id),
                ..
            } => id,
            _ => r.home,
        };
        let i = p
            .world()
            .buildings()
            .ids()
            .iter()
            .position(|v| *v == owner)
            .unwrap();
        rows[i].2 += 1;
        match answer.location {
            Location::AtBuilding(id) => {
                let i = p
                    .world()
                    .buildings()
                    .ids()
                    .iter()
                    .position(|v| *v == id)
                    .unwrap();
                rows[i].0 += 1;
                rows[i].1[answer.activity as usize] += 1;
            }
            Location::InTransit { .. } => transit += 1,
            Location::OffMap(..) => off += 1,
        }
    }
    for (got, (occupancy, activities, governed)) in got.buildings.iter().zip(rows) {
        assert_eq!(
            (got.occupancy, got.activities, got.governed),
            (occupancy, activities, governed)
        );
    }
    assert_eq!((got.in_transit, got.off_map_stationary), (transit, off));
    assert_eq!(
        got.local_stationary + transit + off,
        p.residents().len() as u32
    );
    assert_eq!(got.population, p.residents().len() as u32);
    got
}
#[test]
fn inputs_are_bounded_and_queries_idempotence_and_prepared_drop_preserve_state() {
    let (_p, _s, mut b, mut c) = setup(42, 0, 0);
    let bh = b.state_hash();
    let ch = c.state_hash();
    for input in [
        CameraInterest {
            center: Point {
                east: i64::MIN,
                north: 0,
            },
            ..wide()
        },
        CameraInterest {
            radius_mm: 1_000_000_001,
            ..wide()
        },
    ] {
        assert_eq!(c.set_camera(input), Err(LodError::Input));
        assert_eq!(c.state_hash(), ch);
    }
    c.set_camera(wide()).unwrap();
    let ch = c.state_hash();
    c.set_camera(wide()).unwrap();
    assert_eq!(c.state_hash(), ch);
    for us in [100_000, 200_000, 800_000] {
        assert_eq!(advance(&mut b, &mut c, us, 1), 0);
        assert!(
            c.nodes()
                .iter()
                .all(|n| n.mode == Mode::L2 && n.changed_at == t(0))
        );
    }
    {
        let mut bp = b.prepare(t(1_000_000)).unwrap();
        let cp = c.prepare(&mut bp, 1).unwrap();
        assert_eq!(cp.changes().count(), 4);
        drop(cp);
        drop(bp);
    }
    assert_eq!(c.state_hash(), ch);
    assert_eq!(b.state_hash(), bh);
    assert_eq!(advance(&mut b, &mut c, 1_000_000, 1), 4);
    assert_eq!(c.summary(&b, t(1_000_000)).unwrap().l2, 0);
    c.set_camera(wide()).unwrap();
    assert!(c.nodes().iter().all(|n| n.changed_at == t(1_000_000)));
    c.set_camera(overview()).unwrap();
    advance(&mut b, &mut c, 1_100_000, 1);
    assert!(c.nodes().iter().all(|n| n.mode == Mode::L1));
    assert_eq!(advance(&mut b, &mut c, 1_200_000, 1), 4);
    assert!(c.nodes().iter().all(|n| n.mode == Mode::L3));
    let view = c.view(&b, t(1_200_000)).unwrap();
    assert!(
        view.residents
            .iter()
            .all(|r| r.detail == Detail::L2 && r.fine.is_none())
    );
    let before = (b.state_hash(), c.state_hash());
    c.discard_observations();
    assert_eq!(before, (b.state_hash(), c.state_hash()));
    assert_eq!(view, c.view(&b, t(1_200_000)).unwrap());
}
#[test]
fn outbound_boundary_caches_the_true_off_map_variant_and_expiry_never_changes_projection() {
    let (p, s, _, _) = setup(42, 0, 0);
    let (id, row) = s
        .profiles()
        .iter()
        .find_map(|p| {
            p.weekday()
                .iter()
                .find(|r| {
                    matches!(
                        (r.origin, r.destination),
                        (Some(Place::Building(_)), Place::OffMap(..))
                    )
                })
                .map(|r| (p.resident, *r))
        })
        .unwrap();
    let departure = i64::from(row.minute) * 60_000_000;
    let mut b = Behavior::new(&p, &s, t(departure)).unwrap();
    let mut c = Controller::new(&p, &b, t(departure), t(0)).unwrap();
    let ticks = b
        .navigation()
        .route(row.origin.unwrap(), row.destination)
        .unwrap()
        .walking_ticks();
    assert!(ticks > 10);
    let crossing = departure + i64::try_from(ticks).unwrap() * STEP;
    c.set_camera(wide()).unwrap();
    for us in (departure + STEP..crossing).step_by(STEP as usize) {
        advance(&mut b, &mut c, us, 1);
    }
    assert_eq!(
        b.snapshot(id, t(crossing - STEP)).unwrap().detail,
        Detail::L1
    );
    assert!(matches!(
        b.snapshot(id, t(crossing - STEP)).unwrap().fine,
        Some(FineLocation::Local { walking: true, .. })
    ));
    advance(&mut b, &mut c, crossing, 1);
    let o = c.observation(id, t(crossing)).unwrap();
    assert_eq!(o.time, t(crossing));
    assert!(matches!(o.fine, FineLocation::OffMapTransit { .. }));
    assert_eq!(b.common(id, t(crossing)).unwrap().detail, Detail::L2);
    assert_eq!(
        c.observed_projection(&b, id, t(crossing)).unwrap(),
        Some(o.fine)
    );
    let last = crossing + OBSERVATION_WINDOW_US as i64;
    for us in (crossing + STEP..=last + STEP).step_by(STEP as usize) {
        advance(&mut b, &mut c, us, 1);
        independent_counts(&p, &s, &b, &c, us);
        let hit = c.observed_projection(&b, id, t(us)).unwrap();
        if us <= last {
            assert_eq!(hit, Some(b.projected_location(id, t(us)).unwrap()));
        } else {
            assert!(hit.is_none());
        }
    }
    let before = (
        b.state_hash(),
        c.state_hash(),
        c.view(&b, t(last + STEP)).unwrap(),
    );
    c.discard_observations();
    assert_eq!(
        before,
        (
            b.state_hash(),
            c.state_hash(),
            c.view(&b, t(last + STEP)).unwrap()
        )
    );
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]
    #[test]
    fn thousand_random_camera_cycles_conserve_individuals_needs_counters_and_positions(seed in any::<u64>(),offset in -1_000_000i64..1_000_000i64) {
        let epoch=offset*STEP;
        let start=epoch+7*3_600_000_000;
        let (p,s,mut b,mut c)=setup(seed,start,epoch);
        let mut reference=b.clone();
        let ids=p.residents().ids();
        for &id in ids { reference.set_detail(id,Detail::L1,t(start)).unwrap(); }
        let rng=Stream::new(seed,Purpose::new("test/slice-camera-cycles"),&[]);
        let mut us=start;
        for cycle in 0..1000u64 {
            let k=cycle*4;
            let camera=CameraInterest {
                center:Point {east:(rng.draw(k)%800_000) as i64-200_000,north:(rng.draw(k+1)%600_000) as i64-200_000},
                radius_mm:[0,20_000,80_000,160_000,1_000_000_000][(rng.draw(k+2)%5) as usize],
            };
            let before=ids.iter().map(|&id|b.common(id,t(us)).unwrap()).collect::<Vec<_>>();
            c.set_camera(camera).unwrap();
            prop_assert_eq!(&before,&ids.iter().map(|&id|b.common(id,t(us)).unwrap()).collect::<Vec<_>>());
            for _ in 0..1+rng.draw(k+3)%15 {
                us+=STEP;
                advance(&mut b,&mut c,us,if cycle%17==0 {100}else{1});
                independent_counts(&p,&s,&b,&c,us);
            }
            reference.advance_to(t(us)).unwrap();
            let view=c.view(&b,t(us)).unwrap();
            prop_assert!(view.residents.iter().map(|r|r.resident).eq(ids.iter().copied()));
            for r in view.residents {
                let expected=reference.snapshot(r.resident,t(us)).unwrap();
                prop_assert_eq!((r.needs,r.action,r.decisions,r.logical),(expected.needs,expected.action,expected.decisions,expected.logical));
                prop_assert_eq!(b.projected_location(r.resident,t(us)).unwrap(),expected.fine.unwrap());
                if let Some(fine)=r.fine { prop_assert_eq!(fine,expected.fine.unwrap()); }
                prop_assert!(r.needs.values().iter().all(|v|*v<=100_000));
            }
        }
    }
}
#[test]
fn one_week_every_physical_tick_counts_match_an_independent_individual_scan() {
    let (p, s, mut b, mut c) = setup(42, 0, 0);
    let mut stationary = 0;
    let mut transit = 0;
    let mut off = 0;
    for tick in 1..=7 * 864_000i64 {
        if tick % 1200 == 1 {
            c.set_camera(if tick % 3600 == 1 { overview() } else { wide() })
                .unwrap();
        }
        let us = tick * STEP;
        advance(&mut b, &mut c, us, 1);
        let counts = independent_counts(&p, &s, &b, &c, us);
        stationary += u64::from(counts.local_stationary);
        transit += u64::from(counts.in_transit);
        off += u64::from(counts.off_map_stationary);
    }
    assert!(stationary > 0 && transit > 0 && off > 0);
    assert!(
        s.profiles()
            .iter()
            .any(|p| p.weekday().iter().any(|r| r.activity == Activity::Work))
    );
}

#[test]
fn extreme_separated_epochs_and_coordinate_limits_use_wide_integer_policy_arithmetic() {
    let last = i64::MAX - i64::MAX.rem_euclid(STEP);
    let first = i64::MIN + (STEP - i64::MIN.rem_euclid(STEP)) % STEP;
    for (start, epoch) in [(last - 2_000_000, first), (first + 2_000_000, last)] {
        let (p, s, mut b, mut c) = setup(42, start, epoch);
        c.set_camera(CameraInterest {
            center: Point {
                east: 1_000_000_000,
                north: -1_000_000_000,
            },
            radius_mm: 1_000_000_000,
        })
        .unwrap();
        for delta in (STEP..=1_000_000).step_by(STEP as usize) {
            advance(&mut b, &mut c, start + delta, 1);
            independent_counts(&p, &s, &b, &c, start + delta);
        }
        assert!(c.nodes().iter().all(|n| n.mode == Mode::L2));
        c.set_camera(wide()).unwrap();
        advance(&mut b, &mut c, start + 1_200_000, 1);
        assert!(c.nodes().iter().all(|n| n.mode == Mode::L1));
        independent_counts(&p, &s, &b, &c, start + 1_200_000);
        let hash = c.state_hash();
        assert_eq!(
            c.set_camera(CameraInterest {
                radius_mm: u64::MAX,
                ..wide()
            }),
            Err(LodError::Input)
        );
        assert_eq!(c.state_hash(), hash);
    }
}

#[test]
fn maximum_thirty_resident_cohort_retains_complete_membership_through_a_thousand_cycles() {
    let (p, s, mut b, mut c) = setup(71_449, 0, 0);
    assert_eq!(p.residents().len(), 30);
    let mut us = 0;
    for cycle in 0..1000 {
        c.set_camera(if cycle % 2 == 0 { overview() } else { wide() })
            .unwrap();
        for _ in 0..10 {
            us += STEP;
            advance(&mut b, &mut c, us, 1);
            let summary = independent_counts(&p, &s, &b, &c, us);
            assert_eq!(summary.l1 + summary.l2, 30);
        }
        let view = c.view(&b, t(us)).unwrap();
        assert_eq!(view.residents.len(), 30);
        assert!(
            view.residents
                .iter()
                .map(|r| r.resident)
                .eq(p.residents().ids().iter().copied())
        );
    }
}

#[test]
fn a_backward_quiet_boundary_before_the_last_mode_change_rejects_without_publication() {
    let (_p, _s, mut b, mut c) = setup(42, 0, 0);
    c.set_camera(wide()).unwrap();
    advance(&mut b, &mut c, 1_000_000, 1);
    let before = (b.state_hash(), c.state_hash());
    let mut bp = b.prepare(t(900_000)).unwrap();
    assert!(matches!(c.prepare(&mut bp, 1), Err(LodError::Invariant)));
    drop(bp);
    assert_eq!((b.state_hash(), c.state_hash()), before);
}
