//! TP-010 acceptance tests (#11, ADR 0021): the first structured god
//! intervention, `afflict(illness, severity)`.

use ws_sim::commands::{
    Action, Command, Effect, Intervention, InterventionTarget, IssuedBy, Rejection, Timing,
    decode_batch, encode_batch,
};
use ws_sim::ws_events::{EventId, EventStore, Payload, Relation, Target};
use ws_sim::{EntityId, IllnessKind, Lod, SimTime, TimeScale, World, WorldConfig};
use ws_time::SimDuration;

fn world(seed: u64) -> World {
    World::new(WorldConfig::new(seed, SimTime::EPOCH)).unwrap()
}

fn run(w: &mut World, ticks: u64) {
    for _ in 0..ticks {
        w.step().unwrap();
    }
}

/// Steps until `done` holds (checked each tick), at most `max` ticks.
fn run_until(w: &mut World, max: u64, done: impl Fn(&World) -> bool) {
    for _ in 0..max {
        if done(w) {
            return;
        }
        w.step().unwrap();
    }
    panic!("not reached within {max} ticks");
}

fn all_ill_home(w: &World) -> bool {
    !w.toy().illness().is_empty()
        && w.toy().illness().ids().iter().all(|id| {
            let walk = w.toy().walks().get(*id);
            walk.is_some_and(|k| {
                k.arrived && k.target == w.toy().residents().get(*id).unwrap().home
            })
        })
}

fn afflict(target: InterventionTarget, severity: u8, timing: Timing) -> Command {
    Command::new(
        IssuedBy::Test,
        Action::Intervene(Intervention {
            target,
            effect: Effect::Afflict {
                illness: IllnessKind::Fever,
                severity,
            },
            timing,
        }),
    )
}

fn set_all(w: &mut World, level: Lod) {
    let req = w
        .toy()
        .buildings()
        .ids()
        .iter()
        .map(|id| (*id, level))
        .collect();
    w.submit(vec![Command::new(IssuedBy::Test, Action::SetNodeLod(req))])
        .unwrap();
    w.flush_commands().unwrap();
}

/// A world where agents have left home, plus a building with residents.
fn roaming(seed: u64) -> (World, EntityId) {
    let mut w = world(seed);
    run(&mut w, 300);
    let b = w.toy().buildings().ids()[0];
    assert!(!w.toy().residents_of(b).is_empty());
    (w, b)
}

fn events_of(w: &World, f: impl Fn(&Payload) -> bool) -> Vec<ws_sim::ws_events::Event> {
    w.events()
        .records()
        .iter()
        .filter(|e| f(&e.record.payload))
        .copied()
        .collect()
}

/// ADR 0019 / 24 §24.10 invariants that can be checked on a log.
fn check_log(w: &World) {
    let records = w.events().records();
    for (i, e) in records.iter().enumerate() {
        assert_eq!(e.id, EventId(i as u64 + 1), "IDs are dense and ordered");
        for c in e.record.causes.iter() {
            let Target::Id(t) = c.target else {
                panic!("a committed link is resolved")
            };
            assert!(t < e.id, "links point backwards");
            assert!(w.events().get(t).unwrap().record.time <= e.record.time);
        }
    }
    let known = w
        .toy()
        .buildings()
        .ids()
        .iter()
        .chain(w.toy().residents().ids())
        .copied()
        .collect();
    let rebuilt = EventStore::from_records(known, records).unwrap();
    assert_eq!(rebuilt.digest(), w.events().digest());
}

#[test]
fn afflicting_a_building_yields_the_expected_events_and_links() {
    let (mut w, b) = roaming(5);
    let residents = w.toy().residents_of(b).to_vec();
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        4,
        Timing::Now,
    )])
    .unwrap();
    run_until(&mut w, 100_000, all_ill_home);
    run(&mut w, 200);

    let roots = events_of(&w, |p| matches!(p, Payload::InterventionApplied { .. }));
    assert_eq!(roots.len(), 1);
    let Payload::InterventionApplied { affected, .. } = roots[0].record.payload else {
        unreachable!()
    };
    assert_eq!(affected as usize, residents.len());

    let ill = events_of(&w, |p| matches!(p, Payload::LifeIllness { .. }));
    assert_eq!(ill.len(), residents.len());
    for e in &ill {
        let causes: Vec<_> = e.record.causes.iter().collect();
        assert_eq!(causes.len(), 1);
        assert_eq!(causes[0].relation, Relation::CausedBy);
        assert_eq!(causes[0].target, Target::Id(roots[0].id));
    }

    let returned = events_of(&w, |p| matches!(p, Payload::ReturnedHome { .. }));
    assert!(!returned.is_empty(), "somebody walked home");
    for e in &returned {
        let Payload::ReturnedHome { resident, home } = e.record.payload else {
            unreachable!()
        };
        assert_eq!(home, b);
        let mine = ill
            .iter()
            .find(|i| matches!(i.record.payload, Payload::LifeIllness { resident: r, .. } if r == resident))
            .expect("the returning agent is ill");
        let links: Vec<_> = e.record.causes.iter().collect();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].relation, Relation::ContributedTo);
        assert_eq!(links[0].target, Target::Id(mine.id));
    }
    check_log(&w);
    w.check_lod_invariants(false).unwrap();
}

