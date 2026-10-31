//! ADR 0039 faith and daily goals wired into the slice.
use ws_sim::ws_events::{Confidence, Event, EventId, Origin, Payload, Relation, Target};
use ws_sim::{
    EntityId, SimTime,
    slice::{GoalKind, Operation, SliceConfig, SliceSimulation, generate_goals},
};

const SEED: u64 = 42;
const DAY: i64 = 86_400_000_000;
const HOUR: i64 = 3_600_000_000;
const MINUTE: u64 = 600;

fn world_at(us: i64) -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed: SEED,
        start: SimTime::from_micros_since_epoch(us),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn ok(w: &mut SliceSimulation, op: Operation) {
    assert_eq!(w.apply(op).unwrap().error, None, "{op:?}");
}
fn goals(w: &SliceSimulation) -> Vec<Event> {
    w.events()
        .records()
        .iter()
        .filter(|e| matches!(e.record.payload, Payload::GoalCompleted { .. }))
        .copied()
        .collect()
}
fn faith(w: &SliceSimulation, id: EntityId) -> i16 {
    w.god().faith().iter().find(|(r, _)| *r == id).unwrap().1
}
fn slot(w: &SliceSimulation, kind: GoalKind) -> usize {
    w.god()
        .goals()
        .goals()
        .iter()
        .position(|g| g.kind == kind)
        .expect("the day has this goal")
}
/// The first day at or after `from` (stepping by `step`) whose goals include `kind`.
fn day_with(kind: GoalKind, from: i64, step: i64) -> (i64, u8) {
    let caps = world_at(0).caps();
    (0..60)
        .map(|k| from + k * step)
        .find_map(|d| {
            generate_goals(SEED, d, caps)
                .goals()
                .iter()
                .find(|g| g.kind == kind)
                .map(|g| (d, g.target))
        })
        .unwrap()
}
fn last(w: &SliceSimulation, f: impl Fn(&Payload) -> bool) -> EventId {
    w.events()
        .records()
        .iter()
        .rev()
        .find(|e| f(&e.record.payload))
        .unwrap()
        .id
}
/// One goal event: its words, `System(87)` and one `ContributedTo/Direct` link.
fn assert_goal(e: &Event, day: i64, slot: usize, kind: GoalKind, target: u8, trigger: EventId) {
    assert_eq!(
        e.record.payload,
        Payload::GoalCompleted {
            day,
            slot: slot as u8,
            kind: kind as u8,
            target,
        }
    );
    assert_eq!(e.record.payload.tag(), 23);
    assert_eq!(e.record.origin, Origin::System(87));
    let causes: Vec<_> = e.record.causes.iter().copied().collect();
    assert_eq!(causes.len(), 1);
    assert_eq!(causes[0].target, Target::Id(trigger));
    assert_eq!(
        (causes[0].relation, causes[0].confidence),
        (Relation::ContributedTo, Confidence::Direct)
    );
}
/// No consequence group holds a goal event; the rebuilt index equals the live one.
fn assert_never_indexed(w: &mut SliceSimulation) {
    let snapshot = |w: &SliceSimulation| {
        w.events()
            .records()
            .iter()
            .filter(|e| e.record.payload.is_slice_root())
            .map(|e| {
                let (g, events) = w.consequences(e.id).unwrap();
                assert!(events.iter().all(|e| e.record.payload.tag() != 23));
                *g
            })
            .collect::<Vec<_>>()
    };
    let live = snapshot(w);
    w.rebuild_consequence_index().unwrap();
    assert_eq!(snapshot(w), live);
}
fn set_l1(w: &mut SliceSimulation) {
    for id in w.population().residents().ids().to_vec() {
        ok(
            w,
            Operation::SetDetail {
                resident: id,
                level: ws_life::behavior::Detail::L1,
            },
        );
    }
}
fn rowhouse(w: &SliceSimulation) -> EntityId {
    w.population()
        .world()
        .buildings()
        .iter()
        .find(|(_, b)| b.featured == Some("rowhouse"))
        .unwrap()
        .0
}

