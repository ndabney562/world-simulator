//! Integration tests for slice persistence: format 4 (ADR 0036, Issue #75) and
//! format 5 with storm and gathering episodes (ADR 0038 §6, Issue #125).

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use ws_life::lod::CameraInterest;
use ws_persist::{
    FORMAT_VERSION, LoadError, MODEL_KIND_SLICE, MODEL_KIND_TOY, SaveError, load, load_slice,
    read_header, save_slice, save_slice_background, save_slice_with_version,
};
use ws_sim::slice::{Operation, SliceConfig, SliceSimulation};
use ws_sim::ws_events::EventId;
use ws_sim::{EntityId, SimTime};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ws-persist-slice-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, file: &str) -> PathBuf {
        self.0.join(file)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/saves")
        .join(name)
}

fn sample_slice(seed: u64) -> SliceSimulation {
    let config =
        SliceConfig::calendar_aligned_v1(seed, SimTime::from_micros_since_epoch(1_000_000_000))
            .unwrap();
    SliceSimulation::new(config).unwrap()
}

#[test]
fn fresh_slice_at_tick_zero_round_trips() {
    let dir = Scratch::new("fresh");
    let path = dir.path("fresh.wssave");
    let sim = sample_slice(42);

    save_slice(&sim, &path).unwrap();

    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 5);
    assert_eq!(h.model_kind, MODEL_KIND_SLICE);
    assert!(h.is_slice());
    assert!(!h.is_toy());
    assert_eq!(h.tick, 0);
    assert_eq!(h.state_hash, sim.state_hash());

    let loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.checkpoint(), sim.checkpoint());
    assert_eq!(loaded.operation_digest(), sim.operation_digest());
    assert_eq!(loaded.events().digest(), sim.events().digest());
}

#[test]
fn slice_advanced_with_illness_and_lod_round_trips_and_steps_identically() {
    let dir = Scratch::new("active");
    let path = dir.path("active.wssave");
    let mut sim = sample_slice(101);

    // Step 30 ticks
    sim.apply(Operation::Steps(30)).unwrap();

    // Set camera interest to activate fine LOD on buildings near the camera
    let cam = CameraInterest {
        center: ws_life::navigation::Point {
            east: 20_000,
            north: 30_000,
        },
        radius_mm: 50_000,
    };
    sim.apply(Operation::CameraInterest(cam)).unwrap();

    // Afflict resident
    let resident = sim.population().residents().ids()[0];
    sim.apply(Operation::Afflict {
        resident,
        severity: 5,
        duration_us: 15_000_000,
    })
    .unwrap();

    let root = sim.to_parts().episodes[0].root;

    // Step 40 more ticks
    sim.apply(Operation::Steps(40)).unwrap();

    // Query consequences before save
    let (group_before, events_before) = sim.consequences(root).unwrap();
    let group_before_ids = group_before.ids().to_vec();
    let events_before_ids: Vec<_> = events_before.iter().map(|e| e.id).collect();

    // Save
    save_slice(&sim, &path).unwrap();

    // Header inspection
    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, 5);
    assert_eq!(h.model_kind, MODEL_KIND_SLICE);
    assert_eq!(h.tick, 70);
    assert_eq!(h.state_hash, sim.state_hash());

    // Load
    let mut loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.checkpoint(), sim.checkpoint());
    assert_eq!(loaded.operation_digest(), sim.operation_digest());
    assert_eq!(loaded.events().digest(), sim.events().digest());

    // Verify consequence query on loaded simulation (rebuilt derived consequence index)
    let (group_after, events_after) = loaded.consequences(root).unwrap();
    assert_eq!(group_after.ids(), &group_before_ids[..]);
    let events_after_ids: Vec<_> = events_after.iter().map(|e| e.id).collect();
    assert_eq!(events_after_ids, events_before_ids);

    // Continue stepping both 100 ticks and assert state hashes agree on every tick
    for _ in 0..100 {
        sim.step().unwrap();
        loaded.step().unwrap();
        assert_eq!(loaded.state_hash(), sim.state_hash());
        assert_eq!(loaded.checkpoint(), sim.checkpoint());
    }
}

