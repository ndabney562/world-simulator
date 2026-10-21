//! Bounded actor facts agree with the existing full-query oracle, including backdating.
use ws_ecs::EntityId;
use ws_events::*;
use ws_time::SimTime;
fn actor() -> EntityId {
    EntityId::spawned(1).unwrap()
}
fn candidate(n: u64, time: i64) -> Candidate {
    let mut c = Candidate::new(
        SimTime::from_micros_since_epoch(time),
        10,
        n,
        Payload::CommandApplied {
            sequence: n,
            kind: 0,
            effect: 0,
            demoted: 0,
            promoted: 0,
        },
        Origin::Command(n),
    );
    for role in [Role::Subject, Role::Object] {
        c.actors.push(Actor { id: actor(), role }).unwrap();
    }
    c
}
fn oracle(store: &EventStore, cutoff: i64) {
    let all = store.query(
        Some(actor()),
        SimTime::MIN,
        SimTime::from_micros_since_epoch(cutoff),
    );
    let recent = store
        .recent(actor(), SimTime::from_micros_since_epoch(cutoff))
        .unwrap();
    assert_eq!(
        recent.records,
        all.iter().rev().take(16).copied().collect::<Vec<_>>()
    );
    assert_eq!(recent.more, all.len() > 16);
    assert!(recent.records.capacity() <= 16);
}
#[test]
fn backdated_duplicate_roles_ties_cutoffs_and_reconstruction_preserve_history() {
    let mut store = EventStore::new(vec![actor()]);
    for n in 0..40 {
        let time = if n % 3 == 0 {
            1000 - n as i64
        } else {
            n as i64 / 2
        };
        store.prepare(n, &[candidate(n, time)]).unwrap().commit();
    }
    for cutoff in [-1, 0, 1, 7, 8, 15, 16, 17, 999, 1000] {
        oracle(&store, cutoff);
    }
    let rebuilt = EventStore::from_records(vec![actor()], store.records()).unwrap();
    assert_eq!(rebuilt.digest(), store.digest());
    for cutoff in [7, 999, 1000] {
        assert_eq!(
            rebuilt
                .recent(actor(), SimTime::from_micros_since_epoch(cutoff))
                .unwrap(),
            store
                .recent(actor(), SimTime::from_micros_since_epoch(cutoff))
                .unwrap()
        );
    }
    let before = store.clone();
    drop(store.prepare(50, &[candidate(50, -100)]).unwrap());
    assert_eq!(store, before);
    let mut invalid = candidate(51, 2000);
    invalid
        .actors
        .push(Actor {
            id: EntityId::spawned(999).unwrap(),
            role: Role::Object,
        })
        .unwrap();
    assert!(store.prepare(51, &[invalid]).is_err());
    assert_eq!(store, before);
    assert!(
        store
            .recent(EntityId::spawned(99).unwrap(), SimTime::MAX)
            .unwrap()
            .records
            .is_empty()
    );
}
#[test]
fn bounded_empty_sixteen_seventeen_and_long_history_cutoffs() {
    let mut store = EventStore::new(vec![actor()]);
    oracle(&store, 0);
    for n in 0..10000 {
        store
            .prepare(n, &[candidate(n, n as i64)])
            .unwrap()
            .commit();
    }
    for cutoff in [0, 15, 16, 4999, 9999] {
        oracle(&store, cutoff);
    }
    let before = store.digest();
    for _ in 0..100 {
        oracle(&store, 100);
    }
    assert_eq!(store.digest(), before);
}
#[test]
fn decreasing_times_within_one_batch_are_ordered_by_time_not_commit_order() {
    let mut store = EventStore::new(vec![actor()]);
    store
        .prepare(
            0,
            &[
                candidate(1, 30),
                candidate(2, 20),
                candidate(3, 20),
                candidate(4, 10),
            ],
        )
        .unwrap()
        .commit();
    let got = store.recent(actor(), SimTime::MAX).unwrap();
    assert_eq!(
        got.records.iter().map(|e| e.id.0).collect::<Vec<_>>(),
        [1, 3, 2, 4]
    );
    assert_eq!(store.records().len(), 4);
    oracle(&store, 20);
}
