//! Finite illness, continuous routines and explicit causal regression tests.
use ws_life::{
    Activity, Location,
    behavior::{Action, Detail},
};
use ws_sim::ws_events::{Confidence, Payload, Relation, Target};
use ws_sim::{
    SimTime,
    slice::{Operation, SliceConfig, SliceError, SliceSimulation, illness::Rejection},
};
fn world(start: i64) -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(start),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn apply(w: &mut SliceSimulation, id: ws_sim::EntityId, duration_us: u64) {
    assert_eq!(
        w.apply(Operation::Afflict {
            resident: id,
            severity: 5,
            duration_us
        })
        .unwrap()
        .error,
        None
    );
}
#[test]
fn finite_health_duplicate_noop_recovery_and_reaffliction_replay() {
    let mut w = world(0);
    let id = w.population().residents().ids()[0];
    let original = *w.vitals().get(id).unwrap();
    apply(&mut w, id, 200_000);
    let root = ws_sim::ws_events::EventId(2);
    assert_eq!(w.vitals().get(id).unwrap().health(), 55);
    assert_eq!(w.resident(id).unwrap().action, Action::Rest);
    let hash = w.state_hash();
    apply(&mut w, id, 100_000);
    assert_eq!(w.state_hash(), hash);
    assert_eq!(
        w.consequences(ws_sim::ws_events::EventId(4))
            .unwrap()
            .1
            .len(),
        1
    );
    w.step().unwrap();
    assert_eq!(w.vitals().get(id).unwrap().health(), 55);
    w.step().unwrap();
    assert_eq!(*w.vitals().get(id).unwrap(), original);
    assert!(!w.illness_state(id).unwrap().active);
    let (_, events) = w.consequences(root).unwrap();
    assert_eq!(events.len(), 3);
    let recovered = events.last().unwrap();
    assert!(matches!(
        recovered.record.payload,
        Payload::Recovered { health: 100, .. }
    ));
    let cause = recovered.record.causes.iter().next().unwrap();
    assert_eq!(
        (cause.relation, cause.confidence),
        (Relation::CausedBy, Confidence::Direct)
    );
    apply(&mut w, id, 100_000);
    w.step().unwrap();
    let (mut replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
    assert_eq!(replay.operation_digest(), w.operation_digest());
    for _ in 0..100 {
        w.step().unwrap();
        replay.step().unwrap();
        assert_eq!(w.checkpoint(), replay.checkpoint());
    }
}
#[test]
fn semantic_rejections_leave_state_and_events_unchanged_and_allow_replay_suffix() {
    let mut w = world(0);
    let id = w.population().residents().ids()[0];
    let before = w.checkpoint();
    for (severity, duration, expected) in [
        (0, 100_000, Rejection::Severity),
        (11, 100_000, Rejection::Severity),
        (1, 0, Rejection::Duration),
        (1, 100_001, Rejection::Duration),
        (1, 86_400_100_000, Rejection::Duration),
    ] {
        let r = w
            .apply(Operation::Afflict {
                resident: id,
                severity,
                duration_us: duration,
            })
            .unwrap();
        assert_eq!(r.error, Some(SliceError::Rejected(expected)));
        assert_eq!(w.checkpoint(), before);
    }
    let unknown = w.population().world().buildings().ids()[0];
    assert_eq!(
        w.apply(Operation::Afflict {
            resident: unknown,
            severity: 1,
            duration_us: 100_000
        })
        .unwrap()
        .error,
        Some(SliceError::Rejected(Rejection::Target))
    );
    w.step().unwrap();
    apply(&mut w, id, 100_000);
    w.step().unwrap();
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
    assert_eq!(w.operation_digest(), replay.operation_digest());
    let mut extreme = world(i64::MAX - i64::MAX % 100_000);
    let id = extreme.population().residents().ids()[0];
    assert_eq!(
        extreme
            .apply(Operation::Afflict {
                resident: id,
                severity: 1,
                duration_us: 100_000
            })
            .unwrap()
            .error,
        Some(SliceError::Rejected(Rejection::Deadline))
    );
}
#[test]
fn interrupted_offmap_duty_returns_continuously_and_records_only_actual_facts() {
    let mut w = world(9 * 3_600_000_000);
    let id = *w
        .population()
        .residents()
        .ids()
        .iter()
        .find(|&&id| {
            matches!(
                w.resident(id).unwrap().logical.location,
                Location::OffMap(..)
            )
        })
        .unwrap();
    let original = w.resident(id).unwrap().logical;
    apply(&mut w, id, 3_600_000_000);
    let root = w.illness_state(id).unwrap().own_root;
    let routine = w.behavior().routine(id).unwrap().unwrap();
    let j = routine.homeward.unwrap();
    assert_eq!(
        j.departure,
        i128::from(w.clock().now().as_micros_since_epoch())
    );
    assert!(j.arrival - j.departure >= 1_800_000_000);
    let now = w.resident(id).unwrap();
    assert_eq!(now.logical.activity, Activity::Commute);
    assert!(matches!(
        original.activity,
        Activity::Work | Activity::School
    ));
    let steps =
        ((j.arrival - i128::from(w.clock().now().as_micros_since_epoch())) / 100_000) as u64;
    assert_eq!(w.apply(Operation::Steps(steps)).unwrap().error, None);
    assert_eq!(
        w.resident(id).unwrap().logical.location,
        Location::AtBuilding(w.population().residents().get(id).unwrap().home)
    );
    let (_, events) = w.consequences(ws_sim::ws_events::EventId(root)).unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e.record.payload, Payload::MissedDuty { .. }))
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.record.payload, Payload::ReturnedHome { .. }))
            .count(),
        1
    );
    assert_eq!(w.apply(Operation::Steps(18_000)).unwrap().error, None);
    assert_eq!(w.vitals().get(id).unwrap().health(), 100);
    // Recovered while the baseline remains off-map: actual person holds at home.
    assert_eq!(w.resident(id).unwrap().logical.activity, Activity::Home);
    assert_ne!(
        w.resident(id).unwrap().logical,
        w.schedules()
            .get(id)
            .unwrap()
            .at(w.schedules().epoch(), w.clock().now())
    );
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
}
#[test]
fn a_started_leg_is_preserved_and_recovered_pending_return_blocks_replacement() {
    let mut w = world(460 * 60_000_000);
    let id = *w
        .population()
        .residents()
        .ids()
        .iter()
        .find(|&&id| {
            matches!(
                w.resident(id).unwrap().logical.location,
                Location::InTransit { .. }
            )
        })
        .unwrap();
    let old = w.resident(id).unwrap();
    apply(&mut w, id, 100_000);
    assert_eq!(w.resident(id).unwrap().logical, old.logical);
    let root = w.illness_state(id).unwrap().own_root;
    w.step().unwrap();
    assert_eq!(w.vitals().get(id).unwrap().health(), 100);
    let hash = w.state_hash();
    apply(&mut w, id, 100_000);
    assert_eq!(hash, w.state_hash());
    assert_eq!(w.illness_state(id).unwrap().own_root, root);
    let j = w.behavior().routine(id).unwrap().unwrap().homeward.unwrap();
    let steps =
        ((j.arrival - i128::from(w.clock().now().as_micros_since_epoch())) / 100_000) as u64;
    assert_eq!(w.apply(Operation::Steps(steps)).unwrap().error, None);
    apply(&mut w, id, 100_000);
    assert_ne!(w.illness_state(id).unwrap().own_root, root);
}
#[test]
fn neighbour_is_an_actual_l1_same_building_social_visit_with_contributing_link() {
    let mut w = world(20 * 3_600_000_000);
    let ids = w.population().residents().ids().to_vec();
    let patient = *ids
        .iter()
        .find(|&&a| {
            ids.iter().any(|&b| {
                a != b
                    && w.population().residents().get(a).unwrap().home
                        == w.population().residents().get(b).unwrap().home
                    && w.population().residents().get(a).unwrap().household
                        != w.population().residents().get(b).unwrap().household
            })
        })
        .unwrap();
    for &id in &ids {
        w.apply(Operation::SetDetail {
            resident: id,
            level: Detail::L1,
        })
        .unwrap();
    }
    apply(&mut w, patient, 600_000_000);
    assert_eq!(w.apply(Operation::Steps(600)).unwrap().error, None);
    let root = w.illness_state(patient).unwrap().own_root;
    let (_, events) = w.consequences(ws_sim::ws_events::EventId(root)).unwrap();
    let event = events
        .iter()
        .find(|e| matches!(e.record.payload, Payload::NeighbourCheckIn { .. }))
        .expect("eligible L1 neighbour");
    let Payload::NeighbourCheckIn { visitor, .. } = event.record.payload else {
        unreachable!()
    };
    assert_eq!(w.resident(visitor).unwrap().action, Action::Socialize);
    let cause = event.record.causes.iter().next().unwrap();
    assert_eq!(
        (cause.relation, cause.confidence),
        (Relation::ContributedTo, Confidence::Contributing)
    );
    assert_eq!(
        w.events().root_intervention(event.id).unwrap().id,
        ws_sim::ws_events::EventId(root)
    );
    assert!(matches!(cause.target, Target::Id(_)));
    assert_eq!(w.apply(Operation::Steps(3000)).unwrap().error, None);
    assert!(w.behavior().routine(visitor).unwrap().is_none());
}
#[test]
fn consequence_index_reconstruction_and_long_history_queries_are_pure_and_bounded() {
    let mut w = world(0);
    let id = w.population().residents().ids()[0];
    for _ in 0..100 {
        apply(&mut w, id, 100_000);
        w.step().unwrap();
    }
    let roots: Vec<_> = w
        .events()
        .records()
        .iter()
        .filter(|e| matches!(e.record.payload, Payload::SliceIllnessApplied { .. }))
        .map(|e| e.id)
        .collect();
    let before = w.checkpoint();
    let digest = w.operation_digest();
    let expected: Vec<_> = roots
        .iter()
        .map(|r| {
            let (g, e) = w.consequences(*r).unwrap();
            assert!(e.len() <= 16);
            (*g, e.iter().map(|e| e.digest()).collect::<Vec<_>>())
        })
        .collect();
    w.rebuild_consequence_index().unwrap();
    assert_eq!(before, w.checkpoint());
    assert_eq!(digest, w.operation_digest());
    for (root, (group, events)) in roots.iter().zip(expected) {
        let (g, e) = w.consequences(*root).unwrap();
        assert_eq!(*g, group);
        assert_eq!(e.iter().map(|e| e.digest()).collect::<Vec<_>>(), events);
    }
    assert!(
        w.consequences(ws_sim::ws_events::EventId(u64::MAX))
            .is_err()
    );
    assert_eq!(before, w.checkpoint());
}
#[test]
fn l2_does_not_invent_a_neighbour_visit() {
    let mut w = world(20 * 3_600_000_000);
    let id = w.population().residents().ids()[0];
    apply(&mut w, id, 120_000_000);
    assert_eq!(w.apply(Operation::Steps(1200)).unwrap().error, None);
    assert!(
        !w.events()
            .records()
            .iter()
            .any(|e| matches!(e.record.payload, Payload::NeighbourCheckIn { .. }))
    );
}

