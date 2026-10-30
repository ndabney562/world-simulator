//! ADR 0038 §3 storm deferral: departures the baseline timetable would start from a
//! featured building wait for the storm's end, then resume by the D3 rule.
use ws_ecs::EntityId;
use ws_life::{
    Activity, Answer, Location, Place, Schedules, Segment,
    behavior::{Behavior, Deferral, Detail, StormWindow},
};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;
const STEP: i64 = 100_000;
const MINUTE: i64 = 60_000_000;
const SEED: u64 = 7;
fn t(us: i64) -> SimTime {
    SimTime::from_micros_since_epoch(us)
}
fn inputs() -> (PopulatedSlice, Schedules) {
    let p = PopulatedSlice::new(SEED).unwrap();
    let s = Schedules::new(&p, t(0)).unwrap();
    (p, s)
}
fn world(start: i64) -> Behavior {
    let (p, s) = inputs();
    Behavior::new(&p, &s, t(start)).unwrap()
}
fn featured() -> Vec<EntityId> {
    inputs().0.world().buildings().ids().to_vec()
}
/// First Monday commute row (resident, row) departing a featured building, matching `want`.
fn row(want: impl Fn(&Segment) -> bool) -> (EntityId, Segment) {
    let (_, s) = inputs();
    let featured = featured();
    for p in s.profiles() {
        for r in p.weekday() {
            if let Some(Place::Building(b)) = r.origin
                && featured.contains(&b)
                && want(r)
            {
                return (p.resident, *r);
            }
        }
    }
    panic!("no matching row");
}
fn origin(r: &Segment) -> EntityId {
    match r.origin {
        Some(Place::Building(b)) => b,
        other => panic!("{other:?}"),
    }
}
fn baseline(id: EntityId, at: i64) -> Answer {
    let (_, s) = inputs();
    s.get(id).unwrap().at(t(0), t(at))
}
fn logical(b: &Behavior, id: EntityId, at: i64) -> Answer {
    b.common(id, t(at)).unwrap().logical
}
/// Casts a storm at `start` (already reached), as ws-sim will, on the prepared step.
fn cast(b: &mut Behavior, start: i64, end: i64) {
    let mut p = b.prepare(t(start)).unwrap();
    p.set_storm(Some(StormWindow {
        start: t(start),
        end: t(end),
    }));
    p.commit();
}
/// Prepares and commits each `step` through `to`, returning every listed deferral.
fn run(b: &mut Behavior, from: i64, to: i64, step: i64) -> Vec<(i64, Deferral)> {
    let mut out = Vec::new();
    let mut at = from;
    while at <= to {
        let p = b.prepare(t(at)).unwrap();
        out.extend(p.deferrals().iter().map(|d| (at, *d)));
        p.commit();
        at += step;
    }
    out
}
fn mine(all: &[(i64, Deferral)], id: EntityId) -> Vec<(i64, Deferral)> {
    all.iter()
        .copied()
        .filter(|(_, d)| d.resident == id)
        .collect()
}

#[test]
fn storm_defers_departure_from_featured_building() {
    let (id, r) = row(|r| matches!(r.destination, Place::OffMap(..)));
    let home = origin(&r);
    let d = i64::from(r.minute) * MINUTE;
    let mut b = world(d - 40 * MINUTE);
    b.advance_to(t(d - 30 * MINUTE)).unwrap();
    cast(&mut b, d - 30 * MINUTE, d + 30 * MINUTE);
    let all = run(&mut b, d - 29 * MINUTE, d + 10 * MINUTE, MINUTE);
    assert_eq!(
        mine(&all, id),
        [(
            d,
            Deferral {
                resident: id,
                building: home,
                activity: Activity::Home,
            }
        )]
    );
    assert!(matches!(
        baseline(id, d + 10 * MINUTE).location,
        Location::InTransit { .. }
    ));
    let now = logical(&b, id, d + 10 * MINUTE);
    assert_eq!(now.location, Location::AtBuilding(home));
    assert_eq!(now.activity, Activity::Home);
    let hold = b
        .resident_states()
        .find(|s| s.id == id)
        .unwrap()
        .storm_hold
        .unwrap();
    assert_eq!(hold.building, home);
    // Deferrals are listed per prepare in ascending resident order.
    for at in all.iter().map(|(at, _)| *at) {
        let ids: Vec<_> = all
            .iter()
            .filter(|(a, _)| *a == at)
            .map(|(_, d)| d.resident)
            .collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }
}