#[test]
fn untouched_world_stays_untouched() {
    let mut w = world_at(DAY - 60_000_000);
    let caps = w.caps();
    assert_eq!(w.god().goals(), &generate_goals(SEED, 0, caps));
    let before = w.state_hash();
    // Across two midnights with no cast: 24 h and 2 min.
    ok(&mut w, Operation::Steps(24 * 60 * MINUTE + 2 * MINUTE));
    assert_eq!(w.god().goals().day, 2);
    assert_eq!(w.god().goals(), &generate_goals(SEED, 2, caps));
    assert!(!w.god().touched());
    assert!(w.god().faith().iter().all(|(_, f)| *f == 0));
    assert_eq!(w.god().block_faith(), 0);
    assert!(goals(&w).is_empty());
    assert_ne!(w.state_hash(), before);
}

#[test]
fn bless_goal_completes_and_records_event() {
    let (day, target) = day_with(GoalKind::BlessDistinct, 0, 1);
    let mut w = world_at(day * DAY + 12 * HOUR);
    ok(&mut w, Operation::Steps(MINUTE));
    let s = slot(&w, GoalKind::BlessDistinct);
    let ids = w.population().residents().ids().to_vec();
    for (i, id) in ids.iter().take(usize::from(target)).enumerate() {
        assert!(goals(&w).is_empty());
        assert_eq!(usize::from(w.god().goals().goals()[s].progress), i);
        ok(&mut w, Operation::Bless { resident: *id });
    }
    let g = goals(&w);
    assert_eq!(g.len(), 1);
    let root = last(&w, |p| matches!(p, Payload::BlessApplied { .. }));
    assert_goal(&g[0], day, s, GoalKind::BlessDistinct, target, root);
    assert!(w.god().goals().goals()[s].done);
    // Each blessed +15, everyone +10 for the goal.
    for (i, id) in ids.iter().enumerate() {
        let want = if i < usize::from(target) { 25 } else { 10 };
        assert_eq!(faith(&w, *id), want, "resident {i}");
    }
    assert!(w.god().block_faith() >= 10);
    // A repeat bless after completion records nothing more.
    ok(&mut w, Operation::Bless { resident: ids[0] });
    assert_eq!(goals(&w).len(), 1);
    assert_never_indexed(&mut w);
}

#[test]
fn cure_goal_needs_bless_cure() {
    let (day, _) = day_with(GoalKind::Cure, 0, 1);
    let base = world_at(day * DAY + 12 * HOUR);
    let s = slot(&base, GoalKind::Cure);
    let ids = base.population().residents().ids().to_vec();

    // A healthy resident's bless relieves needs but cures nobody.
    let mut healthy = base.clone();
    ok(&mut healthy, Operation::Steps(MINUTE));
    ok(&mut healthy, Operation::Bless { resident: ids[1] });
    assert!(!healthy.god().goals().goals()[s].done);
    assert!(
        goals(&healthy)
            .iter()
            .all(|e| !matches!(e.record.payload, Payload::GoalCompleted { kind: 3, .. }))
    );

    // Curse then bless the same resident: the bless's Recovered completes it.
    let mut w = base;
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[0],
            severity: 5,
            duration_us: 15 * 60_000_000,
        },
    );
    assert_eq!(faith(&w, ids[0]), -20);
    ok(&mut w, Operation::Steps(MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[0] });
    let cure = goals(&w)
        .into_iter()
        .find(|e| matches!(e.record.payload, Payload::GoalCompleted { kind: 3, .. }))
        .unwrap();
    let recovered = last(&w, |p| matches!(p, Payload::Recovered { .. }));
    assert_goal(&cure, day, s, GoalKind::Cure, 1, recovered);
    assert!(w.god().goals().goals()[s].done);
    // -20 curse, +15 bless, +10 goal.
    assert_eq!(faith(&w, ids[0]), 5);
    assert_eq!(faith(&w, ids[1]), 10);
    // A natural recovery is not a bless cure: a second episode recovering on
    // its own records no further goal.
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[2],
            severity: 2,
            duration_us: 60_000_000,
        },
    );
    ok(&mut w, Operation::Steps(5 * MINUTE));
    assert_eq!(goals(&w).len(), 1);
    assert_never_indexed(&mut w);
}

