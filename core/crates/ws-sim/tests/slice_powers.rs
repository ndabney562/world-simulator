//! ADR 0038 slice powers: bless, per-root consequence groups and their rebuild.
use ws_sim::ws_events::{Confidence, EventId, Payload, Relation, Target};
use ws_sim::{
    EntityId, SimTime,
    slice::{Operation, SliceConfig, SliceError, SliceSimulation, illness::Rejection},
};
use ws_world::slice::vitals::{Life, VitalState};

const MINUTE: u64 = 600;

fn world() -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::EPOCH,
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn ok(w: &mut SliceSimulation, op: Operation) {
    assert_eq!(w.apply(op).unwrap().error, None, "{op:?}");
}
fn last_root(w: &SliceSimulation) -> EventId {
    w.events()
        .records()
        .iter()
        .rev()
        .find(|e| e.record.payload.is_slice_root())
        .unwrap()
        .id
}
fn roots(w: &SliceSimulation) -> Vec<EventId> {
    w.events()
        .records()
        .iter()
        .filter(|e| e.record.payload.is_slice_root())
        .map(|e| e.id)
        .collect()
}
fn tags(w: &SliceSimulation, root: EventId) -> Vec<u32> {
    w.consequences(root)
        .unwrap()
        .1
        .iter()
        .map(|e| e.record.payload.tag())
        .collect()
}
fn baseline(w: &SliceSimulation, id: EntityId, t: SimTime) -> ws_life::Answer {
    w.schedules().get(id).unwrap().at(w.schedules().epoch(), t)
}
/// ADR 0034 rejoin: first instant at or after `from` where the baseline is at
/// home or departs home at that instant.
fn expected_rejoin(w: &SliceSimulation, id: EntityId, from: i128) -> Option<SimTime> {
    let home = w.population().residents().get(id).unwrap().home;
    let profile = w.schedules().get(id).unwrap();
    let epoch = w.schedules().epoch();
    let mut t = SimTime::from_micros_since_epoch(i64::try_from(from).unwrap());
    for _ in 0..128 {
        let a = profile.at(epoch, t);
        if a.location == ws_life::Location::AtBuilding(home)
            || matches!(a.location, ws_life::Location::InTransit { from, departure, .. }
                if from == ws_life::Place::Building(home)
                    && departure == i128::from(t.as_micros_since_epoch()))
        {
            return Some(t);
        }
        t = profile.next_change(epoch, t)?;
    }
    None
}
fn missed_after(w: &SliceSimulation, id: EntityId, t: SimTime) -> usize {
    w.events()
        .records()
        .iter()
        .filter(|e| {
            e.record.time > t
                && matches!(e.record.payload, Payload::MissedDuty { resident, .. } if resident == id)
        })
        .count()
}
fn snapshot(w: &SliceSimulation) -> Vec<(ws_sim::slice::illness::Group, Vec<u64>)> {
    roots(w)
        .into_iter()
        .map(|r| {
            let (g, e) = w.consequences(r).unwrap();
            (*g, e.iter().map(|e| e.digest()).collect())
        })
        .collect()
}

