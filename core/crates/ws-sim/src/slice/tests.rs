use super::*;
use ws_events::{Actor, Role};
fn config(start: i64) -> SliceConfig {
    SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(start),
        schedule_epoch: SimTime::EPOCH,
    }
}
fn advance(w: &mut SliceSimulation, n: u64) {
    assert_eq!(
        w.apply(Operation::Steps(n)).unwrap(),
        Report {
            committed: n,
            error: None
        }
    );
}
fn known(w: &SliceSimulation) -> Vec<crate::EntityId> {
    w.population
        .world()
        .buildings()
        .ids()
        .iter()
        .chain(w.population.households().ids())
        .chain(w.population.residents().ids())
        .chain(w.population.workplaces().ids())
        .copied()
        .collect()
}
#[test]
fn full_week_events_match_all_boundaries_and_rebuild_queries_without_feedback() {
    let mut w = SliceSimulation::new(config(0)).unwrap();
    assert_eq!(w.events.records().len(), 1);
    let schedules = w.schedules.clone();
    let mut expected = Vec::new();
    let mut at = schedules.next_change(w.clock.now());
    let end = SimTime::from_micros_since_epoch(7 * 86_400_000_000);
    while let Some(time) = at {
        if time > end {
            break;
        }
        expected.extend(schedules.facts_at(time).unwrap());
        at = schedules.next_change(time);
    }
    advance(&mut w, 7 * 864_000);
    assert_eq!(w.events.records().len(), expected.len() + 1);
    for (event, fact) in w.events.records()[1..].iter().zip(expected) {
        assert_eq!(event.record, fact);
        assert_eq!(
            event.tick,
            event.record.time.as_micros_since_epoch() as u64 / 100_000
        );
        let Payload::ScheduleChanged {
            resident,
            destination,
            ..
        } = fact.payload
        else {
            panic!("schedule fact")
        };
        assert_eq!(
            fact.actors.iter().copied().collect::<Vec<_>>(),
            [
                Actor {
                    id: resident,
                    role: Role::Subject
                },
                Actor {
                    id: destination,
                    role: Role::Object
                }
            ]
        );
        assert!(event.record.causes.is_empty());
        assert_eq!(event.record.system, 60);
        assert_eq!(event.record.lod, 2);
    }
    let before = w.checkpoint();
    let rebuilt = EventStore::from_records(known(&w), w.events.records()).unwrap();
    assert_eq!(rebuilt.digest(), w.events.digest());
    for id in known(&w) {
        assert_eq!(
            rebuilt.query(Some(id), SimTime::MIN, SimTime::MAX),
            w.events.query(Some(id), SimTime::MIN, SimTime::MAX)
        );
    }
    assert_eq!(w.checkpoint(), before);
    assert_eq!(w.operations.len(), 1); // quiet ticks never create millions of command records
}
#[test]
fn step_grouping_and_cache_rebuild_preserve_ids_ordinals_and_all_digests() {
    let mut a = SliceSimulation::new(config(6 * 3_600_000_000)).unwrap();
    let mut b = a.clone();
    for n in [1, 17, 100, 2, 9999, 18_000, 1024, 5000] {
        advance(&mut a, n);
        for _ in 0..n {
            b.step().unwrap();
        }
        b.next_change = b.schedules.next_change(b.clock.now());
        assert_eq!(a.checkpoint(), b.checkpoint());
        assert_eq!(a.events.records(), b.events.records());
        assert_eq!(a.operation_digest(), b.operation_digest());
        assert_eq!(a.operations, b.operations);
    }
}
#[test]
fn scale_pause_host_remainder_cap_and_explicit_steps_follow_clock_contract() {
    let mut w = SliceSimulation::new(config(0)).unwrap();
    let mut c = w.clock.clone();
    for (scale, elapsed) in [
        (TimeScale::X1, 90_000),
        (TimeScale::Paused, u64::MAX),
        (TimeScale::X10, 123),
        (TimeScale::X100, u64::MAX),
        (TimeScale::X1, 10_000),
    ] {
        w.apply(Operation::SetScale(scale)).unwrap();
        c.set_scale(scale);
        let due = c.advance_by_host_elapsed(elapsed).unwrap();
        let r = w.apply(Operation::HostElapsed(elapsed)).unwrap();
        assert_eq!(
            r,
            Report {
                committed: u64::from(due),
                error: None
            }
        );
        assert_eq!(w.clock, c);
    }
    w.apply(Operation::SetScale(TimeScale::Paused)).unwrap();
    let old = w.clock.steps();
    w.step().unwrap();
    assert_eq!(w.clock.steps(), old + 1);
}
#[test]
fn failed_tick_retains_atomic_state_and_exact_batch_prefix_then_retry_matches() {
    let base = SliceSimulation::new(config(6 * 3_600_000_000)).unwrap();
    let boundary = base.next_change.unwrap();
    let count = ((boundary.as_micros_since_epoch() - base.clock.now().as_micros_since_epoch())
        / 100_000) as u64;
    let mut w = base.clone();
    w.fail_tick = Some(count);
    let report = w.apply(Operation::Steps(count + 5)).unwrap();
    assert_eq!(report.committed, count - 1);
    assert_eq!(
        report.error,
        Some(SliceError::History(EventError::Resource))
    );
    let mut prefix = base.clone();
    advance(&mut prefix, count - 1);
    assert_eq!(w.checkpoint(), prefix.checkpoint());
    assert_eq!(w.events.records(), prefix.events.records());
    let stopped = *w.operations.last().unwrap();
    assert_eq!(stopped.stop, Some(prefix.checkpoint()));
    // Environmental replay verifies/stops at the recorded prefix; it does not retry the failed tick.
    let mut replay = base.clone();
    advance(&mut replay, stopped.report.committed);
    assert_eq!(replay.checkpoint(), stopped.stop.unwrap());
    let before = w.checkpoint();
    assert_eq!(w.step(), Err(SliceError::History(EventError::Resource)));
    assert_eq!(w.checkpoint(), before);
    w.fail_tick = None;
    advance(&mut w, 6);
    let mut success = base;
    advance(&mut success, count + 5);
    assert_eq!(w.checkpoint(), success.checkpoint());
    assert_eq!(w.events.records(), success.events.records());
}
#[test]
fn host_failure_keeps_previous_pacing_remainder_and_records_prefix_stop() {
    let mut w = SliceSimulation::new(config(0)).unwrap();
    w.apply(Operation::HostElapsed(90_000)).unwrap();
    w.fail_tick = Some(3);
    let r = w.apply(Operation::HostElapsed(1_000_000)).unwrap();
    assert_eq!(r.committed, 2);
    assert_eq!(w.clock.pending_micros(), 90_000);
    assert!(r.error.is_some());
    assert_eq!(w.operations.last().unwrap().stop, Some(w.checkpoint()));
}
#[test]
fn extreme_start_overflow_is_typed_and_grid_is_checked() {
    let max = i64::MAX / 100_000 * 100_000;
    let mut w = SliceSimulation::new(config(max - 100_000)).unwrap();
    let report = w.apply(Operation::Steps(2)).unwrap();
    assert_eq!(report.committed, 1);
    assert_eq!(report.error, Some(SliceError::TimeOverflow));
    assert_eq!(w.clock.now().as_micros_since_epoch(), max);
    let before = w.checkpoint();
    let r = w.apply(Operation::HostElapsed(u64::MAX)).unwrap();
    assert_eq!(r.committed, 0);
    assert_eq!(r.error, Some(SliceError::TimeOverflow));
    assert_eq!(w.checkpoint(), before);
    assert!(matches!(
        SliceSimulation::new(config(1)),
        Err(SliceError::Grid)
    ));
    let mut invalid = config(0);
    invalid.schedule_epoch = SimTime::from_micros_since_epoch(-1);
    assert!(matches!(
        SliceSimulation::new(invalid),
        Err(SliceError::Grid)
    ));
}
fn script() -> Vec<(Checkpoint, u64)> {
    let mut w = SliceSimulation::new(config(6 * 3_600_000_000)).unwrap();
    let mut out = Vec::new();
    for operation in [
        Operation::Steps(18_000),
        Operation::SetScale(TimeScale::Paused),
        Operation::HostElapsed(u64::MAX),
        Operation::Steps(9),
        Operation::SetScale(TimeScale::X100),
        Operation::HostElapsed(77_777),
        Operation::Steps(25_000),
    ] {
        w.apply(operation).unwrap();
        out.push((w.checkpoint(), w.operation_digest()));
    }
    let until = (w
        .schedules
        .next_change(w.clock.now())
        .unwrap()
        .as_micros_since_epoch()
        - w.clock.now().as_micros_since_epoch()) as u64
        / 100_000;
    if until > 128 {
        advance(&mut w, until - 128);
        out.push((w.checkpoint(), w.operation_digest()));
    }
    for _ in 0..256 {
        w.step().unwrap();
        out.push((w.checkpoint(), w.operation_digest()));
    }
    out
}
fn script_digest() -> u64 {
    let mut h = SipHasher24::new(1, 2);
    for (p, op) in script() {
        h.write_words(&[p.tick, p.state, p.events, p.pending_micros, op]);
    }
    h.finish()
}
#[test]
fn slice_replay_child() {
    if std::env::var_os("WS_SLICE_REPLAY_CHILD").is_some() {
        println!("slice_digest={:016x}", script_digest());
    }
}
#[test]
fn identical_typed_script_replays_state_events_and_operations_in_another_process() {
    assert_eq!(script(), script());
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "slice::tests::slice_replay_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WS_SLICE_REPLAY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let (_, hex) = stdout.split_once("slice_digest=").unwrap();
    assert_eq!(
        u64::from_str_radix(&hex[..16], 16).unwrap(),
        script_digest()
    );
}

