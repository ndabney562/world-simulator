use super::*;
use std::collections::BTreeMap;
use ws_life::{Place, behavior::FineLocation};
const STEP: i64 = 100_000;
const DAY: i64 = 86_400_000_000;
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
fn detail(w: &mut SliceSimulation, id: EntityId, level: Detail) {
    let events = w.events.digest();
    let time = w.clock.now();
    assert_eq!(
        w.apply(Operation::SetDetail {
            resident: id,
            level
        })
        .unwrap(),
        Report {
            committed: 0,
            error: None
        }
    );
    assert_eq!(w.events.digest(), events);
    assert_eq!(w.clock.now(), time);
}
#[test]
fn real_week_in_l1_reaches_every_local_and_off_map_destination_at_its_deadline() {
    let mut w = SliceSimulation::new(config(0)).unwrap();
    let ids = w.population.residents().ids().to_vec();
    for &id in &ids {
        detail(&mut w, id, Detail::L1);
    }
    let mut arrivals = BTreeMap::<i64, Vec<(EntityId, Place)>>::new();
    for day in 0..7i64 {
        for profile in w.schedules.profiles() {
            let rows = if day % 7 < 5 {
                profile.weekday()
            } else {
                profile.weekend()
            };
            for row in rows {
                if let Some(at) = row.arrival_minute {
                    arrivals
                        .entry(day * DAY + i64::from(at) * 60_000_000)
                        .or_default()
                        .push((profile.resident, row.destination));
                }
            }
        }
    }
    let mut local = 0;
    let mut remote = 0;
    for (time, people) in arrivals {
        let before = time - STEP;
        let count = ((before - w.clock.now().as_micros_since_epoch()) / STEP) as u64;
        advance(&mut w, count);
        for &(id, _) in &people {
            let snapshot = w.resident(id).unwrap();
            assert_eq!(snapshot.logical.activity, ws_events::Activity::Commute);
            assert!(snapshot.fine.is_some());
        }
        w.step().unwrap();
        for (id, destination) in people {
            let snap = w.resident(id).unwrap();
            assert_eq!(snap.detail, Detail::L1);
            match destination {
                Place::Building(building) => {
                    local += 1;
                    assert_eq!(
                        snap.logical.location,
                        ws_life::Location::AtBuilding(building)
                    );
                    assert_eq!(
                        snap.fine,
                        Some(FineLocation::Local {
                            point: w
                                .behavior
                                .navigation()
                                .position(destination, destination, 0)
                                .unwrap(),
                            walking: false
                        })
                    );
                }
                Place::OffMap(workplace, exit) => {
                    remote += 1;
                    assert_eq!(
                        snap.logical.location,
                        ws_life::Location::OffMap(workplace, exit)
                    );
                    assert_eq!(snap.fine, Some(FineLocation::OffMap { workplace, exit }));
                }
            }
            assert!(w.events.records().iter().rev().take(ids.len()).any(|e|e.record.time==w.clock.now()&&matches!(e.record.payload,Payload::ScheduleChanged{resident,..} if resident==id)));
        }
        w.step().unwrap();
    }
    let rest = ((7 * DAY - w.clock.now().as_micros_since_epoch()) / STEP) as u64;
    advance(&mut w, rest);
    assert!(local > 0 && remote > 0);
    assert_eq!(w.behavior.residents().collect::<Vec<_>>(), ids);
    assert!(
        ids.iter()
            .all(|id| w.resident(*id).unwrap().detail == Detail::L1)
    );
}
#[test]
fn detail_round_trip_keeps_state_hash_need_progress_positions_and_history() {
    let mut w = SliceSimulation::new(config(7 * 3_600_000_000 + 40 * 60_000_000)).unwrap();
    let ids = w.population.residents().ids().to_vec();
    for &id in &ids {
        detail(&mut w, id, Detail::L1);
    }
    advance(&mut w, 137);
    let before = w.checkpoint();
    let snapshots = ids
        .iter()
        .map(|id| w.resident(*id).unwrap())
        .collect::<Vec<_>>();
    for &id in &ids {
        detail(&mut w, id, Detail::L2);
    }
    for &id in &ids {
        detail(&mut w, id, Detail::L1);
    }
    assert_eq!(w.checkpoint(), before);
    assert_eq!(
        ids.iter()
            .map(|id| w.resident(*id).unwrap())
            .collect::<Vec<_>>(),
        snapshots
    );
    let (replayed, stopped) = SliceSimulation::replay(w.config, w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(w.checkpoint(), replayed.checkpoint());
    assert_eq!(w.operation_digest(), replayed.operation_digest());
}
#[test]
fn domain_and_later_history_failure_drop_prepared_behavior_and_replay_exact_prefix() {
    let probe = SliceSimulation::new(config(0)).unwrap();
    let boundary = probe.next_change.unwrap().as_micros_since_epoch();
    for domain in [true, false] {
        for host in [true, false] {
            let cfg = config(boundary - 2 * STEP);
            let mut w = SliceSimulation::new(cfg).unwrap();
            let id = w.population.residents().ids()[0];
            detail(&mut w, id, Detail::L1);
            if host {
                w.apply(Operation::HostElapsed(90_000)).unwrap();
            }
            let mut prefix = w.clone();
            advance(&mut prefix, 1);
            if domain {
                w.fail_behavior_tick = Some(2);
            } else {
                w.fail_after_behavior_tick = Some(2);
            }
            let report = w
                .apply(if host {
                    Operation::HostElapsed(400_000)
                } else {
                    Operation::Steps(4)
                })
                .unwrap();
            assert_eq!(report.committed, 1);
            assert_eq!(
                report.error,
                Some(if domain {
                    SliceError::Behavior(BehaviorError::Resource)
                } else {
                    SliceError::History(EventError::Resource)
                })
            );
            assert_eq!(w.checkpoint(), prefix.checkpoint());
            assert_eq!(w.behavior.state_hash(), prefix.behavior.state_hash());
            assert_eq!(w.events.records(), prefix.events.records());
            let (replayed, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
            assert!(stopped);
            assert_eq!(replayed.checkpoint(), w.checkpoint());
            assert_eq!(replayed.operation_digest(), w.operation_digest());
            w.fail_behavior_tick = None;
            w.fail_after_behavior_tick = None;
            advance(&mut w, 3);
            let mut success = SliceSimulation::new(cfg).unwrap();
            detail(&mut success, id, Detail::L1);
            if host {
                success.apply(Operation::HostElapsed(90_000)).unwrap();
            }
            advance(&mut success, 4);
            assert_eq!(w.checkpoint(), success.checkpoint());
            assert_eq!(w.behavior.state_hash(), success.behavior.state_hash());
            assert_eq!(w.events.records(), success.events.records());
        }
    }
}
#[test]
fn unknown_detail_request_records_rejection_without_mutating_state_or_facts() {
    let cfg = config(0);
    let mut w = SliceSimulation::new(cfg).unwrap();
    let before = w.checkpoint();
    let report = w
        .apply(Operation::SetDetail {
            resident: EntityId::procedural(0),
            level: Detail::L1,
        })
        .unwrap();
    assert_eq!(
        report,
        Report {
            committed: 0,
            error: Some(SliceError::Behavior(BehaviorError::UnknownResident))
        }
    );
    assert_eq!(w.checkpoint(), before);
    assert_eq!(w.operations.last().unwrap().stop, Some(before));
    let (replayed, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
    assert!(stopped);
    assert_eq!(replayed.checkpoint(), before);
    assert_eq!(replayed.operation_digest(), w.operation_digest());
}
fn script() -> u64 {
    let cfg = config(7 * 3_600_000_000 + 40 * 60_000_000);
    let mut w = SliceSimulation::new(cfg).unwrap();
    let id = w.population.residents().ids()[0];
    let mut hash = SipHasher24::new(69, 1);
    for op in [
        Operation::SetDetail {
            resident: id,
            level: Detail::L1,
        },
        Operation::Steps(600),
        Operation::SetScale(TimeScale::Paused),
        Operation::HostElapsed(u64::MAX),
        Operation::SetDetail {
            resident: id,
            level: Detail::L2,
        },
        Operation::Steps(1),
        Operation::SetDetail {
            resident: id,
            level: Detail::L1,
        },
        Operation::SetScale(TimeScale::X100),
        Operation::HostElapsed(123_456),
        Operation::Steps(18_000),
    ] {
        assert!(w.apply(op).unwrap().error.is_none());
        let p = w.checkpoint();
        hash.write_words(&[
            p.tick,
            p.state,
            p.events,
            p.pending_micros,
            w.operation_digest(),
        ]);
    }
    for _ in 0..601 {
        w.step().unwrap();
        let p = w.checkpoint();
        let s = w.resident(id).unwrap();
        hash.write_words(&[
            p.tick,
            p.state,
            p.events,
            w.operation_digest(),
            s.action as u64,
            s.decisions,
        ]);
        for v in s.needs.values() {
            hash.write_u64(u64::from(v));
        }
        match s.fine.unwrap() {
            FineLocation::Local { point, walking } => {
                hash.write_words(&[0, point.east as u64, point.north as u64, u64::from(walking)])
            }
            FineLocation::OffMapTransit { workplace, exit } => {
                hash.write_words(&[1, workplace.raw(), u64::from(exit)])
            }
            FineLocation::OffMap { workplace, exit } => {
                hash.write_words(&[2, workplace.raw(), u64::from(exit)])
            }
        }
    }
    let (replayed, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(replayed.checkpoint(), w.checkpoint());
    assert_eq!(replayed.operation_digest(), w.operation_digest());
    hash.finish()
}
#[test]
fn active_replay_child() {
    if std::env::var_os("WS_ACTIVE_REPLAY_CHILD").is_some() {
        println!("active_digest={:016x}", script());
    }
}
#[test]
fn typed_active_script_replays_actual_positions_needs_and_histories_in_another_process() {
    let expected = script();
    assert_eq!(expected, script());
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "slice::active_tests::active_replay_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WS_ACTIVE_REPLAY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let (_, hex) = stdout.split_once("active_digest=").unwrap();
    assert_eq!(u64::from_str_radix(&hex[..16], 16).unwrap(), expected);
}
