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
    // The ADR 0034 home hold is left exactly as it was.
    assert_eq!(
        (state.override_kind, state.override_root),
        (routine.override_kind, routine.override_root)
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
    let routine = w.behavior().routine(id).unwrap();
    ok(&mut w, Operation::Bless { resident: id });
    let bless_root = last_root(&w);
    // The ADR 0034 routine, including its homeward leg, is untouched.
    assert_eq!(w.behavior().routine(id).unwrap(), routine);
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