#[test]
fn typed_replay_verifies_normal_and_environmentally_stopped_operation_digests() {
    for host in [false, true] {
        let mut original = SliceSimulation::new(config(0)).unwrap();
        original.apply(Operation::HostElapsed(90_000)).unwrap();
        original.apply(Operation::SetScale(TimeScale::X10)).unwrap();
        advance(&mut original, 5);
        let (normal, stopped) = SliceSimulation::replay(config(0), original.operations()).unwrap();
        assert!(!stopped);
        assert_eq!(normal.checkpoint(), original.checkpoint());
        assert_eq!(normal.operation_digest(), original.operation_digest());
        original.fail_tick = Some(original.clock.steps() + 3);
        original
            .apply(if host {
                Operation::HostElapsed(100_000)
            } else {
                Operation::Steps(10)
            })
            .unwrap();
        let (replayed, stopped) =
            SliceSimulation::replay(config(0), original.operations()).unwrap();
        assert!(stopped);
        assert_eq!(replayed.checkpoint(), original.checkpoint());
        assert_eq!(replayed.operation_digest(), original.operation_digest());
        assert_eq!(replayed.operations(), original.operations());
        let mut corrupt = original.operations().to_vec();
        corrupt.last_mut().unwrap().stop.as_mut().unwrap().state ^= 1;
        assert!(matches!(
            SliceSimulation::replay(config(0), &corrupt),
            Err(ReplayError::Record(_))
        ));
        let mut suffix = original.operations().to_vec();
        suffix.push(suffix[0]);
        assert!(matches!(
            SliceSimulation::replay(config(0), &suffix),
            Err(ReplayError::Record(_))
        ));
    }
}