#[test]
fn ill_agents_end_up_home_and_stay() {
    let (mut w, b) = roaming(6);
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        2,
        Timing::Now,
    )])
    .unwrap();
    run_until(&mut w, 100_000, all_ill_home);
    let drawn: Vec<_> = w
        .toy()
        .walks()
        .iter()
        .filter(|(id, _)| w.toy().illness().contains(*id))
        .map(|(id, walk)| (id, walk.targets_drawn))
        .collect();
    for (id, walk) in w.toy().walks().iter() {
        if w.toy().illness().contains(id) {
            assert!(walk.arrived);
            assert_eq!(walk.target, w.toy().residents().get(id).unwrap().home);
        }
    }
    run(&mut w, 2000);
    for (id, drawn) in drawn {
        assert_eq!(
            w.toy().walks().get(id).unwrap().targets_drawn,
            drawn,
            "an ill agent draws no new target"
        );
    }
}

#[test]
fn why_is_agent_x_ill_resolves_to_the_intervention_and_its_journal_entry() {
    let (mut w, b) = roaming(7);
    let x = w.toy().residents_of(b)[0];
    assert!(w.why_ill(x).is_none());
    w.submit(vec![
        Command::new(IssuedBy::Test, Action::NoOp),
        afflict(InterventionTarget::Entity(x), 9, Timing::Now),
    ])
    .unwrap();
    run_until(&mut w, 100_000, |w| {
        w.events()
            .records()
            .iter()
            .any(|e| matches!(e.record.payload, Payload::ReturnedHome { .. }))
    });
    let why = w.why_ill(x).unwrap();
    assert_eq!(why.journal.sequence, 2);
    assert!(matches!(
        why.journal.command.action,
        Action::Intervene(Intervention {
            target: InterventionTarget::Entity(t),
            ..
        }) if t == x
    ));
    assert_eq!(why.journal.result, Ok(()));
    // The query also answers from a consequence event.
    let returned = events_of(&w, |p| matches!(p, Payload::ReturnedHome { .. }));
    assert!(!returned.is_empty());
    assert_eq!(
        w.events().root_intervention(returned[0].id).map(|e| e.id),
        Some(why.intervention_event)
    );
    // A healthy resident has no answer.
    let other = w.toy().residents().ids()[0];
    assert!(other == x || w.why_ill(other).is_none());
    assert_eq!(w.events().root_intervention(EventId(1)), None);
}