#[test]
fn storm_and_gathering_goals_complete() {
    // Storm: Monday 07:30, a one-hour storm across the 08:00 departures.
    let (day, target) = day_with(GoalKind::Storm, 0, 7);
    let mut w = world_at(day * DAY + 7 * HOUR + 30 * 60_000_000);
    let s = slot(&w, GoalKind::Storm);
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR as u64,
        },
    );
    ok(&mut w, Operation::Steps(61 * MINUTE));
    let kept = w
        .events()
        .records()
        .iter()
        .find_map(|e| match e.record.payload {
            Payload::StormKeptInside { count } => Some((e.id, count)),
            _ => None,
        })
        .unwrap();
    assert!(kept.1 >= u32::from(target), "{kept:?} vs {target}");
    let g = goals(&w);
    assert_eq!(g.len(), 1);
    assert_goal(&g[0], day, s, GoalKind::Storm, target, kept.0);
    let goal = w.god().goals().goals()[s];
    assert!(goal.done);
    assert_eq!(u32::from(goal.progress), kept.1);
    assert_never_indexed(&mut w);

    // Gathering: Monday 21:00 at the rowhouse, every resident at L1.
    let (day, target) = day_with(GoalKind::Gathering, 0, 7);
    let mut w = world_at(day * DAY + 21 * HOUR);
    set_l1(&mut w);
    let s = slot(&w, GoalKind::Gathering);
    let building = rowhouse(&w);
    ok(
        &mut w,
        Operation::Gather {
            building,
            duration_us: 30 * 60_000_000,
        },
    );
    ok(&mut w, Operation::Steps(40 * MINUTE));
    let joins: Vec<EventId> = w
        .events()
        .records()
        .iter()
        .filter(|e| matches!(e.record.payload, Payload::JoinedGathering { .. }))
        .map(|e| e.id)
        .collect();
    assert!(joins.len() >= usize::from(target));
    let g = goals(&w);
    assert_eq!(g.len(), 1);
    assert_goal(
        &g[0],
        day,
        s,
        GoalKind::Gathering,
        target,
        joins[usize::from(target) - 1],
    );
    let goal = w.god().goals().goals()[s];
    assert!(goal.done);
    assert_eq!(goal.progress, target);
    assert_eq!(w.god().gathering(), None, "the gathering ended");
    assert_never_indexed(&mut w);
}

#[test]
fn midnight_regenerates_once_and_counts_tick_for_new_day() {
    // Two consecutive days that both ask for distinct blessings.
    let caps = world_at(0).caps();
    let day = (0..60)
        .find(|d| {
            [*d, d + 1].iter().all(|x| {
                generate_goals(SEED, *x, caps)
                    .goals()
                    .iter()
                    .any(|g| g.kind == GoalKind::BlessDistinct && g.target >= 2)
            })
        })
        .unwrap();
    let mut w = world_at((day + 1) * DAY - 60_000_000);
    let id = w.population().residents().ids()[0];
    ok(&mut w, Operation::Bless { resident: id });
    let s = slot(&w, GoalKind::BlessDistinct);
    assert_eq!(w.god().goals().goals()[s].progress, 1);
    assert_eq!(w.god().goals().blessed(), &[id]);
    // Step one tick at a time up to the boundary: the day changes at the
    // first tick whose time is midnight, not before.
    let midnight = SimTime::from_micros_since_epoch((day + 1) * DAY);
    while w.clock().now() < midnight {
        assert_eq!(w.god().goals().day, day);
        ok(&mut w, Operation::Steps(1));
    }
    assert_eq!(w.clock().now(), midnight);
    let fresh = generate_goals(SEED, day + 1, caps);
    assert_eq!(w.god().goals(), &fresh);
    assert_eq!(faith(&w, id), 15, "faith survives the day");
    // The same resident counts again on the new day.
    ok(&mut w, Operation::Bless { resident: id });
    let s = slot(&w, GoalKind::BlessDistinct);
    assert_eq!(w.god().goals().goals()[s].progress, 1);
    // Later ticks of the same day never regenerate.
    ok(&mut w, Operation::Steps(10 * MINUTE));
    assert_eq!(w.god().goals().day, day + 1);
    assert_eq!(w.god().goals().goals()[s].progress, 1);
    assert_eq!(w.god().goals().blessed(), &[id]);
    let target = fresh.goals()[s].target;
    let ids = w.population().residents().ids().to_vec();
    for r in ids
        .iter()
        .filter(|r| **r != id)
        .take(usize::from(target) - 1)
    {
        ok(&mut w, Operation::Bless { resident: *r });
    }
    let g = goals(&w);
    assert_eq!(g.len(), 1);
    let root = last(&w, |p| matches!(p, Payload::BlessApplied { .. }));
    assert_goal(&g[0], day + 1, s, GoalKind::BlessDistinct, target, root);
}