#[test]
fn background_save_writes_atomically_and_matches() {
    let dir = Scratch::new("background");
    let path = dir.path("bg.wssave");
    let mut sim = sample_slice(777);

    sim.apply(Operation::Steps(25)).unwrap();

    let parts = sim.to_parts();
    let handle = save_slice_background(
        parts,
        sim.state_hash(),
        sim.operation_digest(),
        sim.events().digest(),
        &path,
    );
    handle.join().unwrap().unwrap();

    assert!(path.is_file());
    let sibling = dir.path("bg.wssave.saving");
    assert!(
        !sibling.exists(),
        "temporary sibling file must be cleaned up"
    );

    let loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), sim.state_hash());
}

#[test]
fn corrupt_or_tampered_slice_saves_are_rejected() {
    let dir = Scratch::new("tampered");
    let path = dir.path("t.wssave");
    let mut sim = sample_slice(888);
    sim.apply(Operation::Steps(10)).unwrap();
    save_slice(&sim, &path).unwrap();

    // 1. Tampered state hash in header
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE header SET state_hash = state_hash + 1 WHERE id = 1",
            [],
        )
        .unwrap();
    }
    match load_slice(&path) {
        Err(LoadError::HashMismatch { what, .. }) => assert_eq!(what, "state"),
        other => panic!("expected HashMismatch state, got {other:?}"),
    }

    // 2. Restore state hash, tamper events digest
    save_slice(&sim, &path).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE header SET events_digest = events_digest + 1 WHERE id = 1",
            [],
        )
        .unwrap();
    }
    match load_slice(&path) {
        Err(LoadError::HashMismatch { what, .. }) => assert_eq!(what, "events"),
        other => panic!("expected HashMismatch events, got {other:?}"),
    }

    // 3. Restore, tamper operation journal digest
    save_slice(&sim, &path).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE header SET journal_digest = journal_digest + 1 WHERE id = 1",
            [],
        )
        .unwrap();
    }
    match load_slice(&path) {
        Err(LoadError::HashMismatch { what, .. }) => assert_eq!(what, "journal"),
        other => panic!("expected HashMismatch journal, got {other:?}"),
    }

    // 4. Missing required slice table
    save_slice(&sim, &path).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute("DROP TABLE slice_vitals", []).unwrap();
    }
    assert!(matches!(
        load_slice(&path),
        Err(LoadError::Corrupt {
            table: "slice_vitals",
            ..
        })
    ));

    // 5. Calling toy `load` on slice file fails with typed error
    save_slice(&sim, &path).unwrap();
    match load(&path) {
        Err(LoadError::Corrupt { table, reason }) => {
            assert_eq!(table, "header");
            assert!(reason.contains("save file is a slice simulation"));
        }
        other => panic!("expected Corrupt header for slice file in toy loader, got {other:?}"),
    }

    // 6. Calling `load_slice` on a toy save file fails
    let toy_path = fixture("toy-v3.wssave");
    match load_slice(&toy_path) {
        Err(LoadError::Corrupt { table, reason }) => {
            assert_eq!(table, "header");
            assert!(reason.contains("save file is not a slice simulation"));
        }
        other => panic!("expected Corrupt header for toy file in slice loader, got {other:?}"),
    }
}

#[test]
fn the_slice_v4_golden_fixture_loads() {
    let path = fixture("slice-v4.wssave");
    assert!(path.is_file(), "missing golden fixture slice-v4.wssave");

    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, 4);
    assert_eq!(h.model_kind, MODEL_KIND_SLICE);
    assert_eq!(h.tick, 100);
    assert_eq!(h.seed, 2026);
    assert_eq!(h.state_hash, 0xc2aec0c3bd1ccfda);

    let mut loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), 0xc2aec0c3bd1ccfda);
    assert_eq!(loaded.clock().steps(), 100);

    // Step 20 more ticks
    for _ in 0..20 {
        loaded.step().unwrap();
    }
    assert_eq!(loaded.clock().steps(), 120);
}

#[test]
fn older_fixtures_load_unchanged() {
    for name in ["toy-v1.wssave", "toy-v2.wssave", "toy-v3.wssave"] {
        let path = fixture(name);
        assert!(path.is_file(), "missing fixture {name}");
        let h = read_header(&path).unwrap();
        assert_eq!(h.model_kind, MODEL_KIND_TOY);
        assert!(!h.is_slice());
        assert!(h.is_toy());
        let w = load(&path).unwrap();
        assert_eq!(w.state_hash(), h.state_hash);
    }
}