#[test]
fn bless_cures_once_and_illness_group_lacks_recovery() {
    let mut w = world();
    let id = w.population().residents().ids()[0];
    ok(
        &mut w,
        Operation::Afflict {
            resident: id,
            severity: 5,
            duration_us: 15 * 60_000_000,
        },
    );
    let illness_root = last_root(&w);
    let life_illness = w
        .events()
        .records()
        .iter()
        .find(|e| matches!(e.record.payload, Payload::LifeIllness { .. }))
        .unwrap()
        .id;
    ok(&mut w, Operation::Steps(10 * MINUTE));
    assert_eq!(w.vitals().get(id).unwrap().health(), 55);
    let routine = w.illness_state(id).unwrap();
    ok(&mut w, Operation::Bless { resident: id });
    let bless_root = last_root(&w);
    assert!(matches!(
        w.events().get(bless_root).unwrap().record.payload,
        Payload::BlessApplied { affected: 1, .. }
    ));
    assert_eq!(w.vitals().get(id).unwrap().health(), 100);
    let state = w.illness_state(id).unwrap();
    assert!(!state.active);
    assert_eq!(state.end, None);
    assert_eq!(state.own_root, illness_root.0);
    // Home at midnight with the baseline also at home: ADR 0034 rejoins at once.
    assert_eq!(
        (routine.override_kind, routine.override_root),
        (1, illness_root.0)
    );
    assert_eq!((state.override_kind, state.override_root), (0, 0));
    assert_eq!(
        w.resident(id).unwrap().logical,
        baseline(&w, id, w.clock().now())
    );
    // Step past the original end: the natural recovery must not fire again.
    ok(&mut w, Operation::Steps(6 * MINUTE));
    let recovered: Vec<_> = w
        .events()
        .records()
        .iter()
        .filter(|e| matches!(e.record.payload, Payload::Recovered { .. }))
        .collect();
    assert_eq!(recovered.len(), 1);
    let rec = recovered[0];
    let causes: Vec<_> = rec.record.causes.iter().copied().collect();
    assert_eq!(causes.len(), 2);
    assert_eq!(causes[0].target, Target::Id(bless_root));
    assert_eq!(
        (causes[0].relation, causes[0].confidence),
        (Relation::CausedBy, Confidence::Direct)
    );
    assert_eq!(causes[1].target, Target::Id(life_illness));
    assert_eq!(causes[1].relation, Relation::ContributedTo);
    assert_eq!(w.events().root_intervention(rec.id).unwrap().id, bless_root);
    let (g, _) = w.consequences(bless_root).unwrap();
    assert_eq!(g.ids(), &[bless_root, rec.id]);
    assert_eq!((g.subject, g.tag, g.severity), (Some(id), 14, 0));
    assert_eq!(g.end, Some(g.applied));
    assert!(!tags(&w, illness_root).contains(&12));
    assert_eq!(w.illness_state(id).unwrap().own_root, illness_root.0);
    let live = snapshot(&w);
    w.rebuild_consequence_index().unwrap();
    assert_eq!(snapshot(&w), live);
}

#[test]
fn bless_relieves_needs_or_is_noop() {
    let mut w = world();
    let id = w.population().residents().ids()[1];
    ok(&mut w, Operation::Steps(MINUTE));
    assert_ne!(w.resident(id).unwrap().needs.values(), [0; 4]);
    ok(&mut w, Operation::Bless { resident: id });
    let root = last_root(&w);
    assert!(matches!(
        w.events().get(root).unwrap().record.payload,
        Payload::BlessApplied { affected: 1, .. }
    ));
    assert_eq!(w.resident(id).unwrap().needs.values(), [0; 4]);
    assert_eq!(tags(&w, root), vec![14, 15]);
    let (g, events) = w.consequences(root).unwrap();
    assert_eq!((g.subject, g.tag, g.severity), (Some(id), 14, 0));
    assert!(matches!(
        events[1].record.payload,
        Payload::NeedsRelieved { resident } if resident == id
    ));
    let hash = w.state_hash();
    ok(&mut w, Operation::Bless { resident: id });
    let again = last_root(&w);
    assert!(matches!(
        w.events().get(again).unwrap().record.payload,
        Payload::BlessApplied { affected: 0, .. }
    ));
    assert_eq!(w.consequences(again).unwrap().0.ids(), &[again]);
    assert_eq!(w.state_hash(), hash);
    ok(&mut w, Operation::Steps(MINUTE));
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
    assert_eq!(replay.operation_digest(), w.operation_digest());
}

#[test]
fn bless_rejects_unknown_and_dead() {
    let mut w = world();
    let unknown = w.population().world().buildings().ids()[0];
    let before = w.checkpoint();
    let r = w.apply(Operation::Bless { resident: unknown }).unwrap();
    assert_eq!(r.error, Some(SliceError::Rejected(Rejection::Target)));
    assert_eq!(w.checkpoint(), before);
    assert_eq!(
        w.operations().last().unwrap().report.error,
        Some(SliceError::Rejected(Rejection::Target))
    );
    let id: EntityId = w.population().residents().ids()[2];
    let mut parts = w.to_parts();
    for (v_id, v) in &mut parts.vitals {
        if *v_id == id {
            *v = VitalState::checked(v.sex(), 0, Life::Dead).unwrap();
        }
    }
    let mut dead = SliceSimulation::from_parts(parts).unwrap();
    let before = dead.checkpoint();
    let events = dead.events().records().len();
    let r = dead.apply(Operation::Bless { resident: id }).unwrap();
    assert_eq!(r.error, Some(SliceError::Rejected(Rejection::Dead)));
    assert_eq!(dead.checkpoint(), before);
    assert_eq!(dead.events().records().len(), events);
    assert_eq!(
        dead.operations().last().unwrap().operation,
        Operation::Bless { resident: id }
    );
    // Rejections continue: a later valid operation still replays.
    let living = w.population().residents().ids()[0];
    ok(&mut w, Operation::Bless { resident: living });
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
    assert_eq!(replay.operation_digest(), w.operation_digest());
}

