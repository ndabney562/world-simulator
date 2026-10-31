//! Kind 1 (illness) and kind 2 (visit) whole-state hashes pinned from the code
//! before kind 3 existed, so the generalised `answer` provably leaves them alone.
use ws_ecs::EntityId;
use ws_life::{
    Location, Schedules,
    behavior::{Behavior, Detail},
};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;
const MINUTE: i64 = 60_000_000;
fn t(us: i64) -> SimTime {
    SimTime::from_micros_since_epoch(us)
}
fn world(start: i64) -> Behavior {
    let p = PopulatedSlice::new(7).unwrap();
    let s = Schedules::new(&p, t(0)).unwrap();
    Behavior::new(&p, &s, t(start)).unwrap()
}
/// First (minute, resident) at or after `from` whose logical answer satisfies `want`.
fn find(from: i64, want: impl Fn(Location) -> bool) -> (i64, EntityId) {
    let mut b = world(from);
    let ids: Vec<_> = b.residents().collect();
    for m in 0..24 * 60 {
        let at = from + m * MINUTE;
        b.advance_to(t(at)).unwrap();
        for &id in &ids {
            if want(b.snapshot(id, t(at)).unwrap().logical.location) {
                return (at, id);
            }
        }
    }
    panic!("no matching resident");
}
/// Runs `act` at `at`, then returns hashes at several later minutes (coarse and L1).
fn hashes(at: i64, id: EntityId, act: impl Fn(&mut Behavior)) -> Vec<u64> {
    let mut b = world(at);
    b.advance_to(t(at)).unwrap();
    act(&mut b);
    assert!(b.routine(id).unwrap().is_some());
    let mut out = vec![b.state_hash()];
    for m in [1, 5, 30, 90, 600, 1500] {
        b.advance_to(t(at + m * MINUTE)).unwrap();
        out.push(b.state_hash());
    }
    b.set_detail(id, Detail::L1, t(at + 1500 * MINUTE)).unwrap();
    out.push(b.state_hash());
    out
}
#[test]
fn illness_and_visit_state_hashes_are_pinned() {
    let (at, id) = find(3 * 60 * MINUTE, |l| matches!(l, Location::AtBuilding(_)));
    let home = match world(at).snapshot(id, t(at)).unwrap().logical.location {
        Location::AtBuilding(h) => h,
        _ => unreachable!(),
    };
    let visit = hashes(at, id, |b| {
        let mut p = b.prepare(t(at)).unwrap();
        p.visit(id, home, t(at + 20 * MINUTE), 5).unwrap();
        p.commit();
    });
    let ill_home = hashes(at, id, |b| {
        let mut p = b.prepare(t(at)).unwrap();
        p.afflict(id, home, t(at + 240 * MINUTE), 6).unwrap();
        p.commit();
    });
    // Illness while walking: original and homeward are both present.
    let (tat, tid) = find(7 * 60 * MINUTE, |l| matches!(l, Location::InTransit { .. }));
    let ill_walk = hashes(tat, tid, |b| {
        let home = match b.snapshot(tid, t(tat)).unwrap().logical.location {
            Location::InTransit {
                to: ws_life::Place::Building(h),
                ..
            } => h,
            Location::InTransit {
                from: ws_life::Place::Building(h),
                ..
            } => h,
            other => panic!("{other:?}"),
        };
        let mut p = b.prepare(t(tat)).unwrap();
        let r = p.afflict(tid, home, t(tat + 240 * MINUTE), 7).unwrap();
        assert!(r.original.is_some() && r.homeward.is_some());
        p.commit();
    });
    // Computed on base 0fd02e2 (before kind 3).
    assert_eq!(
        visit,
        [
            0xefa70447713882f3,
            0x199dc473f45957f3,
            0xc98059181f130be5,
            0x77d1e1b2f355c326,
            0xb8359a87eb095bd3,
            0x2c0cafbb3526d24c,
            0x894dc7623b1eabca,
            0xb9b174c3767d8316
        ]
    );
    assert_eq!(
        ill_home,
        [
            0x246605d5190e2175,
            0xd8cc4f82e8e8ad2f,
            0x043b80344cfea6ee,
            0x78c7261ca879465a,
            0xabaee857e7077528,
            0x2c0cafbb3526d24c,
            0x894dc7623b1eabca,
            0xb9b174c3767d8316
        ]
    );
    assert_eq!(
        ill_walk,
        [
            0x5ac7dd39a538c9cd,
            0xbb4acf985390f0bf,
            0x917d16b74df6182d,
            0x59eff5047e16bda8,
            0xce79547453157b27,
            0x2f1d67ff0afabc26,
            0x41ef8a124da9129b,
            0x01ae3c3bdfacf0ac
        ]
    );
}