#[test]
fn storm_does_not_stop_residents_in_transit() {
    let (id, r) = row(|r| matches!(r.destination, Place::OffMap(..)));
    let d = i64::from(r.minute) * MINUTE;
    let mut b = world(d - 10 * MINUTE);
    b.advance_to(t(d + 5 * MINUTE)).unwrap();
    assert!(matches!(
        logical(&b, id, d + 5 * MINUTE).location,
        Location::InTransit { .. }
    ));
    cast(&mut b, d + 5 * MINUTE, d + 65 * MINUTE);
    let mut all = Vec::new();
    for m in [10, 20, 29, 30, 40] {
        all.extend(run(&mut b, d + m * MINUTE, d + m * MINUTE, MINUTE));
        assert_eq!(
            logical(&b, id, d + m * MINUTE),
            baseline(id, d + m * MINUTE),
            "minute {m}"
        );
    }
    assert!(mine(&all, id).is_empty());
    assert!(
        b.resident_states()
            .all(|s| s.id != id || s.storm_hold.is_none())
    );
}

#[test]
fn resident_departs_at_end_if_row_still_due() {
    let (id, r) = row(|r| matches!(r.destination, Place::OffMap(..)));
    let home = origin(&r);
    let d = i64::from(r.minute) * MINUTE;
    let length = (i64::from(r.arrival_minute.unwrap()) - i64::from(r.minute)) * MINUTE;
    // A non-minute end exercises the exact storm-end re-evaluation.
    let end = d + 10 * MINUTE + 5 * STEP;
    let mut b = world(d - 20 * MINUTE);
    b.advance_to(t(d - 10 * MINUTE)).unwrap();
    cast(&mut b, d - 10 * MINUTE, end);
    let all = run(&mut b, d - 9 * MINUTE, d + 10 * MINUTE, MINUTE);
    assert_eq!(mine(&all, id).len(), 1);
    assert_eq!(
        logical(&b, id, d + 10 * MINUTE).location,
        Location::AtBuilding(home)
    );
    b.advance_to(t(end)).unwrap();
    let journey = Location::InTransit {
        from: Place::Building(home),
        to: r.destination,
        departure: i128::from(end),
        arrival: i128::from(end + length),
    };
    let now = logical(&b, id, end);
    assert_eq!(now.location, journey);
    assert_eq!(now.activity, Activity::Commute);
    b.advance_to(t(end + length - STEP)).unwrap();
    assert_eq!(logical(&b, id, end + length - STEP).location, journey);
    // Arrival: the baseline is already at the destination, so it rejoins there.
    b.advance_to(t(end + length)).unwrap();
    assert_eq!(logical(&b, id, end + length), baseline(id, end + length));
    assert!(
        b.resident_states()
            .all(|s| s.id != id || s.storm_hold.is_none())
    );
}

#[test]
fn resident_rejoins_if_row_passed() {
    let (id, r) = row(|r| matches!(r.destination, Place::OffMap(..)));
    let home = origin(&r);
    let d = i64::from(r.minute) * MINUTE;
    let arrival = i64::from(r.arrival_minute.unwrap()) * MINUTE;
    let end = arrival + 10 * MINUTE;
    // Independent rejoin: first minute at or after the end found at home.
    let rejoin = (0..)
        .map(|m| end + m * MINUTE)
        .find(|&at| baseline(id, at).location == Location::AtBuilding(home))
        .unwrap();
    assert!(rejoin > end + 60 * MINUTE);
    let mut b = world(d - 20 * MINUTE);
    b.advance_to(t(d - 10 * MINUTE)).unwrap();
    cast(&mut b, d - 10 * MINUTE, end);
    let all = run(&mut b, d - 9 * MINUTE, end, MINUTE);
    assert_eq!(mine(&all, id).len(), 1);
    for at in [end, end + MINUTE, (end + rejoin) / 2, rejoin - MINUTE] {
        b.advance_to(t(at)).unwrap();
        let now = logical(&b, id, at);
        assert_eq!(now.location, Location::AtBuilding(home), "at {at}");
        assert_eq!(now.activity, Activity::Home);
    }
    b.advance_to(t(rejoin)).unwrap();
    assert_eq!(logical(&b, id, rejoin), baseline(id, rejoin));
    assert!(
        b.resident_states()
            .all(|s| s.id != id || s.storm_hold.is_none())
    );
    // Every hold has rejoined by late evening; the ended storm is then cleared.
    b.advance_to(t(21 * 60 * MINUTE)).unwrap();
    assert!(b.resident_states().all(|s| s.storm_hold.is_none()));
    assert_eq!(b.storm(), None);
}