fn arrival_log(w: &World) -> Vec<(u64, u64, u32, bool)> {
    let mut v: Vec<_> = w
        .events()
        .records()
        .iter()
        .filter_map(|e| match e.record.payload {
            Payload::Arrived {
                resident,
                destination,
                leg,
            } => Some((resident.raw(), destination.raw(), leg, false)),
            Payload::ReturnedHome { resident, home } => Some((resident.raw(), home.raw(), 0, true)),
            _ => None,
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn illness_survives_demote_and_promote_and_behaves_the_same_at_both_levels() {
    // Reference: everything at L1. Subject: afflicted while at L2, promoted
    // later, demoted again, promoted again; exactly equal to the reference
    // whenever both are at L1 at the same boundary.
    let build = |roundtrip: bool| {
        let mut w = world(11);
        run(&mut w, 200);
        let b = w.toy().buildings().ids()[0];
        if roundtrip {
            set_all(&mut w, Lod::L2);
        }
        w.submit(vec![afflict(
            InterventionTarget::ResidentsOf(b),
            6,
            Timing::Now,
        )])
        .unwrap();
        // Long enough for ill agents to finish a leg, walk home and wait
        // there, with a level change every 500 ticks, so both levels see
        // every phase of the rule.
        for i in 0..60 {
            run(&mut w, 500);
            if roundtrip {
                set_all(&mut w, if i % 2 == 0 { Lod::L2 } else { Lod::L1 });
            }
        }
        run(&mut w, 100);
        (w, b)
    };
    let (reference, b) = build(false);
    let (subject, _) = build(true);
    assert!(!reference.toy().illness().is_empty());
    let rows = |w: &World| {
        w.toy()
            .illness()
            .iter()
            .map(|(id, i)| (id, i.kind, i.severity))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&subject), rows(&reference));
    // (The full hash also covers event IDs, which the extra LodChanged events
    // shift; everything the agents do is compared directly.)
    let walks = |w: &World| {
        w.toy()
            .walks()
            .iter()
            .map(|(id, k)| (id, k.target, k.arrived, k.wait_steps, k.targets_drawn))
            .collect::<Vec<_>>()
    };
    assert_eq!(walks(&subject), walks(&reference));
    // Walking agents' positions are re-derived at a promotion (ADR 0017
    // bounds the difference); ill agents are home, which is exact.
    for id in reference.toy().illness().ids() {
        assert_eq!(
            subject.toy().positions().get(*id),
            reference.toy().positions().get(*id)
        );
    }
    assert_eq!(arrival_log(&subject), arrival_log(&reference));
    for id in reference.toy().residents_of(b) {
        assert!(subject.why_ill(*id).is_some());
    }
    check_log(&subject);
    subject.check_lod_invariants(true).unwrap();
}

#[test]
fn an_afflicted_agent_at_l2_stays_ill_after_promotion_and_vice_versa() {
    let (mut w, b) = roaming(12);
    set_all(&mut w, Lod::L2);
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        3,
        Timing::Now,
    )])
    .unwrap();
    run(&mut w, 10);
    let ill: Vec<_> = w.toy().illness().ids().to_vec();
    assert_eq!(ill.len(), w.toy().residents_of(b).len());
    assert!(ill.iter().all(|id| !w.toy().walks().contains(*id)));
    set_all(&mut w, Lod::L1);
    run(&mut w, 5);
    assert_eq!(w.toy().illness().ids(), ill.as_slice());
    assert!(ill.iter().all(|id| w.toy().walks().contains(*id)));
    set_all(&mut w, Lod::L2);
    run(&mut w, 5);
    assert_eq!(w.toy().illness().ids(), ill.as_slice());
}

#[test]
fn invalid_interventions_are_rejected_and_change_nothing() {
    let (mut w, b) = roaming(8);
    let resident = w.toy().residents_of(b)[0];
    let cases = [
        (
            afflict(InterventionTarget::Entity(b), 3, Timing::Now),
            Rejection::UnknownTarget(b),
        ),
        (
            afflict(InterventionTarget::ResidentsOf(resident), 3, Timing::Now),
            Rejection::UnknownTarget(resident),
        ),
        (
            afflict(InterventionTarget::Entity(resident), 0, Timing::Now),
            Rejection::SeverityOutOfRange(0),
        ),
        (
            afflict(InterventionTarget::Entity(resident), 11, Timing::Now),
            Rejection::SeverityOutOfRange(11),
        ),
    ];
    for (command, reason) in cases {
        let state = w.toy().state_hash();
        let events = w.events().digest();
        w.submit(vec![command]).unwrap();
        w.flush_commands().unwrap();
        assert_eq!(w.journal().last().unwrap().result, Err(reason));
        assert_eq!(w.toy().state_hash(), state, "{reason:?} changed the world");
        assert_eq!(w.events().digest(), events, "{reason:?} published events");
        assert!(w.toy().illness().is_empty());
    }
}

#[test]
fn a_building_without_residents_is_accepted_and_affects_nobody() {
    let mut w = world(9);
    let empty = w
        .toy()
        .buildings()
        .ids()
        .iter()
        .copied()
        .find(|b| w.toy().residents_of(*b).is_empty());
    let Some(empty) = empty else { return };
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(empty),
        1,
        Timing::Now,
    )])
    .unwrap();
    w.flush_commands().unwrap();
    assert_eq!(w.journal()[0].result, Ok(()));
    assert_eq!(w.journal()[0].effect, 0);
    assert!(w.toy().illness().is_empty());
}

#[test]
fn reafflicting_makes_no_second_illness() {
    let (mut w, b) = roaming(13);
    let cmd = afflict(InterventionTarget::ResidentsOf(b), 5, Timing::Now);
    w.submit(vec![cmd.clone()]).unwrap();
    w.flush_commands().unwrap();
    let n = w.toy().illness().len();
    w.submit(vec![cmd]).unwrap();
    w.flush_commands().unwrap();
    assert_eq!(w.toy().illness().len(), n);
    assert_eq!(w.journal()[1].effect, 0);
    assert_eq!(
        events_of(&w, |p| matches!(p, Payload::LifeIllness { .. })).len(),
        n
    );
    // The first illness explains the state.
    let x = w.toy().residents_of(b)[0];
    assert_eq!(w.why_ill(x).unwrap().journal.sequence, 1);
}