#[test]
fn irregular_host_pacing_and_pause_round_trip_retains_operation_digest() {
    let dir = Scratch::new("host-pacing");
    let path = dir.path("host-pacing.wssave");
    let mut sim = sample_slice(71449);
    for operation in [
        Operation::HostElapsed(50_003),
        Operation::HostElapsed(53_127),
        Operation::SetScale(ws_sim::TimeScale::X10),
        Operation::HostElapsed(58_019),
        Operation::SetScale(ws_sim::TimeScale::Paused),
        Operation::HostElapsed(u64::MAX),
        Operation::SetScale(ws_sim::TimeScale::X1),
        Operation::HostElapsed(37_123),
        Operation::SetScale(ws_sim::TimeScale::X100),
        Operation::HostElapsed(2_000_127),
        Operation::Steps(3),
        Operation::SetScale(ws_sim::TimeScale::Paused),
    ] {
        let report = sim.apply(operation).unwrap();
        assert!(report.error.is_none());
    }
    save_slice(&sim, &path).unwrap();
    let mut loaded = load_slice(&path).expect("a paced world must load its own save");
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.operation_digest(), sim.operation_digest());
    assert_eq!(loaded.clock(), sim.clock());
    assert_eq!(loaded.operations(), sim.operations());
    assert_eq!(loaded.events().digest(), sim.events().digest());
    for operation in [
        Operation::SetScale(ws_sim::TimeScale::X100),
        Operation::HostElapsed(61_337),
    ] {
        assert_eq!(
            loaded.apply(operation).unwrap(),
            sim.apply(operation).unwrap()
        );
        assert_eq!(loaded.state_hash(), sim.state_hash());
        assert_eq!(loaded.operation_digest(), sim.operation_digest());
        assert_eq!(loaded.clock(), sim.clock());
    }
}

#[test]
fn rejected_command_then_later_commands_can_save_load_and_replay() {
    let dir = Scratch::new("rejected");
    let path = dir.path("rejected.wssave");
    let mut sim = sample_slice(71449);
    let resident = sim.population().residents().ids()[0];
    sim.apply(Operation::HostElapsed(90_001)).unwrap();
    let before = sim.state_hash();
    let report = sim
        .apply(Operation::Afflict {
            resident,
            severity: 0,
            duration_us: 100_000,
        })
        .unwrap();
    assert!(matches!(
        report.error,
        Some(ws_sim::slice::SliceError::Rejected(_))
    ));
    assert_eq!(sim.state_hash(), before);
    assert!(sim.operations().last().unwrap().stop.is_some());
    sim.apply(Operation::HostElapsed(20_123)).unwrap();
    sim.apply(Operation::Steps(3)).unwrap();
    save_slice(&sim, &path).unwrap();
    let loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.operation_digest(), sim.operation_digest());
    assert_eq!(loaded.operations(), sim.operations());
    let (replayed, terminal) = SliceSimulation::replay(sim.config(), sim.operations()).unwrap();
    assert!(!terminal);
    assert_eq!(replayed.state_hash(), sim.state_hash());
    assert_eq!(replayed.operation_digest(), sim.operation_digest());
    let mut legacy = sim.to_parts();
    legacy
        .operations
        .iter_mut()
        .find(|r| r.report.error.is_some())
        .unwrap()
        .stop = None;
    assert!(SliceSimulation::from_parts(legacy).is_err());
}

// ---- Format 5: storm and gathering episodes (ADR 0038 §6) ----

const MINUTE: u64 = 600;
const HOUR_US: u64 = 3_600_000_000;

fn ok(w: &mut SliceSimulation, op: Operation) {
    assert_eq!(w.apply(op).unwrap().error, None, "{op:?}");
}

/// Every slice root's consequence group and its events' digests, in root order.
fn index(w: &SliceSimulation) -> Vec<(ws_sim::slice::illness::Group, Vec<u64>)> {
    w.events()
        .records()
        .iter()
        .filter(|e| e.record.payload.is_slice_root())
        .map(|e| {
            let (g, ev) = w.consequences(e.id).unwrap();
            (*g, ev.iter().map(|e| e.digest()).collect())
        })
        .collect()
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

fn holds(w: &SliceSimulation) -> usize {
    w.to_parts()
        .behavior_states
        .iter()
        .filter(|s| s.storm_hold.is_some())
        .count()
}

fn hold_roots(w: &SliceSimulation) -> Vec<u64> {
    w.to_parts()
        .behavior_states
        .iter()
        .filter_map(|s| s.storm_hold.map(|h| h.root))
        .collect()
}

/// Monday 07:30, so a one-hour storm spans the 08:00 departures.
fn storm_world() -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(7 * 3_600_000_000 + 30 * 60_000_000),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}