fn process_trace() -> String {
    let mut w = world(20 * 3_600_000_000);
    let id = w.population().residents().ids()[0];
    apply(&mut w, id, 120_000_000);
    let mut trace = String::new();
    for (i, scale) in [
        ws_sim::TimeScale::X100,
        ws_sim::TimeScale::X1,
        ws_sim::TimeScale::Paused,
        ws_sim::TimeScale::X10,
    ]
    .into_iter()
    .enumerate()
    {
        w.apply(Operation::SetScale(scale)).unwrap();
        w.apply(Operation::HostElapsed(100_000)).unwrap();
        for _ in 0..400 {
            w.step().unwrap();
        }
        trace.push_str(&format!(
            "BOUNDARY {i} {:?} {}\n",
            w.checkpoint(),
            w.operation_digest()
        ));
    }
    let (mut replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
    for i in 0..200 {
        w.step().unwrap();
        replay.step().unwrap();
        assert_eq!(w.checkpoint(), replay.checkpoint());
        trace.push_str(&format!("BOUNDARY {} {:?}\n", i + 4, w.checkpoint()));
    }
    trace
}
#[test]
fn illness_script_replays_with_future_trajectory_in_separate_processes() {
    const CHILD: &str = "WS_SLICE_ILLNESS_REPLAY_CHILD";
    if std::env::var_os(CHILD).is_some() {
        print!("{}", process_trace());
        return;
    }
    let expected: Vec<_> = process_trace().lines().map(str::to_owned).collect();
    for _ in 0..2 {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "illness_script_replays_with_future_trajectory_in_separate_processes",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let actual: Vec<_> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter(|s| s.starts_with("BOUNDARY "))
            .map(str::to_owned)
            .collect();
        assert_eq!(actual, expected);
    }
}
#[test]
fn splitting_step_batches_preserves_illness_events_and_normalized_command_sequence() {
    let mut a = world(0);
    let mut b = a.clone();
    let id = a.population().residents().ids()[0];
    for w in [&mut a, &mut b] {
        apply(w, id, 60_000_000);
    }
    assert_eq!(a.apply(Operation::Steps(1000)).unwrap().error, None);
    for _ in 0..1000 {
        b.step().unwrap();
    }
    apply(&mut a, id, 100_000);
    apply(&mut b, id, 100_000);
    assert_eq!(a.checkpoint(), b.checkpoint());
    assert_eq!(a.operation_digest(), b.operation_digest());
    assert_eq!(a.operations(), b.operations());
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config{cases:32,rng_seed:proptest::test_runner::RngSeed::Fixed(34),failure_persistence:None,..proptest::test_runner::Config::default()})]
    #[test]
    fn severity_and_grid_duration_properties_recover_at_the_exact_half_open_end(severity in 1u32..=10,ticks in 1u64..=30) {
        let mut w=world(-60_000_000);let id=w.population().residents().ids()[0];
        proptest::prop_assert_eq!(w.apply(Operation::Afflict{resident:id,severity,duration_us:ticks*100_000}).unwrap().error,None);
        proptest::prop_assert_eq!(w.vitals().get(id).unwrap().health(),(100-9*severity) as u8);
        for _ in 1..ticks {w.step().unwrap();proptest::prop_assert!(w.illness_state(id).unwrap().active);}
        w.step().unwrap();proptest::prop_assert_eq!(w.vitals().get(id).unwrap().health(),100);proptest::prop_assert!(!w.illness_state(id).unwrap().active);
    }
}