#[test]
fn a_timed_intervention_waits_for_its_time() {
    let (mut w, b) = roaming(14);
    let at = w
        .now()
        .saturating_add(SimDuration::from_micros(30 * 100_000));
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        5,
        Timing::At(at),
    )])
    .unwrap();
    run(&mut w, 29);
    assert!(w.toy().illness().is_empty());
    assert!(events_of(&w, |p| matches!(p, Payload::InterventionApplied { .. })).is_empty());
    run(&mut w, 1);
    assert_eq!(w.toy().illness().len(), w.toy().residents_of(b).len());
    let root = events_of(&w, |p| matches!(p, Payload::InterventionApplied { .. }));
    assert_eq!(root.len(), 1);
    assert_eq!(root[0].record.time, at);
    // A time already reached applies at the next boundary it is processed.
    let x = w.toy().residents_of(w.toy().buildings().ids()[1])[0];
    w.submit(vec![afflict(
        InterventionTarget::Entity(x),
        1,
        Timing::At(SimTime::EPOCH),
    )])
    .unwrap();
    w.flush_commands().unwrap();
    assert!(w.toy().illness().contains(x));
    check_log(&w);
}

#[test]
fn the_wire_format_of_an_intervention_is_explicit_and_round_trips() {
    let id = EntityId::spawned(42).unwrap();
    let commands = vec![
        afflict(InterventionTarget::Entity(id), 7, Timing::Now),
        afflict(
            InterventionTarget::ResidentsOf(id),
            10,
            Timing::At(SimTime::from_micros_since_epoch(-5)),
        ),
    ];
    let bytes = encode_batch(&commands).unwrap();
    assert_eq!(bytes.len(), 8 + 2 * (8 + 8 + 36));
    assert_eq!(decode_batch(&bytes).unwrap(), commands);
    // First command: opcode 3, then length 44.
    assert_eq!(&bytes[8..16], &[3, 0, 0, 0, 44, 0, 0, 0]);
    // Any single corrupted structural word is refused.
    for at in [8, 12, 24, 36, 40, 48, 52] {
        let mut b = bytes.clone();
        b[at] = 0xfe;
        assert!(decode_batch(&b).is_err() || decode_batch(&b).unwrap() != commands);
    }
    // Severity does not fit a byte: still decodes, and is a semantic rejection.
    let mut b = bytes.clone();
    b[44..48].copy_from_slice(&1000_u32.to_le_bytes());
    let Action::Intervene(i) = &decode_batch(&b).unwrap()[0].action else {
        panic!()
    };
    assert_eq!(
        i.effect,
        Effect::Afflict {
            illness: IllnessKind::Fever,
            severity: u8::MAX
        }
    );
}

#[test]
fn replaying_the_same_commands_gives_identical_checkpoints_every_tick() {
    let play = || {
        let (mut w, b) = roaming(15);
        let mut hashes = Vec::new();
        w.submit(vec![
            Command::new(IssuedBy::Test, Action::SetTimeScale(TimeScale::X1)),
            afflict(InterventionTarget::ResidentsOf(b), 4, Timing::Now),
        ])
        .unwrap();
        for i in 0..800 {
            if i == 100 {
                let at = w
                    .now()
                    .saturating_add(SimDuration::from_micros(50 * 100_000));
                let c = w.toy().buildings().ids()[2];
                w.submit(vec![afflict(
                    InterventionTarget::ResidentsOf(c),
                    8,
                    Timing::At(at),
                )])
                .unwrap();
            }
            w.step().unwrap();
            hashes.push(w.checkpoint());
        }
        hashes
    };
    assert_eq!(play(), play());
}

#[test]
#[ignore = "measurement, run with --release -- --ignored --nocapture"]
// Timing a command is a host measurement, not simulation logic.
#[allow(clippy::disallowed_methods)]
fn measure_apply_cost() {
    use ws_sim::Population;
    let mut cfg = WorldConfig::new(3, SimTime::EPOCH);
    cfg.population = Population {
        min_per_building: 1_000,
        max_per_building: 1_000,
    };
    let mut w = World::new(cfg).unwrap();
    run(&mut w, 50);
    let b = w.toy().buildings().ids()[0];
    let n = w.toy().residents_of(b).len();
    let t = std::time::Instant::now();
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        4,
        Timing::Now,
    )])
    .unwrap();
    w.flush_commands().unwrap();
    println!(
        "{} agents in the world, {n} afflicted: {:?}",
        w.toy().residents().len(),
        t.elapsed()
    );
}