#[test]
fn two_deferred_rows_count_twice() {
    // A resident whose two commute rows both start from the same featured building.
    let (_, s) = inputs();
    let featured = featured();
    let (id, first, second) = s
        .profiles()
        .iter()
        .find_map(|p| {
            let rows: Vec<_> = p.weekday().iter().filter(|r| r.origin.is_some()).collect();
            rows.windows(2).find_map(|w| {
                (w[0].origin == w[1].origin
                    && matches!(w[0].origin, Some(Place::Building(b)) if featured.contains(&b))
                    && w[1].minute - w[0].minute <= 50)
                    .then(|| (p.resident, *w[0], *w[1]))
            })
        })
        .expect("a resident with two rows from one building");
    let a = i64::from(first.minute) * MINUTE;
    let z = i64::from(second.minute) * MINUTE;
    let start = a - 5 * MINUTE;
    let mut b = world(start - 10 * MINUTE);
    b.advance_to(t(start)).unwrap();
    cast(&mut b, start, start + 60 * MINUTE);
    let all = run(&mut b, start + MINUTE, start + 60 * MINUTE, MINUTE);
    let ours = mine(&all, id);
    assert_eq!(ours.len(), 2, "{ours:?}");
    assert_eq!(ours[0].0, a);
    assert_eq!(ours[1].0, z);
    assert!(ours.iter().all(|(_, d)| d.building == origin(&first)));
}

#[test]
fn l1_and_l2_defer_identically() {
    let start = 7 * 60 * MINUTE;
    let end = 8 * 60 * MINUTE + 3 * STEP;
    let mut coarse = world(start - MINUTE);
    let mut fine = world(start - MINUTE);
    coarse.advance_to(t(start)).unwrap();
    fine.advance_to(t(start)).unwrap();
    let ids: Vec<_> = fine.residents().collect();
    for &id in &ids {
        fine.set_detail(id, Detail::L1, t(start)).unwrap();
    }
    cast(&mut coarse, start, end);
    cast(&mut fine, start, end);
    let mut deferred = 0;
    let mut at = start + MINUTE;
    while at <= 18 * 60 * MINUTE {
        let pc = coarse.prepare(t(at)).unwrap();
        let dc = pc.deferrals().to_vec();
        pc.commit();
        let pf = fine.prepare(t(at)).unwrap();
        assert_eq!(pf.deferrals(), dc.as_slice(), "at {at}");
        pf.commit();
        deferred += dc.len();
        for &id in &ids {
            let (c, f) = (
                coarse.common(id, t(at)).unwrap(),
                fine.snapshot(id, t(at)).unwrap(),
            );
            assert_eq!(
                (c.needs, c.action, c.decisions, c.logical),
                (f.needs, f.action, f.decisions, f.logical),
                "{id:?} at {at}"
            );
            // The coarse diagnostic projection follows the held logical answer too.
            assert_eq!(
                coarse.projected_location(id, t(at)).unwrap(),
                f.fine.unwrap(),
                "{id:?} at {at}"
            );
        }
        at += MINUTE;
    }
    assert!(deferred > 0);
    assert_eq!(coarse.storm(), None);
    assert_eq!(fine.storm(), None);
}

#[test]
fn no_storm_hash_unchanged() {
    // Pinned on base 9f490db, before storm state existed.
    let mut b = world(0);
    let mut out = vec![];
    for m in [0i64, 1, 450, 480, 1080, 1500, 3000] {
        b.advance_to(t(m * MINUTE)).unwrap();
        out.push(b.state_hash());
    }
    let id = b.residents().next().unwrap();
    b.set_detail(id, Detail::L1, t(3000 * MINUTE)).unwrap();
    out.push(b.state_hash());
    assert_eq!(
        out,
        [
            0x6948d1d09abfa361,
            0xb1a3f4496aac6116,
            0x64d9dfbb557ffc3c,
            0x050a367bbfaffcdf,
            0xb59d79919543362f,
            0xf915737925785f95,
            0xd042afe71fca97bc,
            0x8686864e1231eecc,
        ]
    );
    // A night storm defers nothing; once ended and cleared it leaves no hash trace.
    let mut quiet = world(0);
    let mut stormy = world(0);
    stormy.advance_to(t(60 * MINUTE)).unwrap();
    cast(&mut stormy, 60 * MINUTE, 120 * MINUTE);
    quiet.advance_to(t(90 * MINUTE)).unwrap();
    stormy.advance_to(t(90 * MINUTE)).unwrap();
    assert_ne!(quiet.state_hash(), stormy.state_hash());
    quiet.advance_to(t(121 * MINUTE)).unwrap();
    stormy.advance_to(t(121 * MINUTE)).unwrap();
    assert_eq!(stormy.storm(), None);
    assert_eq!(quiet.state_hash(), stormy.state_hash());
}