#[test]
fn an_already_homebound_journey_never_gets_a_fabricated_illness_return_link() {
    let initial = world(0);
    let (id, minute, arrival) =
        initial
            .schedules()
            .profiles()
            .iter()
            .find_map(|p| {
                let home = initial
                    .population()
                    .residents()
                    .get(p.resident)
                    .unwrap()
                    .home;
                p.weekday().iter().find_map(|r| {
                    (r.origin.is_some() && r.destination == ws_life::Place::Building(home))
                        .then_some((p.resident, r.minute, r.arrival_minute.unwrap_or(0)))
                })
            })
            .unwrap();
    let start = i64::from(minute) * 60_000_000 + 100_000;
    let mut w = world(start);
    let old = w.resident(id).unwrap().logical;
    apply(&mut w, id, 3_600_000_000);
    assert_eq!(w.resident(id).unwrap().logical, old);
    assert!(
        w.behavior()
            .routine(id)
            .unwrap()
            .unwrap()
            .homeward
            .is_none()
    );
    let steps = ((i64::from(arrival) * 60_000_000 - start) / 100_000) as u64;
    assert_eq!(w.apply(Operation::Steps(steps)).unwrap().error, None);
    let root = ws_sim::ws_events::EventId(w.illness_state(id).unwrap().own_root);
    assert!(
        !w.consequences(root)
            .unwrap()
            .1
            .iter()
            .any(|e| matches!(e.record.payload, Payload::ReturnedHome { .. }))
    );
}

