//! Stable identities, causal validation, bounded queries and reconstruction.
use proptest::prelude::*;
use ws_ecs::EntityId;
use ws_events::*;
use ws_time::SimTime;
fn actor() -> EntityId {
    EntityId::spawned(1).unwrap()
}
fn candidate(n: u64) -> Candidate {
    let mut c = Candidate::new(
        SimTime::from_micros_since_epoch(n as i64),
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
    c.actors
        .push(Actor {
            id: actor(),
            role: Role::Subject,
        })
        .unwrap();
    c
}
#[test]
fn invalid_links_and_order_are_atomic() {
    let mut store = EventStore::new(vec![actor()]);
    let mut c = candidate(1);
    c.causes
        .push(Cause {
            target: Target::Local(0),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .unwrap();
    assert!(matches!(store.prepare(0, &[c]), Err(EventError::Cause)));
    assert!(store.records().is_empty());
    assert_eq!(store.counters().unwrap(), (1, 1));
    let mut future = candidate(0);
    future
        .causes
        .push(Cause {
            target: Target::Local(0),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .unwrap();
    assert!(matches!(
        store.prepare(0, &[candidate(1), future]),
        Err(EventError::Order)
    ));
    let mut unknown = candidate(1);
    unknown
        .actors
        .push(Actor {
            id: EntityId::spawned(2).unwrap(),
            role: Role::Object,
        })
        .unwrap();
    assert!(matches!(
        store.prepare(0, &[unknown]),
        Err(EventError::UnknownActor)
    ));
    let mut later = candidate(2);
    later.time = SimTime::EPOCH;
    later
        .causes
        .push(Cause {
            target: Target::Local(0),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .unwrap();
    assert!(matches!(
        store.prepare(0, &[candidate(1), later]),
        Err(EventError::Cause)
    ));
}
#[test]
fn same_transaction_causes_restore_indices_and_counters() {
    let mut store = EventStore::new(vec![actor()]);
    let mut c = candidate(2);
    c.causes
        .push(Cause {
            target: Target::Local(0),
            relation: Relation::CausedBy,
            confidence: Confidence::Direct,
        })
        .unwrap();
    store.prepare(0, &[candidate(1), c]).unwrap().commit();
    let rebuilt = EventStore::from_records(vec![actor()], store.records()).unwrap();
    assert_eq!(rebuilt, store);
    assert_eq!(rebuilt.counters().unwrap(), (3, 2));
    let walk = rebuilt.backward(EventId(2), 1, 2);
    assert_eq!(walk.ids, vec![EventId(2), EventId(1)]);
    assert!(!walk.truncated);
    assert!(rebuilt.backward(EventId(2), 0, 2).truncated);
    assert!(rebuilt.backward(EventId(2), 9, 1).truncated);
    let mut corrupt = store.records().to_vec();
    corrupt[1].id = EventId(99);
    assert_eq!(
        EventStore::from_records(vec![actor()], &corrupt),
        Err(EventError::Order)
    );
}
proptest! {
 #![proptest_config(ProptestConfig{cases:128,rng_seed:proptest::test_runner::RngSeed::Fixed(0x4556454e54),failure_persistence:None,..ProptestConfig::default()})]
 #[test]
 fn ids_queries_and_reconstruction(n in 0usize..128,lo in 0usize..128,hi in 0usize..128){
  let mut s=EventStore::new(vec![actor()]);for i in 0..n{let mut c=candidate(i as u64);if i>0 {c.causes.push(Cause {target:Target::Id(EventId(i as u64)),relation:Relation::CausedBy,confidence:Confidence::Direct}).unwrap();}c.actors.push(Actor{id:actor(),role:Role::Object}).unwrap();s.prepare(i as u64,&[c]).unwrap().commit();}
  for event in s.records(){for cause in event.record.causes.iter(){let Target::Id(id)=cause.target else {panic!("unresolved")};let target=s.get(id).unwrap();prop_assert!(target.id<event.id);prop_assert!(target.record.time<=event.record.time);}}
  prop_assert_eq!(s.records().iter().map(|e|e.id.0).collect::<Vec<_>>(),(1..=n as u64).collect::<Vec<_>>());
  let from=SimTime::from_micros_since_epoch(lo as i64);let to=SimTime::from_micros_since_epoch(hi as i64);let expected:Vec<_>=s.records().iter().filter(|e|e.record.time>=from&&e.record.time<=to).collect();prop_assert_eq!(s.query(Some(actor()),from,to),expected.clone());prop_assert_eq!(s.query(None,from,to),expected);prop_assert_eq!(EventStore::from_records(vec![actor()],s.records()).unwrap(),s);
 }
}

#[test]
fn multi_actor_queries_match_arena_after_dropped_preparation_and_rebuild() {
    let ids: Vec<_> = (1..=5).map(|n| EntityId::spawned(n).unwrap()).collect();
    // Canonical registration must not depend on caller order or duplicates.
    let mut store = EventStore::new(vec![ids[4], ids[2], ids[0], ids[1], ids[2], ids[3]]);
    for batch in 0..12 {
        let mut candidates = Vec::new();
        for i in 0..16 {
            let mut c = candidate((batch * 16 + i) as u64);
            // Occurrence order differs from publication/ID order, including ties
            // and negative times. Actor lists include non-adjacent duplicate roles.
            c.time = SimTime::from_micros_since_epoch(((i * 7 + batch) % 13) - 6);
            let a = ids[(i as usize + batch as usize) % 4];
            for (id, role) in [
                (a, Role::Subject),
                (ids[4], Role::Object),
                (a, Role::Object),
            ] {
                c.actors.push(Actor { id, role }).unwrap();
            }
            candidates.push(c);
        }
        let before = store.clone();
        drop(store.prepare(batch as u64, &candidates).unwrap());
        assert_eq!(store, before);
        store.prepare(batch as u64, &candidates).unwrap().commit();
    }
    let rebuilt = EventStore::from_records(ids.clone(), store.records()).unwrap();
    assert_eq!(rebuilt, store);
    for actor in ids
        .into_iter()
        .map(Some)
        .chain([None, Some(EntityId::spawned(999).unwrap())])
    {
        for (lo, hi) in [(-6, 6), (-2, 3), (0, 0), (7, 9), (3, -2)] {
            let from = SimTime::from_micros_since_epoch(lo);
            let to = SimTime::from_micros_since_epoch(hi);
            let mut expected: Vec<_> = store
                .records()
                .iter()
                .filter(|e| {
                    e.record.time >= from
                        && e.record.time <= to
                        && actor.is_none_or(|id| e.record.actors.iter().any(|a| a.id == id))
                })
                .collect();
            expected.sort_unstable_by_key(|e| (e.record.time, e.id));
            assert_eq!(store.query(actor, from, to), expected);
            assert_eq!(rebuilt.query(actor, from, to), expected);
        }
    }
    // Publication after reconstruction keeps IDs/counters/digests in sync.
    let c = candidate(999);
    store.prepare(12, &[c]).unwrap().commit();
    let mut rebuilt = rebuilt;
    rebuilt.prepare(12, &[c]).unwrap().commit();
    assert_eq!(store, rebuilt);
}
