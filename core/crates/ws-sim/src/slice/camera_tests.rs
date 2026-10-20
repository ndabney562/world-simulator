use super::*;
use ws_life::{behavior::FineLocation, lod::Mode, navigation::Point};
fn config(start: i64) -> SliceConfig {
    SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(start),
        schedule_epoch: SimTime::EPOCH,
    }
}
fn wide() -> CameraInterest {
    CameraInterest {
        center: Point { east: 0, north: 0 },
        radius_mm: 1_000_000_000,
    }
}
fn overview() -> CameraInterest {
    CameraInterest {
        radius_mm: 0,
        ..wide()
    }
}
fn apply(w: &mut SliceSimulation, op: Operation) {
    assert!(w.apply(op).unwrap().error.is_none());
}
fn steps(w: &mut SliceSimulation, n: u64) {
    assert_eq!(
        w.apply(Operation::Steps(n)).unwrap(),
        Report {
            committed: n,
            error: None
        }
    );
}
#[test]
fn camera_ownership_rejects_manual_changes_before_first_policy_tick_and_preserves_bulk_views() {
    let cfg = config(0);
    let mut w = SliceSimulation::new(cfg).unwrap();
    let id = w.population.residents().ids()[0];
    apply(
        &mut w,
        Operation::SetDetail {
            resident: id,
            level: Detail::L1,
        },
    );
    let before = w.resident(id).unwrap();
    let events = w.events.digest();
    apply(&mut w, Operation::CameraInterest(overview()));
    // Ownership is stored now; initial node modes and diagnostic views wait for
    // the first physical membership plan. No L3 node exposes fine components.
    assert_eq!(
        w.view()
            .unwrap()
            .residents
            .iter()
            .find(|r| r.resident == id),
        Some(&before)
    );
    assert!(w.lod.nodes().iter().all(|n| n.mode == Mode::L2));
    assert_eq!(w.events.digest(), events);
    let check = w.checkpoint();
    assert_eq!(
        w.apply(Operation::SetDetail {
            resident: id,
            level: Detail::L1
        })
        .unwrap()
        .error,
        Some(SliceError::Lod(LodError::CameraOwned))
    );
    assert_eq!(
        (w.checkpoint().state, w.checkpoint().events, w.clock.now()),
        (check.state, check.events, SimTime::EPOCH)
    );
    let (replay, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
    assert!(stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
    assert_eq!(w.view().unwrap(), replay.view().unwrap());
    // Explicit resume is outside that terminal replay input.
    steps(&mut w, 1);
    assert_eq!(w.resident(id).unwrap().detail, Detail::L2);
    steps(&mut w, 1);
    let view = w.view().unwrap();
    assert!(view.summary.buildings.iter().all(|b| b.mode == Mode::L3));
    assert!(view.residents.iter().all(|r| r.fine.is_none()));
}
#[test]
fn invalid_camera_is_recorded_without_mutation_and_idempotence_does_not_restart_dwell() {
    let cfg = config(0);
    let mut w = SliceSimulation::new(cfg).unwrap();
    let before = w.checkpoint();
    let invalid = CameraInterest {
        center: Point {
            east: i64::MAX,
            north: 0,
        },
        ..wide()
    };
    assert_eq!(
        w.apply(Operation::CameraInterest(invalid)).unwrap().error,
        Some(SliceError::Lod(LodError::Input))
    );
    assert_eq!(
        (w.checkpoint().state, w.checkpoint().events),
        (before.state, before.events)
    );
    let (replay, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
    assert!(stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
    let mut w = SliceSimulation::new(cfg).unwrap();
    apply(&mut w, Operation::CameraInterest(wide()));
    steps(&mut w, 8);
    let before = w.checkpoint();
    let digest = w.operation_digest();
    apply(&mut w, Operation::CameraInterest(wide()));
    assert_eq!(w.checkpoint().state, before.state);
    assert_ne!(w.operation_digest(), digest);
    steps(&mut w, 2);
    assert!(
        w.lod
            .nodes()
            .iter()
            .all(|n| n.mode == Mode::L1
                && n.changed_at == SimTime::from_micros_since_epoch(1_000_000))
    );
    let before = w.checkpoint();
    let count = w.operations.len();
    let view = w.view().unwrap();
    assert_eq!(before, w.checkpoint());
    assert_eq!(count, w.operations.len());
    w.discard_observation_cache();
    assert_eq!(view, w.view().unwrap());
    assert_eq!(before, w.checkpoint());
}
#[test]
fn planned_common_needs_controller_cache_and_facts_rollback_and_replay_exact_resource_prefix() {
    for history in [false, true] {
        let cfg = config(58_000_000);
        let mut w = SliceSimulation::new(cfg).unwrap();
        apply(&mut w, Operation::CameraInterest(wide()));
        steps(&mut w, 19);
        apply(&mut w, Operation::CameraInterest(overview()));
        let before = w.checkpoint();
        let view = w.view().unwrap();
        if history {
            w.fail_after_behavior_tick = Some(20);
        } else {
            w.fail_controller_tick = Some(20);
        }
        let error = if history {
            SliceError::History(EventError::Resource)
        } else {
            SliceError::Lod(LodError::Resource)
        };
        assert_eq!(
            w.apply(Operation::Steps(5)).unwrap(),
            Report {
                committed: 0,
                error: Some(error)
            }
        );
        assert_eq!(
            (
                w.checkpoint().tick,
                w.checkpoint().state,
                w.checkpoint().events
            ),
            (before.tick, before.state, before.events)
        );
        assert_eq!(w.view().unwrap(), view);
        assert!(
            w.population
                .residents()
                .ids()
                .iter()
                .all(|&id| w.lod.observation(id, w.clock.now()).is_none())
        );
        let (replay, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
        assert!(stopped);
        assert_eq!(w.checkpoint(), replay.checkpoint());
        assert_eq!(view, replay.view().unwrap());
        w.fail_after_behavior_tick = None;
        w.fail_controller_tick = None;
        steps(&mut w, 1);
        assert!(
            w.view()
                .unwrap()
                .residents
                .iter()
                .all(|r| r.detail == Detail::L2)
        );
        assert!(
            w.population
                .residents()
                .ids()
                .iter()
                .any(|&id| w.lod.observation(id, w.clock.now()).is_some())
        );
    }
    // A committed physical prefix followed by a controller resource stop is
    // retained identically by the same terminal replay contract.
    for operation in [Operation::Steps(20), Operation::HostElapsed(250_123)] {
        let cfg = config(0);
        let mut w = SliceSimulation::new(cfg).unwrap();
        apply(&mut w, Operation::SetScale(TimeScale::X10));
        apply(&mut w, Operation::CameraInterest(wide()));
        w.fail_controller_tick = Some(12);
        assert_eq!(
            w.apply(operation).unwrap(),
            Report {
                committed: 11,
                error: Some(SliceError::Lod(LodError::Resource))
            }
        );
        let (replay, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
        assert!(stopped);
        assert_eq!(w.checkpoint(), replay.checkpoint());
        assert_eq!(w.operation_digest(), replay.operation_digest());
        assert_eq!(w.view().unwrap(), replay.view().unwrap());
    }
}
#[test]
fn simultaneous_schedule_and_lod_facts_share_one_tick_commit_in_producer_order() {
    let probe = SliceSimulation::new(config(0)).unwrap();
    let boundary = probe
        .schedules
        .next_change(SimTime::EPOCH)
        .unwrap()
        .as_micros_since_epoch();
    let mut w = SliceSimulation::new(config(boundary - 2_000_000)).unwrap();
    apply(&mut w, Operation::CameraInterest(wide()));
    steps(&mut w, 19);
    apply(&mut w, Operation::CameraInterest(overview()));
    steps(&mut w, 1);
    let facts = w
        .events
        .records()
        .iter()
        .filter(|e| e.record.time.as_micros_since_epoch() == boundary)
        .collect::<Vec<_>>();
    assert!(
        facts
            .iter()
            .any(|e| matches!(e.record.payload, Payload::ScheduleChanged { .. }))
    );
    assert!(
        facts
            .iter()
            .any(|e| matches!(e.record.payload, Payload::SliceLodChanged { .. }))
    );
    assert!(
        facts
            .windows(2)
            .all(|w| w[0].record.system <= w[1].record.system)
    );
    assert!(
        facts
            .iter()
            .all(|e| e.ordinal == facts[0].ordinal && e.tick == 20)
    );
    assert!(
        facts
            .iter()
            .filter(|e| e.record.system == 70)
            .all(|e| e.record.origin == Origin::System(70) && e.record.causes.is_empty())
    );
    let mut known = w.population.world().buildings().ids().to_vec();
    known.extend_from_slice(w.population.households().ids());
    known.extend_from_slice(w.population.residents().ids());
    known.extend_from_slice(w.population.workplaces().ids());
    let rebuilt = EventStore::from_records(known, w.events.records()).unwrap();
    assert_eq!(rebuilt.digest(), w.events.digest());
    for &id in w.population.world().buildings().ids() {
        assert_eq!(
            rebuilt.query(Some(id), SimTime::MIN, SimTime::MAX),
            w.events.query(Some(id), SimTime::MIN, SimTime::MAX)
        );
    }
}
#[test]
fn camera_membership_and_facts_are_independent_of_physical_step_batch_size() {
    let cfg = config(7 * 3_600_000_000 + 35 * 60_000_000);
    let mut a = SliceSimulation::new(cfg).unwrap();
    let mut b = a.clone();
    for camera in [
        wide(),
        overview(),
        CameraInterest {
            center: Point {
                east: 150_000,
                north: 40_000,
            },
            radius_mm: 70_000,
        },
    ] {
        apply(&mut a, Operation::CameraInterest(camera));
        apply(&mut b, Operation::CameraInterest(camera));
        steps(&mut a, 1200);
        for _ in 0..1200 {
            b.step().unwrap();
        }
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.events.records(), b.events.records());
        assert_eq!(a.view().unwrap(), b.view().unwrap());
    }
}

#[test]
fn a_thousand_actual_camera_operations_conserve_the_bulk_population_and_replay_full_history() {
    let cfg = config(7 * 3_600_000_000);
    let mut w = SliceSimulation::new(cfg).unwrap();
    let mut reference = w.behavior.clone();
    let ids = w.population.residents().ids().to_vec();
    let rng = ws_rng::Stream::new(42, Purpose::new("test/actual-slice-camera-cycles"), &[]);
    for cycle in 0..1000u64 {
        let k = cycle * 4;
        let input = CameraInterest {
            center: Point {
                east: (rng.draw(k) % 600_000) as i64 - 200_000,
                north: (rng.draw(k + 1) % 500_000) as i64 - 200_000,
            },
            radius_mm: [0, 50_000, 100_000, 1_000_000_000][(rng.draw(k + 2) % 4) as usize],
        };
        apply(&mut w, Operation::CameraInterest(input));
        steps(&mut w, 1 + rng.draw(k + 3) % 15);
        reference.advance_to(w.clock.now()).unwrap();
        let view = w.view().unwrap();
        assert!(
            view.residents
                .iter()
                .map(|r| r.resident)
                .eq(ids.iter().copied())
        );
        assert_eq!(view.summary.population, ids.len() as u32);
        assert_eq!(view.summary.l1 + view.summary.l2, ids.len() as u32);
        for r in view.residents {
            let expected = reference.common(r.resident, w.clock.now()).unwrap();
            assert_eq!(
                (r.needs, r.action, r.decisions, r.logical),
                (
                    expected.needs,
                    expected.action,
                    expected.decisions,
                    expected.logical
                )
            );
            if let Some(fine) = r.fine {
                assert_eq!(
                    fine,
                    reference
                        .projected_location(r.resident, w.clock.now())
                        .unwrap()
                );
            }
        }
    }
    let (replay, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
    assert_eq!(w.operation_digest(), replay.operation_digest());
    assert_eq!(w.events.records(), replay.events.records());
    assert_eq!(w.view().unwrap(), replay.view().unwrap());
}
fn script() -> u64 {
    let cfg = config(7 * 3_600_000_000 + 35 * 60_000_000);
    let mut w = SliceSimulation::new(cfg).unwrap();
    let mut checkpoints = Vec::new();
    for operation in [
        Operation::CameraInterest(wide()),
        Operation::Steps(2000),
        Operation::CameraInterest(overview()),
        Operation::Steps(3),
        Operation::CameraInterest(CameraInterest {
            center: Point {
                east: 150_000,
                north: 40_000,
            },
            radius_mm: 70_000,
        }),
        Operation::Steps(21),
        Operation::SetScale(TimeScale::X100),
        Operation::HostElapsed(50_000),
        Operation::SetScale(TimeScale::Paused),
        Operation::HostElapsed(1_000_000),
        Operation::CameraInterest(wide()),
        Operation::SetScale(TimeScale::X1),
        Operation::HostElapsed(100_000),
    ] {
        apply(&mut w, operation);
        checkpoints.push((w.checkpoint(), w.operation_digest()));
        let (prefix, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
        assert!(!stopped);
        assert_eq!(w.checkpoint(), prefix.checkpoint());
        assert_eq!(w.operation_digest(), prefix.operation_digest());
        assert_eq!(w.view().unwrap(), prefix.view().unwrap());
    }
    let (replay, stopped) = SliceSimulation::replay(cfg, w.operations()).unwrap();
    assert!(!stopped);
    assert_eq!(w.checkpoint(), replay.checkpoint());
    assert_eq!(w.operation_digest(), replay.operation_digest());
    assert_eq!(w.view().unwrap(), replay.view().unwrap());
    let mut altered = w.operations().to_vec();
    altered[0].operation = Operation::CameraInterest(overview());
    // A different valid input is a different replay, not a malformed outcome.
    // External expected operation/history digests distinguish the two scripts.
    let (altered, stopped) = SliceSimulation::replay(cfg, &altered).unwrap();
    assert!(!stopped);
    assert_ne!(altered.operation_digest(), w.operation_digest());
    assert_ne!(altered.events.digest(), w.events.digest());
    let mut h = SipHasher24::new(0, Purpose::new("test/camera-script").value());
    for (c, operations) in checkpoints {
        h.write_words(&[c.tick, c.state, c.events, operations]);
    }
    for r in w.view().unwrap().residents {
        h.write_words(&[r.resident.raw(), r.detail as u64, r.decisions]);
        for need in r.needs.values() {
            h.write_u64(u64::from(need));
        }
        if let Some(FineLocation::Local { point, walking }) = r.fine {
            h.write_words(&[point.east as u64, point.north as u64, u64::from(walking)]);
        }
    }
    let before = w.checkpoint();
    let view = w.view().unwrap();
    w.discard_observation_cache();
    assert_eq!(w.checkpoint(), before);
    assert_eq!(w.view().unwrap(), view);
    h.finish()
}
#[test]
fn camera_replay_child() {
    if std::env::var_os("WS_CAMERA_REPLAY_CHILD").is_some() {
        println!("CAMERA_REPLAY:{:016x}", script());
    }
}
#[test]
fn actual_camera_host_pacing_views_and_histories_replay_in_another_process() {
    let expected = script();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "slice::camera_tests::camera_replay_child",
            "--nocapture",
        ])
        .env("WS_CAMERA_REPLAY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&format!("CAMERA_REPLAY:{expected:016x}"))
    );
}