#[test]
fn environmental_stop_replay_refuses_impossible_semantically_rejected_illness_commands() {
    use ws_sim::slice::{OperationRecord, ReplayError, Report};
    use ws_sim::ws_events::EventError;
    let w = world(0);
    let id = w.population().residents().ids()[0];
    let building = w.population().world().buildings().ids()[0];
    let cases = [
        (
            Operation::Afflict {
                resident: id,
                severity: 0,
                duration_us: 100_000,
            },
            Rejection::Severity,
        ),
        (
            Operation::Afflict {
                resident: id,
                severity: 11,
                duration_us: 100_000,
            },
            Rejection::Severity,
        ),
        (
            Operation::Afflict {
                resident: building,
                severity: 1,
                duration_us: 100_000,
            },
            Rejection::Target,
        ),
        (
            Operation::Afflict {
                resident: id,
                severity: 1,
                duration_us: 0,
            },
            Rejection::Duration,
        ),
        (
            Operation::Afflict {
                resident: id,
                severity: 1,
                duration_us: 100_001,
            },
            Rejection::Duration,
        ),
        (
            Operation::Afflict {
                resident: id,
                severity: 1,
                duration_us: 86_400_100_000,
            },
            Rejection::Duration,
        ),
    ];
    let check = |w: &SliceSimulation, operation, reason| {
        let mut actual = w.clone();
        assert_eq!(
            actual.apply(operation).unwrap().error,
            Some(SliceError::Rejected(reason))
        );
        assert_eq!(actual.checkpoint(), w.checkpoint());
        for error in [
            SliceError::History(EventError::Resource),
            SliceError::Behavior(ws_life::behavior::BehaviorError::Resource),
            SliceError::Lod(ws_life::lod::LodError::Resource),
        ] {
            let forged = OperationRecord {
                operation,
                report: Report {
                    committed: 0,
                    error: Some(error),
                },
                stop: Some(w.checkpoint()),
            };
            assert!(matches!(
                SliceSimulation::replay(w.config(), &[forged]),
                Err(ReplayError::Record(0))
            ));
        }
    };
    for (operation, reason) in cases {
        check(&w, operation, reason);
    }
    let extreme = world(i64::MAX - i64::MAX % 100_000);
    check(
        &extreme,
        Operation::Afflict {
            resident: id,
            severity: 1,
            duration_us: 100_000,
        },
        Rejection::Deadline,
    );
}