#[test]
fn old_hash_regressions_and_new_vitals_envelope_are_independent() {
    for (seed, legacy) in [
        (0, 12243142442886412300),
        (42, 4644431189437526967),
        (71449, 15645245484537560688),
        (u64::MAX, 12037866834627665270),
    ] {
        let w =
            SliceSimulation::new(SliceConfig::calendar_aligned_v1(seed, SimTime::EPOCH).unwrap())
                .unwrap();
        assert_eq!(w.legacy_state_hash(), legacy);
        assert_eq!(
            w.state_hash(),
            ws_rng::hash(
                0,
                Purpose::new("ws-sim/slice-state/v2").value(),
                &[legacy, 1, w.vitals().state_hash()]
            )
        );
        assert_ne!(w.state_hash(), legacy);
    }
}
#[test]
fn retained_vitals_survive_steps_camera_lod_queries_and_operation_replay() {
    let mut w = SliceSimulation::new(config(0)).unwrap();
    let original = w.vitals.clone();
    let resident = w.vitals.ids()[0];
    for operation in [
        Operation::SetDetail {
            resident,
            level: Detail::L1,
        },
        Operation::Steps(100),
        Operation::SetScale(TimeScale::X100),
        Operation::CameraInterest(CameraInterest {
            center: ws_life::navigation::Point { east: 0, north: 0 },
            radius_mm: 1_000_000_000,
        }),
        Operation::Steps(100),
    ] {
        assert!(w.apply(operation).unwrap().error.is_none());
        assert_eq!(w.vitals, original);
        let before = (w.checkpoint(), w.operation_digest());
        for &id in w.vitals.ids() {
            assert!(w.inspect(id).is_ok());
        }
        assert_eq!((w.checkpoint(), w.operation_digest()), before);
    }
    let (mut replay, stopped) = SliceSimulation::replay(w.config(), w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replay.vitals, original);
    assert_eq!(replay.checkpoint(), w.checkpoint());
    w.apply(Operation::Steps(50)).unwrap();
    replay.apply(Operation::Steps(50)).unwrap();
    assert_eq!(replay.checkpoint(), w.checkpoint());
}
