//! Independent policy goldens plus graph, utility, deadline and conservation invariants.
use proptest::prelude::*;
use std::fmt::Write;
use ws_ecs::EntityId;
use ws_life::{
    Activity, Location, Place, Schedules,
    behavior::{Action, Behavior, BehaviorError, Detail, FineLocation, Needs},
    navigation::{Graph, NavError, Navigation, Point},
};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;
const STEP: i64 = 100_000;
const MINUTE: i64 = 60_000_000;
const DAY: i64 = 86_400_000_000;
fn t(us: i64) -> SimTime {
    SimTime::from_micros_since_epoch(us)
}
fn body(seed: u64, start: i64, epoch: i64) -> Behavior {
    let p = PopulatedSlice::new(seed).unwrap();
    let s = Schedules::new(&p, t(epoch)).unwrap();
    Behavior::new(&p, &s, t(start)).unwrap()
}
fn place_words(p: Place) -> (u8, EntityId, u8) {
    match p {
        Place::Building(id) => (0, id, 255),
        Place::OffMap(id, e) => (1, id, e),
    }
}
#[test]
fn independent_new_graph_paths_initial_needs_and_tie_goldens_match() {
    let mut out = String::from("navigation_rules 1 behavior_rules 1\n");
    for seed in [0, 42, u64::MAX] {
        let p = PopulatedSlice::new(seed).unwrap();
        let nav = Navigation::new(&p).unwrap();
        let g = nav.graph();
        let o = nav.origin();
        let b = nav.bounds();
        writeln!(
            out,
            "world {seed:016x} origin {} {} bounds {} {} {} {}",
            o.lon_pdeg, o.lat_pdeg, b[0], b[1], b[2], b[3]
        )
        .unwrap();
        for (i, p) in g.points().iter().enumerate() {
            writeln!(out, "node {i} {} {}", p.east, p.north).unwrap();
        }
        for &(a, b) in g.edges() {
            let (p, q) = (g.points()[a], g.points()[b]);
            writeln!(
                out,
                "edge {a} {b} {}",
                p.east.abs_diff(q.east) + p.north.abs_diff(q.north)
            )
            .unwrap();
        }
        for &(place, node) in nav.places() {
            let (kind, id, exit) = place_words(place);
            writeln!(out, "place {kind} {:016x} {exit} {node}", id.raw()).unwrap();
        }
        for a in 0..g.points().len() {
            for b in 0..g.points().len() {
                let path = g.shortest(a, b).unwrap();
                let nodes = path
                    .nodes()
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                writeln!(out, "path {a} {b} {} {nodes}", path.distance_mm()).unwrap();
            }
        }
        for &id in p.residents().ids() {
            let n = Needs::initial(seed, id).values();
            let zero = Needs::checked([0; 4]).unwrap();
            let ties = [0, 1, u64::MAX].map(|counter| {
                ws_life::behavior::choose(seed, id, counter, Activity::Home, zero) as u8
            });
            writeln!(
                out,
                "needs {seed:016x} {:016x} {} {} {} {} {} {} {}",
                id.raw(),
                n[0],
                n[1],
                n[2],
                n[3],
                ties[0],
                ties[1],
                ties[2]
            )
            .unwrap();
        }
    }
    assert_eq!(out, include_str!("golden/behavior.txt"));
}
#[test]
fn positive_paths_stay_on_padded_envelope_and_fit_every_commute_budget() {
    for seed in [0, 42, u64::MAX] {
        let p = PopulatedSlice::new(seed).unwrap();
        let nav = Navigation::new(&p).unwrap();
        let b = nav.bounds();
        let half = (b[1] - b[0] + b[3] - b[2]) as u64;
        assert_eq!(half, 362_435);
        for &(a, c) in nav.graph().edges() {
            let (p, q) = (nav.graph().points()[a], nav.graph().points()[c]);
            assert!(p.east == q.east || p.north == q.north);
            assert_ne!(p, q);
        }
        for &(from, _) in nav.places() {
            for &(to, _) in nav.places() {
                let path = nav.route(from, to).unwrap();
                assert!(path.distance_mm() <= half);
                assert!(path.walking_ticks() <= 6000);
                for distance in [0, 140, path.distance_mm() / 2, path.distance_mm(), u64::MAX] {
                    let q = nav.position(from, to, distance).unwrap();
                    assert!(q.east == b[0] || q.east == b[1] || q.north == b[2] || q.north == b[3]);
                    assert!((b[0]..=b[1]).contains(&q.east) && (b[2]..=b[3]).contains(&q.north));
                }
                assert_eq!(
                    nav.position(from, to, 0).unwrap(),
                    nav.graph().points()[nav.node(from).unwrap()]
                );
                assert_eq!(
                    nav.position(from, to, u64::MAX).unwrap(),
                    nav.graph().points()[nav.node(to).unwrap()]
                );
            }
        }
        let hash = nav.fingerprint();
        assert_eq!(nav, Navigation::new(&p).unwrap());
        assert_eq!(nav.fingerprint(), hash);
        assert_eq!(
            nav.node(Place::Building(EntityId::procedural(0))),
            Err(NavError::Endpoint)
        );
    }
}
#[test]
fn shortest_path_uses_full_lexicographic_tie_and_rejects_invalid_or_disconnected_graphs() {
    let points = vec![
        Point { east: 0, north: 0 },
        Point { east: 0, north: 10 },
        Point { east: 10, north: 0 },
        Point {
            east: 10,
            north: 10,
        },
    ];
    let g = Graph::new(points.clone(), vec![(0, 1), (0, 2), (1, 3), (2, 3)]).unwrap();
    assert_eq!(g.shortest(0, 3).unwrap().nodes(), &[0, 1, 3]);
    assert_eq!(g.shortest(3, 0).unwrap().nodes(), &[3, 1, 0]);
    assert_eq!(g.shortest(1, 1).unwrap().distance_mm(), 0);
    assert_eq!(g.shortest(10, 0), Err(NavError::Endpoint));
    assert_eq!(
        Graph::new(points.clone(), vec![(0, 1)])
            .unwrap()
            .shortest(0, 3),
        Err(NavError::Disconnected)
    );
    for edges in [
        vec![(0, 0)],
        vec![(0, 4)],
        vec![(0, 3)],
        vec![(1, 0)],
        vec![(0, 1), (0, 1)],
    ] {
        assert!(matches!(
            Graph::new(points.clone(), edges),
            Err(NavError::Geometry)
        ));
    }
    assert!(matches!(
        Graph::new(vec![points[0], points[0]], vec![]),
        Err(NavError::Geometry)
    ));
    assert!(matches!(
        Graph::new(
            vec![Point {
                east: i64::MIN,
                north: 0
            }],
            vec![]
        ),
        Err(NavError::Geometry)
    ));
}
#[test]
fn deficit_actions_clamp_and_distinct_home_needs_choose_different_actions() {
    use Action::*;
    assert!(Needs::checked([100_001, 0, 0, 0]).is_none());
    assert_eq!(
        Needs::checked([1, 100, 2, 3])
            .unwrap()
            .advanced(Eat, 10)
            .values(),
        [11, 0, 12, 23]
    );
    assert_eq!(
        Needs::checked([1; 4])
            .unwrap()
            .advanced(Eat, u64::MAX)
            .values(),
        [100_000, 0, 100_000, 100_000]
    );
    assert_eq!(
        Needs::checked([1; 4])
            .unwrap()
            .advanced(Sleep, u64::MAX)
            .values(),
        [0, 100_000, 100_000, 100_000]
    );
    let id = EntityId::procedural(1);
    for (needs, expected) in [
        ([100_000, 0, 0, 0], Rest),
        ([0, 100_000, 0, 0], Eat),
        ([0, 0, 100_000, 0], Socialize),
        ([0, 0, 0, 100_000], Duties),
    ] {
        assert_eq!(
            ws_life::behavior::choose(42, id, 0, Activity::Home, Needs::checked(needs).unwrap()),
            expected
        );
    }
    for (activity, expected) in [
        (Activity::Sleep, Sleep),
        (Activity::Commute, Travel),
        (Activity::Work, Duties),
        (Activity::School, Duties),
        (Activity::Errands, Duties),
    ] {
        for n in [[0; 4], [100_000; 4]] {
            assert_eq!(
                ws_life::behavior::choose(42, id, u64::MAX, activity, Needs::checked(n).unwrap()),
                expected
            );
        }
    }
    for tag in 0..=255 {
        assert_eq!(
            Detail::from_tag(tag).map(|d| d as u8),
            if tag == 1 || tag == 2 {
                Some(tag)
            } else {
                None
            }
        );
    }
    assert_ne!(ws_life::behavior::INITIAL.value(), ws_life::JITTER.value());
    assert_ne!(
        ws_life::behavior::INITIAL.value(),
        ws_life::behavior::TIE.value()
    );
}
#[test]
fn every_residents_week_has_exact_local_and_off_map_deadlines_and_return_positions() {
    for seed in [0, 42, u64::MAX] {
        let epoch = -123_400_000;
        let p = PopulatedSlice::new(seed).unwrap();
        let schedules = Schedules::new(&p, t(epoch)).unwrap();
        let b = Behavior::new(&p, &schedules, t(epoch - DAY)).unwrap();
        let mut outbound = 0;
        let mut returning = 0;
        let mut same = 0;
        for day in -1..7i64 {
            for profile in schedules.profiles() {
                let table = if day.rem_euclid(7) < 5 {
                    profile.weekday()
                } else {
                    profile.weekend()
                };
                for row in table {
                    if let (Some(from), Some(arrive)) = (row.origin, row.arrival_minute) {
                        let depart = epoch + day * DAY + i64::from(row.minute) * MINUTE;
                        let arrival = epoch + day * DAY + i64::from(arrive) * MINUTE;
                        let path = b.navigation().route(from, row.destination).unwrap();
                        let walking = path.walking_ticks() as i64 * STEP;
                        let endpoint = b
                            .navigation()
                            .position(row.destination, row.destination, 0)
                            .unwrap();
                        let at = b.projected_location(profile.resident, t(arrival)).unwrap();
                        match row.destination {
                            Place::Building(_) => assert_eq!(
                                at,
                                FineLocation::Local {
                                    point: endpoint,
                                    walking: false
                                }
                            ),
                            Place::OffMap(workplace, exit) => {
                                assert_eq!(at, FineLocation::OffMap { workplace, exit })
                            }
                        }
                        let start = if matches!(from, Place::OffMap(..)) {
                            arrival - walking
                        } else {
                            depart
                        };
                        for tick in [
                            0,
                            path.walking_ticks() / 2,
                            path.walking_ticks().saturating_sub(1),
                        ] {
                            if tick >= path.walking_ticks() {
                                continue;
                            }
                            let now = start + tick as i64 * STEP;
                            assert_eq!(
                                b.projected_location(profile.resident, t(now)).unwrap(),
                                FineLocation::Local {
                                    point: b
                                        .navigation()
                                        .position(from, row.destination, tick * 140)
                                        .unwrap(),
                                    walking: true
                                }
                            );
                        }
                        if let Place::OffMap(workplace, exit) = from {
                            returning += 1;
                            assert_eq!(
                                b.projected_location(profile.resident, t(start - STEP))
                                    .unwrap(),
                                FineLocation::OffMapTransit { workplace, exit }
                            );
                        }
                        if let Place::OffMap(workplace, exit) = row.destination {
                            outbound += 1;
                            assert_eq!(
                                b.projected_location(profile.resident, t(arrival - STEP))
                                    .unwrap(),
                                FineLocation::OffMapTransit { workplace, exit }
                            );
                        }
                        if from == row.destination {
                            same += 1;
                            assert_eq!(path.distance_mm(), 0);
                            assert_eq!(
                                b.projected_location(profile.resident, t(depart)).unwrap(),
                                FineLocation::Local {
                                    point: endpoint,
                                    walking: false
                                }
                            );
                        }
                    }
                }
            }
        }
        assert!(outbound > 0 && returning > 0); // Same-place coverage is seed-dependent (shop's own residents).
        if seed == 0 {
            assert!(same > 0);
        }
    }
}
#[test]
fn week_coarse_catch_up_matches_minute_reference_and_preserves_every_identity() {
    let epoch = -123_400_000;
    let start = epoch + STEP;
    let mut fine = body(42, start, epoch);
    let mut coarse = fine.clone();
    let ids = fine.residents().collect::<Vec<_>>();
    for &id in &ids {
        fine.set_detail(id, Detail::L1, t(start)).unwrap();
    }
    // Minute-by-minute advancement is independent of the one-call coarse drain.
    for minute in 1..=7 * 1440 {
        fine.advance_to(t(epoch + minute * MINUTE)).unwrap();
    }
    let end = epoch + 7 * DAY;
    coarse.advance_to(t(end)).unwrap();
    assert_eq!(coarse.residents().collect::<Vec<_>>(), ids);
    for id in ids {
        coarse.set_detail(id, Detail::L1, t(end)).unwrap();
        assert_eq!(fine.snapshot(id, t(end)), coarse.snapshot(id, t(end)));
        assert_eq!(coarse.snapshot(id, t(end)).unwrap().decisions, 1 + 7 * 1440);
    }
    assert_eq!(fine.state_hash(), coarse.state_hash());
}
#[test]
fn invalid_requests_and_uncommitted_times_leave_canonical_state_untouched() {
    let mut b = body(42, 0, 0);
    let id = b.residents().next().unwrap();
    let before = b.state_hash();
    assert_eq!(
        b.set_detail(EntityId::procedural(0), Detail::L1, t(0)),
        Err(BehaviorError::UnknownResident)
    );
    assert_eq!(
        b.set_detail(id, Detail::L1, t(MINUTE)),
        Err(BehaviorError::Time)
    );
    assert_eq!(b.snapshot(id, t(-STEP)), Err(BehaviorError::Time));
    assert_eq!(b.snapshot(id, t(1)), Err(BehaviorError::Grid));
    assert_eq!(b.state_hash(), before);
    for epoch in [i64::MIN / STEP * STEP, i64::MAX / STEP * STEP, 0] {
        let b = body(0, epoch, epoch);
        let id = b.residents().next().unwrap();
        for now in [i64::MIN / STEP * STEP, i64::MAX / STEP * STEP, 0] {
            assert!(b.projected_location(id, t(now)).is_ok());
        }
    }
}
#[test]
fn anchored_needs_match_independent_physical_tick_reference_across_activity_changes() {
    // Literal policy rates, integrated one physical tick at a time independently
    // of the production clamped interval formula and minute catch-up loop.
    let rates = [
        [1, 2, 1, 2],
        [-3, 2, 1, 2],
        [1, -10, 1, 2],
        [1, 2, -7, 2],
        [1, 2, 1, -6],
        [1, 2, 1, 2],
        [-7, 2, 1, 2],
    ];
    let start = 6 * 3_600_000_000 + STEP;
    let mut b = body(42, start, 0);
    let id = b.residents().next().unwrap();
    b.set_detail(id, Detail::L1, t(start)).unwrap();
    let mut expected = b.snapshot(id, t(start)).unwrap().needs.values();
    let mut old = b.snapshot(id, t(start)).unwrap().action;
    let mut seen = std::collections::BTreeSet::new();
    for tick in 1..=3 * 36_000 {
        for (v, delta) in expected.iter_mut().zip(rates[old as usize]) {
            *v = (i64::from(*v) + delta).clamp(0, 100_000) as u32;
        }
        let now = start + tick * STEP;
        b.advance_to(t(now)).unwrap();
        let snapshot = b.snapshot(id, t(now)).unwrap();
        assert_eq!(snapshot.needs.values(), expected);
        seen.insert(snapshot.logical.activity as u8);
        old = snapshot.action;
    }
    assert!(seen.len() >= 3);
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn held_action_integration_composes_without_losing_clamp_progress(v in prop::array::uniform4(0..=100_000u32),which in 0..7usize,a in 0..100_000u64,b in 0..100_000u64) {
        let action=[Action::Idle,Action::Rest,Action::Eat,Action::Socialize,Action::Duties,Action::Travel,Action::Sleep][which];let needs=Needs::checked(v).unwrap();
        prop_assert_eq!(needs.advanced(action,a+b),needs.advanced(action,a).advanced(action,b));
    }
    #[test]
    fn mixed_level_zero_time_round_trip_preserves_hash_needs_counters_and_exact_position(seed in any::<u64>(),minute in 0..180i64,tick in 0..600i64,shift in -10_000..10_000i64,slot in 0..30usize) {
        let epoch=shift*STEP;let mut b=body(seed,epoch,epoch);let now=epoch+minute*MINUTE+tick*STEP;b.advance_to(t(now)).unwrap();
        let ids=b.residents().collect::<Vec<_>>();for (i,&id) in ids.iter().enumerate() {if i%2==0 {b.set_detail(id,Detail::L1,t(now)).unwrap();}}
        let id=ids[slot%ids.len()];b.set_detail(id,Detail::L1,t(now)).unwrap();let before=b.snapshot(id,t(now)).unwrap();let hash=b.state_hash();
        b.set_detail(id,Detail::L2,t(now)).unwrap();b.set_detail(id,Detail::L1,t(now)).unwrap();
        prop_assert_eq!(b.state_hash(),hash);prop_assert_eq!(b.snapshot(id,t(now)).unwrap(),before);prop_assert_eq!(b.residents().collect::<Vec<_>>(),ids);
    }
}