#[test]
fn rebuilt_index_equals_live_after_bless() {
    let mut w = world();
    let ids = w.population().residents().ids().to_vec();
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[0],
            severity: 3,
            duration_us: 15 * 60_000_000,
        },
    );
    ok(&mut w, Operation::Steps(10 * MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[0] });
    ok(&mut w, Operation::Bless { resident: ids[1] });
    ok(&mut w, Operation::Bless { resident: ids[1] });
    ok(&mut w, Operation::Steps(10 * MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[0] });
    ok(&mut w, Operation::Steps(MINUTE));
    assert_eq!(roots(&w).len(), 5);
    let live = snapshot(&w);
    let before = w.checkpoint();
    let digest = w.operation_digest();
    w.rebuild_consequence_index().unwrap();
    assert_eq!(snapshot(&w), live);
    assert_eq!(w.checkpoint(), before);
    assert_eq!(w.operation_digest(), digest);
}

#[test]
fn bless_keeps_pending_return_home_under_the_illness_root() {
    let mut w = SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(9 * 3_600_000_000),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap();
    let id = *w
        .population()
        .residents()
        .ids()
        .iter()
        .find(|&&id| {
            matches!(
                w.resident(id).unwrap().logical.location,
                ws_life::Location::OffMap(..)
            )
        })
        .unwrap();
    ok(
        &mut w,
        Operation::Afflict {
            resident: id,
            severity: 4,
            duration_us: 3_600_000_000,
        },
    );
    let illness_root = last_root(&w);
    let j = w.behavior().routine(id).unwrap().unwrap().homeward.unwrap();
    ok(&mut w, Operation::Steps(MINUTE));
    let routine = w.behavior().routine(id).unwrap().unwrap();
    ok(&mut w, Operation::Bless { resident: id });
    let bless_root = last_root(&w);
    // The homeward leg is kept; the hold ends now and rejoins by the ADR 0034 rule.
    let ended = w.behavior().routine(id).unwrap().unwrap();
    assert_eq!(ended.homeward, routine.homeward);
    assert_eq!(ended.original, routine.original);
    assert_eq!(ended.until, w.clock().now());
    assert_eq!(ended.rejoin, expected_rejoin(&w, id, j.arrival));
    let steps =
        ((j.arrival - i128::from(w.clock().now().as_micros_since_epoch())) / 100_000) as u64;
    ok(&mut w, Operation::Steps(steps));
    assert_eq!(tags(&w, bless_root), vec![14, 12]);
    let illness = tags(&w, illness_root);
    assert_eq!(illness.iter().filter(|t| **t == 6).count(), 1);
    assert!(!illness.contains(&12));
    let live = snapshot(&w);
    w.rebuild_consequence_index().unwrap();
    assert_eq!(snapshot(&w), live);
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
}

