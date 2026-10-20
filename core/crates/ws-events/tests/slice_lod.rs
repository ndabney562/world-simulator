//! Frozen ADR0029 tag9 field order, validation and independent digest golden.
use ws_ecs::EntityId;
use ws_events::*;
use ws_time::SimTime;
fn fact() -> Candidate {
    let building = EntityId::procedural(4);
    let mut c = Candidate::new(
        SimTime::from_micros_since_epoch(1_000_000),
        70,
        0,
        Payload::SliceLodChanged {
            building,
            from: 1,
            to: 3,
            demoted: 2,
            promoted: 0,
        },
        Origin::System(70),
    );
    c.lod = 3;
    c.actors
        .push(Actor {
            id: building,
            role: Role::Subject,
        })
        .unwrap();
    c
}
#[test]
fn tag9_exact_words_and_digest_are_frozen_without_changing_toy_tag2() {
    let c = fact();
    let mut s = EventStore::new(vec![EntityId::procedural(4)]);
    s.prepare(10, &[c]).unwrap().commit();
    let mut words = Vec::new();
    s.records()[0].visit_words(|v| words.extend_from_slice(v));
    let b = 0x4000_0000_0000_0001;
    assert_eq!(
        words,
        vec![
            1, 1, 10, 1, 1_000_000, 70, 0, 9, 16384, 0, 3, 0, 70, b, 1, 3, 2, 0, 1, b, 0, 0, 0, 0
        ]
    );
    // Independent Python SipHash over these explicit words; schema/rules v1.
    assert_eq!(s.records()[0].digest(), 0xe22c_d2ff_279b_8e06);
    assert_eq!(c.payload.name(), "sim.slice_lod_changed");
    let mut same = c;
    same.payload = Payload::SliceLodChanged {
        building: EntityId::procedural(4),
        from: 3,
        to: 3,
        demoted: 1,
        promoted: 0,
    };
    EventStore::new(vec![EntityId::procedural(4)])
        .prepare(10, &[same])
        .unwrap()
        .commit();
    let mut toy = c;
    toy.payload = Payload::LodChanged {
        building: EntityId::procedural(4),
        from: 1,
        to: 3,
        demoted: 2,
        promoted: 0,
    };
    toy.system = 20;
    toy.origin = Origin::System(20);
    assert_eq!(
        EventStore::new(vec![EntityId::procedural(4)])
            .prepare(10, &[toy])
            .err(),
        Some(EventError::Order)
    );
}
#[test]
fn malformed_node_modes_participants_producers_and_false_changes_leave_history_untouched() {
    for case in 0..10 {
        let mut c = fact();
        match case {
            0 => c.system = 20,
            1 => c.origin = Origin::Command(1),
            2 => c.lod = 2,
            3 => c.actors = Small::default(),
            4 => {
                c.actors = {
                    let mut a = Small::default();
                    a.push(Actor {
                        id: EntityId::procedural(4),
                        role: Role::Object,
                    })
                    .unwrap();
                    a
                }
            }
            5 => {
                c.payload = Payload::SliceLodChanged {
                    building: EntityId::procedural(4),
                    from: 0,
                    to: 3,
                    demoted: 2,
                    promoted: 0,
                }
            }
            6 => {
                c.payload = Payload::SliceLodChanged {
                    building: EntityId::procedural(4),
                    from: 1,
                    to: 4,
                    demoted: 2,
                    promoted: 0,
                }
            }
            7 => {
                c.payload = Payload::SliceLodChanged {
                    building: EntityId::procedural(4),
                    from: 3,
                    to: 3,
                    demoted: 0,
                    promoted: 0,
                }
            }
            8 => {
                c.payload = Payload::SliceLodChanged {
                    building: EntityId::procedural(8),
                    from: 1,
                    to: 3,
                    demoted: 2,
                    promoted: 0,
                }
            }
            _ => c
                .causes
                .push(Cause {
                    target: Target::Local(0),
                    relation: Relation::CausedBy,
                    confidence: Confidence::Direct,
                })
                .unwrap(),
        }
        let mut s = EventStore::new(vec![EntityId::procedural(4)]);
        let before = s.digest();
        assert!(s.prepare(10, &[c]).is_err(), "case {case}");
        assert!(s.records().is_empty());
        assert_eq!(s.digest(), before);
    }
}
