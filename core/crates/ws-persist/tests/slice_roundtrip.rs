//! Integration tests for save format 4 slice persistence (ADR 0036, Issue #75).

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use ws_life::lod::CameraInterest;
use ws_persist::{
    FORMAT_VERSION, LoadError, MODEL_KIND_SLICE, MODEL_KIND_TOY, load, load_slice, read_header,
    save_slice, save_slice_background,
};
use ws_sim::SimTime;
use ws_sim::slice::{Operation, SliceConfig, SliceSimulation};

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
    assert_eq!(FORMAT_VERSION, 4);
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
    assert_eq!(h.format_version, 4);
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