#[test]
fn environmental_stop_replay_runs_bless_preflight() {
    use ws_sim::slice::{OperationRecord, ReplayError, Report};
    use ws_sim::ws_events::EventError;
    let w = world();
    let id = w.population().residents().ids()[0];
    let building = w.population().world().buildings().ids()[0];
    let forged = |resident| OperationRecord {
        operation: Operation::Bless { resident },
        report: Report {
            committed: 0,
            error: Some(SliceError::History(EventError::Resource)),
        },
        stop: Some(w.checkpoint()),
    };
    assert!(matches!(
        SliceSimulation::replay(w.config(), &[forged(building)]),
        Err(ReplayError::Record(0))
    ));
    let (replay, stopped) = SliceSimulation::replay(w.config(), &[forged(id)]).unwrap();
    assert!(stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
}

/// ADR 0038 §2 with the ADR 0034 recovery rule: a cure ends the home hold now.
/// An off-map worker, cursed for 12 hours and blessed once home, rejoins at the
/// baseline's next home instant (the evening return), not after the original
/// end, and records no duty missed after the cure.
#[test]
fn bless_cure_rejoins_baseline_at_the_next_home_instant() {
    let mut w = SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(9 * 3_600_000_000),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap();
    let id = *w
        .population()
        .residents()
        .ids()
        .iter()
        .find(|&&id| {
            matches!(
                w.resident(id).unwrap().logical.location,
                ws_life::Location::OffMap(..)
            )
        })
        .unwrap();
    ok(
        &mut w,
        Operation::Afflict {
            resident: id,
            severity: 6,
            duration_us: 12 * 3_600_000_000,
        },
    );
    let illness_root = last_root(&w);
    let routine = w.behavior().routine(id).unwrap().unwrap();
    let arrival = routine.homeward.unwrap().arrival;
    let steps = ((arrival - i128::from(w.clock().now().as_micros_since_epoch())) / 100_000) as u64;
    ok(&mut w, Operation::Steps(steps + 10 * MINUTE));
    let home = w.population().residents().get(id).unwrap().home;
    assert_eq!(
        w.resident(id).unwrap().logical.location,
        ws_life::Location::AtBuilding(home)
    );
    ok(&mut w, Operation::Bless { resident: id });
    let cured = w.clock().now();
    let expected = expected_rejoin(&w, id, i128::from(cured.as_micros_since_epoch())).unwrap();
    assert!(expected > cured);
    // The original hold would have lasted at least until the 12-hour end.
    assert!(routine.rejoin.unwrap() > expected);
    let ended = w.behavior().routine(id).unwrap().unwrap();
    assert_eq!((ended.until, ended.rejoin), (cured, Some(expected)));
    let to_rejoin =
        ((expected.as_micros_since_epoch() - cured.as_micros_since_epoch()) / 100_000) as u64;
    ok(&mut w, Operation::Steps(to_rejoin - 1));
    assert!(w.behavior().routine(id).unwrap().is_some());
    ok(&mut w, Operation::Steps(1));
    assert_eq!(w.clock().now(), expected);
    assert_eq!(w.behavior().routine(id).unwrap(), None);
    assert_eq!(w.resident(id).unwrap().logical, baseline(&w, id, expected));
    ok(&mut w, Operation::Steps(60 * MINUTE));
    assert_eq!(missed_after(&w, id, cured), 0);
    assert!(!tags(&w, illness_root).contains(&12));
    let live = snapshot(&w);
    w.rebuild_consequence_index().unwrap();
    assert_eq!(snapshot(&w), live);
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
}

/// Save/restore after a relieve and after a cure: equal hashes, equal futures
/// and a rebuilt index equal to the live one.
#[test]
fn bless_survives_save_and_restore() {
    let mut w = world();
    let ids = w.population().residents().ids().to_vec();
    ok(&mut w, Operation::Steps(MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[1] });
    let check = |w: &mut SliceSimulation| {
        let mut restored = SliceSimulation::from_parts(w.to_parts()).unwrap();
        assert_eq!(restored.state_hash(), w.state_hash());
        assert_eq!(restored.checkpoint(), w.checkpoint());
        assert_eq!(snapshot(&restored), snapshot(w));
        ok(w, Operation::Steps(60 * MINUTE));
        ok(&mut restored, Operation::Steps(60 * MINUTE));
        assert_eq!(restored.state_hash(), w.state_hash());
        assert_eq!(restored.checkpoint(), w.checkpoint());
        assert_eq!(snapshot(&restored), snapshot(w));
    };
    check(&mut w);
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[0],
            severity: 5,
            duration_us: 2 * 3_600_000_000,
        },
    );
    ok(&mut w, Operation::Steps(10 * MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[0] });
    check(&mut w);
}

// ---- Storm (ADR 0038 §3) ----

const HOUR_US: u64 = 3_600_000_000;