/// Steps one minute at a time, recording every tick's state hash.
fn record(
    w: &mut SliceSimulation,
    hashes: &mut Vec<u64>,
    minutes: u64,
    mut each: impl FnMut(&mut SliceSimulation, u64),
) {
    for k in 0..minutes * MINUTE {
        each(w, k);
        ok(w, Operation::Steps(1));
        hashes.push(w.state_hash());
    }
}
fn assert_done(w: &SliceSimulation, kind: GoalKind) {
    let g = w
        .god()
        .goals()
        .goals()
        .iter()
        .find(|g| g.kind == kind)
        .unwrap();
    assert!(g.done, "{kind:?} must complete in the digest scenario");
}

/// Per-tick state hashes and the history digest of fixed goal scenarios, one
/// world per goal kind (a day holds at most three goals): a gathering with a
/// mid-gathering bless (kind 1), a storm across the 08:00 departures (kind 4),
/// a curse then bless cure (kind 3) and distinct blessings (kind 2).
fn god_hashes() -> (Vec<u64>, u64, Vec<(EntityId, i16)>) {
    let mut hashes = Vec::new();
    let mut events = Vec::new();
    let mut faith_all = Vec::new();
    let mut finish = |w: &SliceSimulation| {
        events.push(w.events().digest());
        faith_all.extend_from_slice(w.god().faith());
    };

    let (day, _) = day_with(GoalKind::Gathering, 0, 7);
    let mut w = world_at(day * DAY + 21 * HOUR);
    set_l1(&mut w);
    let building = rowhouse(&w);
    let ids = w.population().residents().ids().to_vec();
    ok(
        &mut w,
        Operation::Gather {
            building,
            duration_us: 30 * 60_000_000,
        },
    );
    record(&mut w, &mut hashes, 40, |w, k| {
        if k == 5 * MINUTE {
            ok(w, Operation::Bless { resident: ids[19] });
        }
    });
    assert_done(&w, GoalKind::Gathering);
    assert_eq!(goals(&w).len(), 1);
    finish(&w);

    let (day, _) = day_with(GoalKind::Storm, 0, 7);
    let mut w = world_at(day * DAY + 7 * HOUR + 30 * 60_000_000);
    ok(
        &mut w,
        Operation::Storm {
            duration_us: HOUR as u64,
        },
    );
    record(&mut w, &mut hashes, 61, |_, _| {});
    assert_done(&w, GoalKind::Storm);
    finish(&w);

    let (day, _) = day_with(GoalKind::Cure, 0, 1);
    let mut w = world_at(day * DAY + 12 * HOUR);
    let ids = w.population().residents().ids().to_vec();
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[0],
            severity: 5,
            duration_us: 15 * 60_000_000,
        },
    );
    record(&mut w, &mut hashes, 5, |w, k| {
        if k == MINUTE {
            ok(w, Operation::Bless { resident: ids[0] });
        }
    });
    assert_done(&w, GoalKind::Cure);
    finish(&w);

    let (day, target) = day_with(GoalKind::BlessDistinct, 0, 1);
    let mut w = world_at(day * DAY + 12 * HOUR);
    let ids = w.population().residents().ids().to_vec();
    record(&mut w, &mut hashes, 5, |w, k| {
        if k % MINUTE == 0 && k / MINUTE < u64::from(target) {
            ok(
                w,
                Operation::Bless {
                    resident: ids[(k / MINUTE) as usize],
                },
            );
        }
    });
    assert_done(&w, GoalKind::BlessDistinct);
    finish(&w);

    let events = ws_rng::hash(1, 5, &events);
    (hashes, events, faith_all)
}
fn digest(hashes: &[u64], events: u64, faith: &[(EntityId, i16)]) -> u64 {
    let words: Vec<u64> = faith
        .iter()
        .flat_map(|(id, f)| [id.raw(), *f as u64])
        .collect();
    ws_rng::hash(
        1,
        2,
        &[
            ws_rng::hash(1, 3, hashes),
            events,
            ws_rng::hash(1, 4, &words),
        ],
    )
}
const GOD_CHILD_ENV: &str = "WS_SIM_GOD_REPLAY_CHILD";

