//! ADR0027 vocabulary validation and explicit digest fields.
use ws_ecs::EntityId;
use ws_events::*;
use ws_time::SimTime;
fn fact() -> Candidate {
    let r = EntityId::procedural(1);
    let d = EntityId::procedural(2);
    let mut c = Candidate::new(
        SimTime::EPOCH,
        60,
        0,
        Payload::ScheduleChanged {
            resident: r,
            destination: d,
            previous: Activity::Home,
            next: Activity::Commute,
        },
        Origin::System(60),
    );
    c.lod = 2;
    c.actors
        .push(Actor {
            id: r,
            role: Role::Subject,
        })
        .unwrap();
    c.actors
        .push(Actor {
            id: d,
            role: Role::Object,
        })
        .unwrap();
    c
}
fn store() -> EventStore {
    EventStore::new(vec![EntityId::procedural(1), EntityId::procedural(2)])
}
#[test]
fn new_facts_validate_producer_participants_and_activity_and_hash_explicit_words() {
    let original = fact();
    let mut s = store();
    s.prepare(1, &[original]).unwrap().commit();
    let mut words = Vec::new();
    s.records()[0].visit_words(|w| words.extend_from_slice(w));
    assert_eq!(words[7], 7);
    assert_eq!(
        &words[13..17],
        &[
            EntityId::procedural(1).raw(),
            EntityId::procedural(2).raw(),
            5,
            1
        ]
    );
    assert_eq!(original.payload.name(), "agent.schedule_changed");
    for mutation in 0..7 {
        let mut c = original;
        match mutation {
            0 => c.system = 40,
            1 => c.lod = 1,
            2 => c.origin = Origin::System(40),
            3 => c.actors = Small::default(),
            4 => {
                let Payload::ScheduleChanged {
                    resident,
                    destination,
                    ..
                } = c.payload
                else {
                    unreachable!()
                };
                c.payload = Payload::ScheduleChanged {
                    resident,
                    destination,
                    previous: Activity::Home,
                    next: Activity::Home,
                };
            }
            5 => {
                c.actors = Small::default();
                c.actors
                    .push(Actor {
                        id: EntityId::procedural(2),
                        role: Role::Subject,
                    })
                    .unwrap();
            }
            _ => {
                let Payload::ScheduleChanged {
                    resident,
                    previous,
                    next,
                    ..
                } = c.payload
                else {
                    unreachable!()
                };
                c.payload = Payload::ScheduleChanged {
                    resident,
                    destination: EntityId::procedural(99),
                    previous,
                    next,
                };
            }
        }
        let mut s = store();
        assert!(s.prepare(1, &[c]).is_err());
        assert!(s.records().is_empty());
    }
}
#[test]
fn slice_creation_has_fixed_fields_and_world_scope_without_false_participants() {
    let p = Payload::SliceCreated {
        seed: 42,
        dataset: 123,
        structural: 1,
        population: 2,
        schedules: 3,
        schedule_epoch: -100_000,
    };
    let c = Candidate::new(SimTime::EPOCH, 0, 0, p, Origin::System(0));
    let mut s = store();
    s.prepare(0, &[c]).unwrap().commit();
    let mut words = Vec::new();
    s.records()[0].visit_words(|w| words.extend_from_slice(w));
    assert_eq!(words[7], 8);
    assert_eq!(&words[13..19], &[42, 123, 1, 2, 3, (-100_000i64) as u64]);
    let before = s.digest();
    let mut bad = c;
    bad.lod = 2;
    assert!(s.prepare(1, &[bad]).is_err());
    assert_eq!(s.digest(), before);
}