/// Monday 07:30, so a one-hour storm spans the 08:00 departures.
fn storm_world() -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(7 * 3_600_000_000 + 30 * 60_000_000),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn stayed(w: &SliceSimulation) -> Vec<(EventId, EntityId, EntityId)> {
    w.events()
        .records()
        .iter()
        .filter_map(|e| match e.record.payload {
            Payload::StayedInside {
                resident, building, ..
            } => Some((e.id, resident, building)),
            _ => None,
        })
        .collect()
}
fn missed_duties(w: &SliceSimulation) -> Vec<(SimTime, Payload)> {
    w.events()
        .records()
        .iter()
        .filter(|e| matches!(e.record.payload, Payload::MissedDuty { .. }))
        .map(|e| (e.record.time, e.record.payload))
        .collect()
}

#[test]
fn storm_group_is_root_kept_ended() {
    let mut w = storm_world();
    let start = w.clock().now();
    let hash = w.state_hash();
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    let root = last_root(&w);
    let sequence = match w.events().get(root).unwrap().record.payload {
        Payload::StormApplied {
            sequence,
            duration_us,
        } => {
            assert_eq!(duration_us, HOUR_US);
            sequence
        }
        other => panic!("{other:?}"),
    };
    let end = SimTime::from_micros_since_epoch(start.as_micros_since_epoch() + HOUR_US as i64);
    let storm = *w.storm().unwrap();
    assert_eq!(
        (storm.root, storm.start, storm.end, storm.kept),
        (root, start, end, 0)
    );
    // An active storm enters the canonical hash.
    assert_ne!(w.state_hash(), hash);
    ok(&mut w, Operation::Steps(45 * MINUTE));
    let during = stayed(&w);
    assert!(!during.is_empty(), "the 08:00 departures are deferred");
    assert_eq!(w.storm().unwrap().kept as usize, during.len());
    assert_eq!(w.consequences(root).unwrap().0.ids(), &[root]);
    ok(&mut w, Operation::Steps(15 * MINUTE - 1));
    assert!(w.storm().is_some());
    ok(&mut w, Operation::Steps(1));
    assert_eq!(w.clock().now(), end);
    assert!(w.storm().is_none());
    let all = stayed(&w);
    // Canonical ascending resident order within each tick.
    for pair in all.windows(2) {
        let (a, b) = (
            w.events().get(pair[0].0).unwrap(),
            w.events().get(pair[1].0).unwrap(),
        );
        if a.record.time == b.record.time {
            assert!(pair[0].1 < pair[1].1);
        }
    }
    for (id, _, _) in &all {
        let e = w.events().get(*id).unwrap();
        let causes: Vec<_> = e.record.causes.iter().copied().collect();
        assert_eq!(causes.len(), 1);
        assert_eq!(causes[0].target, Target::Id(root));
        assert_eq!(causes[0].relation, Relation::ContributedTo);
        assert_eq!(w.events().root_intervention(*id).unwrap().id, root);
    }
    let (g, events) = w.consequences(root).unwrap();
    assert_eq!(g.ids().len(), 3);
    assert_eq!((g.subject, g.tag, g.severity), (None, 16, 0));
    assert_eq!((g.applied, g.end), (start, Some(end)));
    assert_eq!(tags(&w, root), vec![16, 18, 19]);
    assert_eq!(
        events[1].record.payload,
        Payload::StormKeptInside {
            count: all.len() as u32
        }
    );
    assert_eq!(events[2].record.payload, Payload::StormEnded { sequence });
    assert_eq!((events[1].record.time, events[2].record.time), (end, end));
    for e in &events[1..] {
        let causes: Vec<_> = e.record.causes.iter().copied().collect();
        assert_eq!(causes.len(), 1);
        assert_eq!(causes[0].target, Target::Id(root));
        assert_eq!(
            (causes[0].relation, causes[0].confidence),
            (Relation::CausedBy, Confidence::Direct)
        );
    }
    let ids = g.ids().to_vec();
    // No new StayedInside after the end.
    ok(&mut w, Operation::Steps(30 * MINUTE));
    assert_eq!(stayed(&w), all);
    assert_eq!(w.consequences(root).unwrap().0.ids(), ids);
}