/// Runs only inside the child process started by `god_replays_same_and_separate_process`.
#[test]
fn god_replay_child() {
    if std::env::var_os(GOD_CHILD_ENV).is_none() {
        return;
    }
    let (h, e, f) = god_hashes();
    println!("god_digest={:016x}", digest(&h, e, &f));
}

#[test]
fn god_replays_same_and_separate_process() {
    let (a, ea, fa) = god_hashes();
    let (b, eb, fb) = god_hashes();
    assert_eq!((&a, ea, &fa), (&b, eb, &fb));
    // Journal replay reproduces the god as well.
    let (day, _) = day_with(GoalKind::BlessDistinct, 0, 1);
    let mut w = world_at(day * DAY + 23 * HOUR + 50 * 60_000_000);
    let ids = w.population().residents().ids().to_vec();
    ok(&mut w, Operation::Bless { resident: ids[0] });
    ok(&mut w, Operation::Bless { resident: ids[1] });
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[2],
            severity: 3,
            duration_us: 20 * 60_000_000,
        },
    );
    ok(&mut w, Operation::Steps(20 * MINUTE));
    ok(&mut w, Operation::Bless { resident: ids[3] });
    assert!(w.god().touched());
    let (replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.god(), w.god());
    assert_eq!(replay.checkpoint(), w.checkpoint());
    assert_eq!(replay.operation_digest(), w.operation_digest());
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "god_replay_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(GOD_CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let (_, hex) = stdout
        .split_once("god_digest=")
        .expect("child printed a digest");
    assert_eq!(
        u64::from_str_radix(&hex[..16], 16).unwrap(),
        digest(&a, ea, &fa)
    );
}

/// Two goals completing in one batch from different triggers: the bless
/// root completes the distinct-bless goal, then its `Recovered` completes
/// the cure goal in a lower slot. Both events commit, in trigger order.
#[test]
fn two_goals_complete_in_one_batch() {
    let caps = world_at(0).caps();
    let (day, bless, cure, target) = (0..400)
        .find_map(|d| {
            let g = generate_goals(SEED, d, caps);
            let gs = g.goals();
            let b = gs.iter().position(|g| g.kind == GoalKind::BlessDistinct)?;
            let c = gs.iter().position(|g| g.kind == GoalKind::Cure)?;
            (b > c).then(|| (d, b, c, gs[b].target))
        })
        .expect("a day with the cure slot before the bless slot");
    let mut w = world_at(day * DAY + 12 * HOUR);
    let ids = w.population().residents().ids().to_vec();
    ok(
        &mut w,
        Operation::Afflict {
            resident: ids[0],
            severity: 5,
            duration_us: 15 * 60_000_000,
        },
    );
    for id in &ids[1..usize::from(target)] {
        ok(&mut w, Operation::Bless { resident: *id });
    }
    ok(&mut w, Operation::Steps(MINUTE));
    assert!(goals(&w).is_empty());
    ok(&mut w, Operation::Bless { resident: ids[0] });
    let g = goals(&w);
    assert_eq!(g.len(), 2);
    let root = last(&w, |p| matches!(p, Payload::BlessApplied { .. }));
    let recovered = last(&w, |p| matches!(p, Payload::Recovered { .. }));
    assert_goal(&g[0], day, bless, GoalKind::BlessDistinct, target, root);
    assert_goal(&g[1], day, cure, GoalKind::Cure, 1, recovered);
    assert!(g[0].id < g[1].id);
    let state = w.god().goals().goals();
    assert!(state[bless].done && state[cure].done);
    // Two rewards on top of -20 + 15 for the cured resident.
    assert_eq!(faith(&w, ids[0]), 15);
    assert_never_indexed(&mut w);
    let (replay, _) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert_eq!(replay.checkpoint(), w.checkpoint());
}