/// Monday 21:00, when most residents are home; every resident at L1.
fn gather_world() -> SliceSimulation {
    let mut w = SliceSimulation::new(SliceConfig {
        seed: 42,
        start: SimTime::from_micros_since_epoch(21 * 3_600_000_000),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap();
    for id in w.population().residents().ids().to_vec() {
        ok(
            &mut w,
            Operation::SetDetail {
                resident: id,
                level: ws_life::behavior::Detail::L1,
            },
        );
    }
    w
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

/// Saves `sim` as format 5, loads it, and checks equal hashes, equal episodes,
/// a rebuilt index equal to the live producer's, and equal futures for an hour.
fn save_load_and_step(sim: &mut SliceSimulation, name: &str) {
    let dir = Scratch::new(name);
    let path = dir.path("p.wssave");
    save_slice(sim, &path).unwrap();
    assert_eq!(read_header(&path).unwrap().format_version, 5);
    let mut loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.checkpoint(), sim.checkpoint());
    assert_eq!(loaded.operation_digest(), sim.operation_digest());
    assert_eq!(loaded.events().digest(), sim.events().digest());
    assert_eq!(loaded.storm(), sim.storm());
    assert_eq!(loaded.gathering(), sim.gathering());
    assert_eq!(loaded.to_parts(), sim.to_parts());
    // The live index equals the one rebuilt from committed history on load.
    assert_eq!(index(&loaded), index(sim));
    for _ in 0..6 {
        ok(sim, Operation::Steps(10 * MINUTE));
        ok(&mut loaded, Operation::Steps(10 * MINUTE));
        assert_eq!(loaded.state_hash(), sim.state_hash());
        assert_eq!(loaded.checkpoint(), sim.checkpoint());
    }
    assert_eq!(index(&loaded), index(sim));
}

#[test]
fn format5_round_trips_active_storm_and_gathering() {
    // Mid-storm: departures deferred, holds deferring.
    let mut storm = storm_world();
    ok(
        &mut storm,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    ok(&mut storm, Operation::Steps(40 * MINUTE));
    assert!(storm.storm().is_some_and(|s| s.kept > 0));
    assert!(holds(&storm) > 0);
    // Every hold and the window carry the storm root (D1 as amended), and both
    // survive the round trip.
    let root = storm.storm().unwrap().root.0;
    assert_eq!(hold_roots(&storm), vec![root; holds(&storm)]);
    assert_eq!(storm.to_parts().storm_window.unwrap().root, root);
    let loaded_dir = Scratch::new("f5-storm-root");
    let path = loaded_dir.path("p.wssave");
    save_slice(&storm, &path).unwrap();
    let loaded = load_slice(&path).unwrap();
    assert_eq!(hold_roots(&loaded), hold_roots(&storm));
    assert_eq!(loaded.to_parts().storm_window.unwrap().root, root);
    save_load_and_step(&mut storm, "f5-storm");

    // After the storm's end, while holds still resume: the episode is gone
    // but ws-life keeps its window until every hold has rejoined.
    let mut after = storm_world();
    ok(
        &mut after,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    ok(&mut after, Operation::Steps(60 * MINUTE));
    while after.storm().is_some() {
        ok(&mut after, Operation::Steps(1));
    }
    assert!(holds(&after) > 0, "a hold must still be resuming");
    save_load_and_step(&mut after, "f5-storm-resuming");

    // Mid-gathering: some attendees joined, the stay not yet over.
    let mut gather = gather_world();
    let target = rowhouse(&gather);
    ok(
        &mut gather,
        Operation::Gather {
            building: target,
            duration_us: 30 * 60_000_000,
        },
    );
    ok(&mut gather, Operation::Steps(10 * MINUTE));
    let g = gather.gathering().copied().unwrap();
    assert!(g.joined > 0);
    assert_eq!(g.attendees().len(), 7);
    save_load_and_step(&mut gather, "f5-gather");
}

#[test]
fn format4_save_loads_without_episodes() {
    let dir = Scratch::new("f4-fixture");
    let path = dir.path("v4.wssave");
    let mut sim = sample_slice(101);
    sim.apply(Operation::Steps(30)).unwrap();
    let resident = sim.population().residents().ids()[0];
    sim.apply(Operation::Afflict {
        resident,
        severity: 5,
        duration_us: 15_000_000,
    })
    .unwrap();
    sim.apply(Operation::Steps(40)).unwrap();
    save_slice_with_version(&sim, &path, 4).unwrap();

    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, 4);
    {
        let conn = Connection::open(&path).unwrap();
        for table in ["slice_storm", "slice_gathering"] {
            let present: bool = conn
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = ?)",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(!present, "format 4 has no {table} table");
        }
    }
    let mut loaded = load_slice(&path).unwrap();
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(loaded.storm(), None);
    assert_eq!(loaded.gathering(), None);
    assert_eq!(holds(&loaded), 0);
    assert_eq!(index(&loaded), index(&sim));
    for _ in 0..50 {
        sim.step().unwrap();
        loaded.step().unwrap();
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }

    // The golden format-4 fixture loads with no episodes either.
    let golden = load_slice(&fixture("slice-v4.wssave")).unwrap();
    assert_eq!((golden.storm(), golden.gathering()), (None, None));
    assert_eq!(holds(&golden), 0);

    // A format-4 file cannot hold an active storm or gathering.
    let mut storm = storm_world();
    ok(
        &mut storm,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    assert!(matches!(
        save_slice_with_version(&storm, &dir.path("storm-v4.wssave"), 4),
        Err(SaveError::Unsaveable(_))
    ));
    let mut gather = gather_world();
    let target = rowhouse(&gather);
    ok(
        &mut gather,
        Operation::Gather {
            building: target,
            duration_us: 30 * 60_000_000,
        },
    );
    assert!(matches!(
        save_slice_with_version(&gather, &dir.path("gather-v4.wssave"), 4),
        Err(SaveError::Unsaveable(_))
    ));
    assert!(matches!(
        save_slice_with_version(&sim, &dir.path("v3.wssave"), 3),
        Err(SaveError::Unsaveable(_))
    ));
}

/// ADR 0038 §6: one case per power, saved after the episode completes; the
/// index rebuilt on load equals the live one.
#[test]
fn each_power_saved_after_completion_rebuilds_equal() {
    let check = |sim: &mut SliceSimulation, name: &str| {
        assert_eq!((sim.storm(), sim.gathering()), (None, None));
        let mut rebuilt = sim.clone();
        rebuilt.rebuild_consequence_index().unwrap();
        assert_eq!(index(&rebuilt), index(sim));
        save_load_and_step(sim, name);
    };

    // Bless: a cure of an active illness.
    let mut bless = storm_world();
    let id = bless.population().residents().ids()[0];
    ok(
        &mut bless,
        Operation::Afflict {
            resident: id,
            severity: 4,
            duration_us: 2 * HOUR_US,
        },
    );
    ok(&mut bless, Operation::Steps(10 * MINUTE));
    ok(&mut bless, Operation::Bless { resident: id });
    ok(&mut bless, Operation::Steps(MINUTE));
    let root = last_root(&bless);
    assert_eq!(bless.consequences(root).unwrap().1.len(), 2);
    check(&mut bless, "done-bless");

    // Storm: ended and every hold rejoined.
    let mut storm = storm_world();
    ok(
        &mut storm,
        Operation::Storm {
            duration_us: HOUR_US,
        },
    );
    let root = last_root(&storm);
    ok(&mut storm, Operation::Steps(60 * MINUTE));
    while storm.storm().is_some() || holds(&storm) > 0 {
        ok(&mut storm, Operation::Steps(MINUTE));
    }
    let tags: Vec<_> = storm
        .consequences(root)
        .unwrap()
        .1
        .iter()
        .map(|e| e.record.payload.tag())
        .collect();
    assert_eq!(tags, vec![16, 18, 19]);
    check(&mut storm, "done-storm");

    // Gathering: ended and every attendee home.
    let mut gather = gather_world();
    let target = rowhouse(&gather);
    ok(
        &mut gather,
        Operation::Gather {
            building: target,
            duration_us: 30 * 60_000_000,
        },
    );
    let root = last_root(&gather);
    while gather.gathering().is_some() {
        ok(&mut gather, Operation::Steps(MINUTE));
    }
    let tags: Vec<_> = gather
        .consequences(root)
        .unwrap()
        .1
        .iter()
        .map(|e| e.record.payload.tag())
        .collect();
    assert_eq!(tags.first(), Some(&20));
    assert_eq!(tags.iter().filter(|t| **t == 22).count(), 1);
    assert_eq!(tags.iter().filter(|t| **t == 6).count(), 7);
    check(&mut gather, "done-gather");
}