#[test]
fn storm_rejected_while_active() {
    let mut w = storm_world();
    ok(
        &mut w,
        Operation::Storm {
            duration_us: 600_000_000,
        },
    );
    let root = last_root(&w);
    ok(&mut w, Operation::Steps(MINUTE));
    let before = w.checkpoint();
    let events = w.events().records().len();
    for op in [
        Operation::Storm {
            duration_us: HOUR_US,
        },
        Operation::Storm {
            duration_us: 100_000,
        },
    ] {
        let r = w.apply(op).unwrap();
        assert_eq!(
            r.error,
            Some(SliceError::Rejected(Rejection::Busy)),
            "{op:?}"
        );
        assert_eq!(w.checkpoint(), before);
        assert_eq!(w.events().records().len(), events);
        assert_eq!(w.operations().last().unwrap().operation, op);
    }
    assert_eq!(w.storm().unwrap().root, root);
    ok(&mut w, Operation::Steps(9 * MINUTE));
    assert!(w.storm().is_none());
    // Accepted again once the storm has ended.
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    assert_ne!(last_root(&w), root);
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
    assert_eq!(replay.operation_digest(), w.operation_digest());
}

#[test]
fn storm_duration_bounds() {
    for duration_us in [0, 99_999, 150_000, 3_600_000_001, 3_600_100_000, u64::MAX] {
        let mut w = storm_world();
        let before = w.checkpoint();
        let r = w.apply(Operation::Storm { duration_us }).unwrap();
        assert_eq!(
            r.error,
            Some(SliceError::Rejected(Rejection::Duration)),
            "{duration_us}"
        );
        assert_eq!(w.checkpoint(), before);
        assert!(w.storm().is_none());
    }
    let mut w = storm_world();
    ok(
        &mut w,
        Operation::Storm {
            duration_us: 100_000,
        },
    );
    let root = last_root(&w);
    ok(&mut w, Operation::Steps(1));
    assert!(w.storm().is_none());
    assert_eq!(tags(&w, root), vec![16, 18, 19]);
    assert_eq!(
        w.consequences(root).unwrap().1[1].record.payload,
        Payload::StormKeptInside { count: 0 }
    );
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    assert!(w.storm().is_some());
}

#[test]
fn storm_produces_no_missed_duty() {
    let base = storm_world();
    let now = base.clock().now();
    let later = SimTime::from_micros_since_epoch(now.as_micros_since_epoch() + 3 * HOUR_US as i64);
    // A resident at home now whose baseline is at work or school by 10:30.
    let ill = *base
        .population()
        .residents()
        .ids()
        .iter()
        .find(|&&id| {
            let home = base.population().residents().get(id).unwrap().home;
            base.resident(id).unwrap().logical.location == ws_life::Location::AtBuilding(home)
                && matches!(
                    baseline(&base, id, later).activity,
                    ws_life::Activity::Work | ws_life::Activity::School
                )
        })
        .unwrap();
    let run = |storm: bool| {
        let mut w = base.clone();
        ok(
            &mut w,
            Operation::Afflict {
                resident: ill,
                severity: 5,
                duration_us: 4 * HOUR_US,
            },
        );
        if storm {
            ok(
                &mut w,
                Operation::Storm {
                    duration_us: HOUR_US,
                },
            );
        }
        ok(&mut w, Operation::Steps(180 * MINUTE));
        w
    };
    let calm = run(false);
    let stormy = run(true);
    assert!(stayed(&calm).is_empty());
    assert!(!stayed(&stormy).is_empty());
    let missed = missed_duties(&stormy);
    assert!(
        missed
            .iter()
            .any(|(_, p)| matches!(p, Payload::MissedDuty { resident, .. } if *resident == ill)),
        "the ill resident misses a duty"
    );
    // The storm adds no MissedDuty, and the ill resident's count is unchanged.
    assert_eq!(missed, missed_duties(&calm));
    let count = |w: &SliceSimulation| {
        w.to_parts()
            .episodes
            .iter()
            .find(|e| e.resident == ill)
            .unwrap()
            .missed
    };
    assert_eq!(count(&stormy), count(&calm));
    // No deferred resident ever records a MissedDuty.
    for (_, resident, _) in stayed(&stormy) {
        assert!(
            !missed.iter().any(
                |(_, p)| matches!(p, Payload::MissedDuty { resident: r, .. } if *r == resident)
            )
        );
    }
}