#[test]
fn saved_holds_restore_and_continue_identically() {
    let start = 7 * 60 * MINUTE;
    let end = 8 * 60 * MINUTE + 3 * STEP;
    let mut live = world(start - MINUTE);
    live.advance_to(t(start)).unwrap();
    cast(&mut live, start, end);
    let mid = 7 * 60 * MINUTE + 50 * MINUTE;
    live.advance_to(t(mid)).unwrap();
    let saved: Vec<_> = live.resident_states().collect();
    assert!(saved.iter().any(|s| s.storm_hold.is_some()));
    let (p, s) = inputs();
    // A hold without its storm window is not a valid saved state.
    assert!(Behavior::from_saved_states(&p, &s, &saved, t(mid), None).is_err());
    let mut restored = Behavior::from_saved_states(&p, &s, &saved, t(mid), live.storm()).unwrap();
    assert_eq!(restored.state_hash(), live.state_hash());
    for id in live.residents().collect::<Vec<_>>() {
        assert_eq!(
            restored.common(id, t(mid)).unwrap(),
            live.common(id, t(mid)).unwrap()
        );
    }
    for at in [end, end + 15 * MINUTE, 18 * 60 * MINUTE] {
        live.advance_to(t(at)).unwrap();
        restored.advance_to(t(at)).unwrap();
        assert_eq!(restored.state_hash(), live.state_hash(), "at {at}");
    }
}

#[test]
fn overrides_are_never_deferred() {
    let (id, r) = row(|r| matches!(r.destination, Place::OffMap(..)));
    let home = origin(&r);
    let d = i64::from(r.minute) * MINUTE;
    let mut b = world(d - 20 * MINUTE);
    b.advance_to(t(d - 10 * MINUTE)).unwrap();
    let mut p = b.prepare(t(d - 10 * MINUTE)).unwrap();
    p.set_storm(Some(StormWindow {
        start: t(d - 10 * MINUTE),
        end: t(d + 30 * MINUTE),
    }));
    p.visit(id, home, t(d + 20 * MINUTE), 9).unwrap();
    p.commit();
    let all = run(&mut b, d - 9 * MINUTE, d + 25 * MINUTE, MINUTE);
    assert!(mine(&all, id).is_empty());
    assert!(
        b.resident_states()
            .all(|s| s.id != id || s.storm_hold.is_none())
    );
}

#[test]
fn cancelling_a_storm_releases_holds_by_the_post_storm_rule() {
    let (id, r) = row(|r| matches!(r.destination, Place::OffMap(..)));
    let home = origin(&r);
    let d = i64::from(r.minute) * MINUTE;
    let length = (i64::from(r.arrival_minute.unwrap()) - i64::from(r.minute)) * MINUTE;
    let mut b = world(d - 20 * MINUTE);
    b.advance_to(t(d - 10 * MINUTE)).unwrap();
    cast(&mut b, d - 10 * MINUTE, d + 50 * MINUTE);
    // Cancel at the very instant of the deferral.
    let mut p = b.prepare(t(d)).unwrap();
    assert!(p.deferrals().iter().any(|x| x.resident == id));
    p.set_storm(None);
    p.commit();
    assert_eq!(logical(&b, id, d).location, Location::AtBuilding(home));
    let end = d + STEP;
    b.advance_to(t(end)).unwrap();
    assert_eq!(
        logical(&b, id, end).location,
        Location::InTransit {
            from: Place::Building(home),
            to: r.destination,
            departure: i128::from(end),
            arrival: i128::from(end + length),
        }
    );
    b.advance_to(t(end + length)).unwrap();
    assert!(b.resident_states().all(|s| s.storm_hold.is_none()));
    assert_eq!(b.storm(), None);
}