/// A resident at home at 03:00, a distinct target building and the walk between them.
fn gathering_setup() -> (Behavior, EntityId, EntityId, EntityId, i64) {
    let start = 3 * 60 * MINUTE;
    let b = body(7, start, 0);
    for id in b.residents().collect::<Vec<_>>() {
        let Location::AtBuilding(home) = b.snapshot(id, t(start)).unwrap().logical.location else {
            continue;
        };
        for &(place, _) in b.navigation().places() {
            if let Place::Building(target) = place {
                let ticks = b
                    .navigation()
                    .route(Place::Building(home), place)
                    .unwrap()
                    .walking_ticks() as i64;
                if target != home && ticks > 0 {
                    return (b, id, home, target, ticks * STEP);
                }
            }
        }
    }
    panic!("no resident home with a distinct target");
}
#[test]
fn gathering_override_walks_stays_and_returns() {
    let (mut b, id, home, target, walk) = gathering_setup();
    let start = 3 * 60 * MINUTE;
    let until = start + walk + 10 * MINUTE;
    let mut p = b.prepare(t(start)).unwrap();
    let r = p.gather(id, home, target, t(until), 9).unwrap();
    p.commit();
    assert_eq!(r.kind, 3);
    let (out, back) = (r.original.unwrap(), r.homeward.unwrap());
    assert_eq!(out.to, Place::Building(target));
    assert_eq!(
        (out.departure, out.arrival),
        (start.into(), (start + walk).into())
    );
    assert_eq!(
        (back.departure, back.arrival),
        (until.into(), (until + walk).into())
    );
    let at = |b: &mut Behavior, us: i64| {
        b.advance_to(t(us)).unwrap();
        let s = b.snapshot(id, t(us)).unwrap();
        (
            s.logical.activity,
            s.logical.location,
            s.action,
            b.routine(id).unwrap(),
        )
    };
    let (a, l, act, _) = at(&mut b, start + MINUTE);
    assert_eq!((a, act), (Activity::Commute, Action::Travel));
    assert!(matches!(l, Location::InTransit { to: Place::Building(x), .. } if x == target));
    let (a, l, act, _) = at(&mut b, start + walk + MINUTE);
    assert_eq!(
        (a, l, act),
        (
            Activity::Home,
            Location::AtBuilding(target),
            Action::Socialize
        )
    );
    let (a, l, act, _) = at(&mut b, until - MINUTE);
    assert_eq!(
        (a, l, act),
        (
            Activity::Home,
            Location::AtBuilding(target),
            Action::Socialize
        )
    );
    let (a, l, act, _) = at(&mut b, until + MINUTE);
    assert_eq!((a, act), (Activity::Commute, Action::Travel));
    assert!(matches!(l, Location::InTransit { to: Place::Building(x), .. } if x == home));
    let home_at = until + walk;
    let rejoin = r.rejoin.expect("baseline returns home within the horizon");
    let rejoin_us = rejoin.as_micros_since_epoch();
    assert!(rejoin_us >= home_at);
    if rejoin_us > home_at + MINUTE {
        // Held at home by the routine until the baseline rejoin.
        let (a, l, act, routine) = at(&mut b, home_at + MINUTE);
        assert_eq!(
            (a, l, act),
            (Activity::Home, Location::AtBuilding(home), Action::Rest)
        );
        assert!(routine.is_some());
    }
    let (_, l, _, routine) = at(&mut b, rejoin_us);
    assert_eq!((l, routine), (Location::AtBuilding(home), None));
}
#[test]
fn late_arrival_skips_the_stay() {
    let (mut b, id, home, target, walk) = gathering_setup();
    let start = 3 * 60 * MINUTE;
    let until = start + MINUTE;
    assert!(walk > MINUTE, "fixture walk must outlast the gathering");
    let mut p = b.prepare(t(start)).unwrap();
    let r = p.gather(id, home, target, t(until), 9).unwrap();
    p.commit();
    let (out, back) = (r.original.unwrap(), r.homeward.unwrap());
    assert_eq!(back.departure, out.arrival);
    for us in (start..start + 2 * walk + 5 * MINUTE).step_by(MINUTE as usize) {
        b.advance_to(t(us)).unwrap();
        assert_ne!(b.snapshot(id, t(us)).unwrap().action, Action::Socialize);
    }
}
#[test]
fn gathering_rejects_an_empty_stay() {
    let (mut b, id, home, target, _) = gathering_setup();
    let start = 3 * 60 * MINUTE;
    let mut p = b.prepare(t(start)).unwrap();
    assert_eq!(
        p.gather(id, home, target, t(start), 1).unwrap_err(),
        BehaviorError::Time
    );
}
#[test]
fn illness_and_visit_hashes_unchanged() {
    // The generalised answer must not alter kind 1/2 states: the pinned
    // intervention goldens in ws-sim/ws-persist cover whole-world hashes;
    // here, kind 1/2 never take the stay branch (no gap between journeys).
    let (mut b, id, home, _, _) = gathering_setup();
    let start = 3 * 60 * MINUTE;
    let mut p = b.prepare(t(start)).unwrap();
    p.visit(id, home, t(start + 10 * MINUTE), 3).unwrap();
    p.commit();
    for us in (start..start + 30 * MINUTE).step_by(MINUTE as usize) {
        let r = b.routine(id).unwrap();
        if let Some(r) = r {
            assert!(r.original.is_none() && r.homeward.is_none());
            let snap = b.snapshot(id, t(us)).unwrap();
            let want = if us < start + 10 * MINUTE {
                Action::Socialize
            } else {
                Action::Rest
            };
            assert_eq!(snap.action, want);
            assert_eq!(snap.logical.location, Location::AtBuilding(home));
        }
        b.advance_to(t(us + MINUTE)).unwrap();
    }
}