/// Per-tick state hashes of a fixed storm scenario, with a curse and a bless mid-storm.
fn storm_hashes() -> (Vec<u64>, u64) {
    let mut w = storm_world();
    let ids = w.population().residents().ids().to_vec();
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    let mut hashes = Vec::new();
    for k in 0..70 * MINUTE {
        if k == 20 * MINUTE {
            ok(
                &mut w,
                Operation::Afflict {
                    resident: ids[3],
                    severity: 4,
                    duration_us: 30 * 60_000_000,
                },
            );
        }
        if k == 25 * MINUTE {
            ok(&mut w, Operation::Bless { resident: ids[5] });
        }
        ok(&mut w, Operation::Steps(1));
        hashes.push(w.state_hash());
    }
    (hashes, w.events().digest())
}
fn digest(hashes: &[u64], events: u64) -> u64 {
    ws_rng::hash(1, 2, &[ws_rng::hash(1, 3, hashes), events])
}
const STORM_CHILD_ENV: &str = "WS_SIM_STORM_REPLAY_CHILD";

/// Runs only inside the child process started by `storm_replays_same_and_separate_process`.
#[test]
fn storm_replay_child() {
    if std::env::var_os(STORM_CHILD_ENV).is_none() {
        return;
    }
    let (h, e) = storm_hashes();
    println!("storm_digest={:016x}", digest(&h, e));
}

#[test]
fn storm_replays_same_and_separate_process() {
    let (a, ea) = storm_hashes();
    let (b, eb) = storm_hashes();
    assert_eq!((&a, ea), (&b, eb));
    // Journal replay reproduces the same world.
    let mut w = storm_world();
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    ok(&mut w, Operation::Steps(30 * MINUTE));
    let r = w
        .apply(Operation::Storm {
            duration_us: HOUR_US,
        })
        .unwrap();
    assert_eq!(r.error, Some(SliceError::Rejected(Rejection::Busy)));
    ok(&mut w, Operation::Steps(40 * MINUTE));
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    ok(&mut w, Operation::Steps(10 * MINUTE));
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.checkpoint(), w.checkpoint());
    assert_eq!(replay.operation_digest(), w.operation_digest());
    assert_eq!(snapshot(&replay), snapshot(&w));
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "storm_replay_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(STORM_CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let (_, hex) = stdout
        .split_once("storm_digest=")
        .expect("child printed a digest");
    assert_eq!(u64::from_str_radix(&hex[..16], 16).unwrap(), digest(&a, ea));
}

#[test]
fn rebuilt_index_equals_live_after_storm() {
    let mut w = storm_world();
    let ids = w.population().residents().ids().to_vec();
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[0],
            severity: 3,
            duration_us: 2 * HOUR_US,
        },
    );
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    ok(&mut w, Operation::Steps(10 * MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[1] });
    ok(&mut w, Operation::Steps(50 * MINUTE));
    ok(
        &mut w,
        Operation::Storm {
            duration_us: 10 * 60_000_000,
        },
    );
    ok(&mut w, Operation::Steps(20 * MINUTE));
    let storms: Vec<_> = roots(&w)
        .into_iter()
        .filter(|r| w.consequences(*r).unwrap().0.tag == 16)
        .collect();
    assert_eq!(storms.len(), 2);
    for r in storms {
        assert_eq!(tags(&w, r), vec![16, 18, 19]);
    }
    let live = snapshot(&w);
    let before = w.checkpoint();
    w.rebuild_consequence_index().unwrap();
    assert_eq!(snapshot(&w), live);
    assert_eq!(w.checkpoint(), before);
}

/// ADR 0021 §4 lock-step: the deferral is identical at L1 and L2.
#[test]
fn storm_defers_identically_at_l1_and_l2() {
    let run = |l1: bool| {
        let mut w = storm_world();
        if l1 {
            for id in w.population().residents().ids().to_vec() {
                ok(
                    &mut w,
                    Operation::SetDetail {
                        resident: id,
                        level: ws_life::behavior::Detail::L1,
                    },
                );
            }
        }
        ok(
            &mut w,
            Operation::Storm {
                duration_us: HOUR_US,
            },
        );
        ok(&mut w, Operation::Steps(60 * MINUTE));
        let rows: Vec<_> = w
            .events()
            .records()
            .iter()
            .filter(|e| matches!(e.record.payload, Payload::StayedInside { .. }))
            .map(|e| (e.record.time, e.record.payload))
            .collect();
        (rows, tags(&w, last_root(&w)))
    };
    let l2 = run(false);
    assert!(!l2.0.is_empty());
    assert_eq!(run(true), l2);
}
