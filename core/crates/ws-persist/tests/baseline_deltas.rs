//! Issue #53 / ADR 0025 acceptance tests: version-pinned baseline plus
//! canonical structural deltas (`structural_overrides`, `notables`,
//! `tombstones`, `spawned`) in save format 3, format-1/2 compatibility, and
//! explicit atomic migration to format 3.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use ws_math::{SECTOR_LEVEL, SurfacePos, reference::BAY_RIDGE};
use ws_persist::{
    BaselineRegistry, COMPONENT_BUILDING, COMPONENT_RESIDENT, FORMAT_VERSION, LoadError, SaveError,
    load, load_with_registry, migrate_to_v3, read_baseline, read_header, save, save_with_registry,
};
use ws_sim::commands::{
    Action, Command, Effect, Intervention, InterventionTarget, IssuedBy, Timing,
};
use ws_sim::ws_events::{Actor, EventStore, Role};
use ws_sim::{
    Building, Dataset, EntityId, IllnessKind, Lod, Population, SimTime, TOY_DATASET_ID,
    TOY_DATASET_VERSION, World, WorldConfig, sha256,
};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ws-persist-v3-{name}-{}", std::process::id()));
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

fn world(seed: u64) -> World {
    World::new(WorldConfig::new(seed, SimTime::EPOCH)).unwrap()
}

fn steps(w: &mut World, n: u64) {
    for _ in 0..n {
        w.step().unwrap();
    }
}

fn count_rows(path: &Path, table: &str) -> i64 {
    let conn = Connection::open(path).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

const fn sql_key(u: u64) -> i64 {
    (u ^ (1 << 63)) as i64
}

const fn un_sql_key(k: i64) -> u64 {
    (k as u64) ^ (1 << 63)
}

#[test]
fn unchanged_baseline_writes_zero_structural_delta_rows() {
    let dir = Scratch::new("unchanged");
    let mut w = world(101);
    steps(&mut w, 120);
    let b0 = w.toy().buildings().ids()[0];
    w.submit(vec![Command::new(
        IssuedBy::Test,
        Action::SetNodeLod(vec![(b0, Lod::L2)]),
    )])
    .unwrap();
    steps(&mut w, 240);

    let path = dir.path("u.wssave");
    save(&w, &path).unwrap();

    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3);

    let base = read_baseline(&path).unwrap();
    let ds = Dataset::toy().unwrap();
    assert_eq!(base.seed, 101);
    assert_eq!(base.dataset_id, TOY_DATASET_ID);
    assert_eq!(base.dataset_version, TOY_DATASET_VERSION);
    assert_eq!(base.dataset_sha256, ds.content_sha256());
    assert_eq!(base.building_count as usize, w.toy().buildings().len());
    assert_eq!(base.resident_count as usize, w.toy().residents().len());
    assert_eq!(base.baseline_digest, base.compute_digest());

    assert_eq!(count_rows(&path, "baseline"), 1);
    assert_eq!(count_rows(&path, "structural_overrides"), 0);
    assert_eq!(count_rows(&path, "notables"), 0);
    assert_eq!(count_rows(&path, "tombstones"), 0);
    assert_eq!(count_rows(&path, "spawned"), 0);
    assert_eq!(
        count_rows(&path, "node_lod") as usize,
        w.toy().buildings().len()
    );

    let loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert_eq!(loaded.checkpoint(), w.checkpoint());
}

#[test]
fn structural_overrides_store_only_changed_components_and_round_trip() {
    let dir = Scratch::new("overrides");
    let mut w = world(102);
    steps(&mut w, 40);

    let b_id = w.toy().buildings().ids()[1];
    let mut b = *w.toy().buildings().get(b_id).unwrap();
    b.height_m += 7.5;
    w.override_building(b_id, b).unwrap();

    let r_id = w.toy().residents().ids()[3];
    let mut r = *w.toy().residents().get(r_id).unwrap();
    let new_home = w.toy().buildings().ids()[2];
    r.home = new_home;
    w.override_resident(r_id, r).unwrap();

    let path = dir.path("ov.wssave");
    save(&w, &path).unwrap();

    assert_eq!(count_rows(&path, "structural_overrides"), 2);
    assert_eq!(count_rows(&path, "notables"), 0);
    assert_eq!(count_rows(&path, "tombstones"), 0);
    assert_eq!(count_rows(&path, "spawned"), 0);

    let conn = Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT id, component_tag FROM structural_overrides ORDER BY id, component_tag")
        .unwrap();
    let rows: Vec<(u64, u8)> = stmt
        .query_map([], |row| {
            Ok((
                un_sql_key(row.get::<_, i64>(0)?),
                row.get::<_, u32>(1)? as u8,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    drop(stmt);
    drop(conn);

    assert!(rows.contains(&(b_id.raw(), COMPONENT_BUILDING)));
    assert!(rows.contains(&(r_id.raw(), COMPONENT_RESIDENT)));

    let mut loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert_eq!(loaded.checkpoint(), w.checkpoint());
    assert_eq!(loaded.toy().buildings().get(b_id), Some(&b));
    assert_eq!(loaded.toy().residents().get(r_id), Some(&r));
    assert!(loaded.toy().residents_of(new_home).contains(&r_id));

    for _ in 0..100 {
        w.step().unwrap();
        loaded.step().unwrap();
        assert_eq!(w.checkpoint(), loaded.checkpoint());
    }
}

#[test]
fn promoted_notables_persist_in_notables_table_without_structural_overrides() {
    let dir = Scratch::new("notables");
    let mut w = world(103);
    steps(&mut w, 25);

    let b_id = w.toy().buildings().ids()[0];
    let r_id = w.toy().residents().ids()[0];

    // Modify the resident's home AND promote it to notable: notable owns the
    // full structural record, so structural_overrides must have 0 rows.
    let mut r = *w.toy().residents().get(r_id).unwrap();
    r.home = w.toy().buildings().ids()[1];
    w.override_resident(r_id, r).unwrap();
    w.promote_notable(b_id).unwrap();
    w.promote_notable(r_id).unwrap();

    let path = dir.path("notables.wssave");
    save(&w, &path).unwrap();

    assert_eq!(count_rows(&path, "notables"), 2);
    assert_eq!(count_rows(&path, "structural_overrides"), 0);

    let loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert_eq!(loaded.toy().notables(), w.toy().notables());
    assert!(loaded.toy().notables().contains(&b_id));
    assert!(loaded.toy().notables().contains(&r_id));
}

#[test]
fn tombstones_suppress_regenerated_entities_while_preserving_historical_event_references() {
    let dir = Scratch::new("tombstones");
    let mut w = world(104);
    steps(&mut w, 50);

    // Afflict residents of building 0 so they appear as actors in historical events.
    let b0 = w.toy().buildings().ids()[0];
    let doomed_resident = w.toy().residents_of(b0)[0];
    w.submit(vec![Command::new(
        IssuedBy::Test,
        Action::Intervene(Intervention {
            target: InterventionTarget::Entity(doomed_resident),
            effect: Effect::Afflict {
                illness: IllnessKind::Fever,
                severity: 4,
            },
            timing: Timing::Now,
        }),
    )])
    .unwrap();
    steps(&mut w, 10);

    let root_event = w.toy().illness().get(doomed_resident).unwrap().root_event;
    assert!(
        w.events().records().iter().any(|e| e
            .record
            .actors
            .iter()
            .any(|a| a.id == doomed_resident)),
        "doomed_resident is an actor in committed history"
    );

    // Tombstone the resident at current tick.
    w.tombstone_resident(doomed_resident, root_event).unwrap();
    assert!(!w.toy().residents().contains(doomed_resident));
    assert!(w.toy().tombstones().contains(doomed_resident));

    let path = dir.path("tomb.wssave");
    save(&w, &path).unwrap();

    assert_eq!(count_rows(&path, "tombstones"), 1);
    assert_eq!(count_rows(&path, "structural_overrides"), 0);

    let mut loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert!(!loaded.toy().residents().contains(doomed_resident));
    assert_eq!(
        loaded.toy().tombstones().get(doomed_resident),
        w.toy().tombstones().get(doomed_resident)
    );

    // Historical events referencing the tombstoned actor are still intact.
    assert_eq!(loaded.events().records(), w.events().records());

    // And EventStore still rejects unknown/forged actor IDs that are neither
    // live nor tombstoned.
    let parts = loaded.to_parts();
    let known: Vec<EntityId> = parts
        .toy
        .buildings()
        .ids()
        .iter()
        .chain(parts.toy.residents().ids())
        .chain(parts.toy.tombstones().ids())
        .copied()
        .collect();
    let mut forged_events = parts.events.clone();
    let forged_id = EntityId::spawned(999_999).unwrap();
    forged_events[0]
        .record
        .actors
        .push(Actor {
            id: forged_id,
            role: Role::Subject,
        })
        .unwrap();
    assert!(EventStore::from_records(known, &forged_events).is_err());

    // Continuing the simulation after tombstoning matches uninterrupted execution.
    for _ in 0..100 {
        w.step().unwrap();
        loaded.step().unwrap();
        assert_eq!(w.checkpoint(), loaded.checkpoint());
    }
}

#[test]
fn unreferenced_building_can_be_tombstoned_and_referenced_building_cannot() {
    let dir = Scratch::new("tomb-building");
    let mut w = world(105);
    let b_last = *w.toy().buildings().ids().last().unwrap();

    // While residents live at b_last, tombstoning b_last is refused.
    assert!(w.tombstone_building(b_last, 0).is_err());

    // Tombstone all residents of b_last first.
    let res_ids: Vec<EntityId> = w.toy().residents_of(b_last).to_vec();
    for r_id in res_ids {
        w.tombstone_resident(r_id, 0).unwrap();
    }
    w.tombstone_building(b_last, 0).unwrap();
    assert!(!w.toy().buildings().contains(b_last));
    assert!(w.toy().tombstones().contains(b_last));

    let path = dir.path("tb.wssave");
    save(&w, &path).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
}

#[test]
fn spawned_entities_and_next_spawned_id_round_trip_and_continue_allocating() {
    let dir = Scratch::new("spawned");
    let mut w = world(106);
    steps(&mut w, 30);

    let cell = ws_math::CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap();
    let pos = SurfacePos::new(cell, [25.0, -15.0, 0.0]).unwrap();
    let sb = w
        .spawn_building(
            Building {
                index: 99,
                pos,
                half_extent_m: 8.0,
                height_m: 14.0,
            },
            Lod::L1,
        )
        .unwrap();
    let sr = w.spawn_resident(sb).unwrap();
    assert_eq!(sb, EntityId::spawned(0).unwrap());
    assert_eq!(sr, EntityId::spawned(1).unwrap());
    assert_eq!(w.toy().next_spawned_id(), 2);

    steps(&mut w, 60);

    let path = dir.path("sp.wssave");
    save(&w, &path).unwrap();

    assert_eq!(count_rows(&path, "spawned"), 2);
    assert_eq!(count_rows(&path, "structural_overrides"), 0);

    let mut loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert_eq!(loaded.toy().next_spawned_id(), 2);

    // Next spawn after load gets EntityId::spawned(2) on both worlds.
    let next_w = w.spawn_resident(sb).unwrap();
    let next_loaded = loaded.spawn_resident(sb).unwrap();
    assert_eq!(next_w, EntityId::spawned(2).unwrap());
    assert_eq!(next_loaded, EntityId::spawned(2).unwrap());
    assert_eq!(loaded, w);
}

#[test]
fn missing_or_altered_dataset_fails_with_missing_baseline() {
    let dir = Scratch::new("missing-baseline");
    let w = world(107);
    let path = dir.path("mb.wssave");
    save(&w, &path).unwrap();
    let expected_sha = Dataset::toy().unwrap().content_sha256();

    // Empty registry -> MissingBaseline.
    let empty_reg = BaselineRegistry::new();
    match load_with_registry(&path, &empty_reg) {
        Err(LoadError::MissingBaseline {
            dataset_id,
            dataset_version,
            sha256: digest,
        }) => {
            assert_eq!(dataset_id, TOY_DATASET_ID);
            assert_eq!(dataset_version, TOY_DATASET_VERSION);
            assert_eq!(digest, expected_sha);
        }
        other => panic!("expected MissingBaseline, got {other:?}"),
    }

    // 1-bit altered dataset height -> content SHA-256 differs -> MissingBaseline.
    let mut altered = Dataset::toy().unwrap();
    altered.sites[0].height_m = f32::from_bits(altered.sites[0].height_m.to_bits() ^ 1);
    assert_ne!(altered.content_sha256(), expected_sha);
    let mut bad_reg = BaselineRegistry::new();
    bad_reg.register(TOY_DATASET_ID, altered);
    assert!(matches!(
        load_with_registry(&path, &bad_reg),
        Err(LoadError::MissingBaseline { .. })
    ));
}

#[test]
fn unsupported_baseline_versions_and_collision_overrides_are_rejected() {
    let dir = Scratch::new("unsupported-baseline");
    let w = world(108);
    let path = dir.path("ub.wssave");

    for (field, mutator) in [
        (
            "building_generator_version",
            Box::new(|b: &mut ws_persist::BaselineRecord| b.building_generator_version = 99)
                as Box<dyn Fn(&mut ws_persist::BaselineRecord)>,
        ),
        (
            "resident_generator_version",
            Box::new(|b: &mut ws_persist::BaselineRecord| b.resident_generator_version = 99),
        ),
        (
            "rules_version",
            Box::new(|b: &mut ws_persist::BaselineRecord| b.rules_version = 99),
        ),
        (
            "identity_policy_version",
            Box::new(|b: &mut ws_persist::BaselineRecord| b.identity_policy_version = 99),
        ),
        (
            "collision_overrides",
            Box::new(|b: &mut ws_persist::BaselineRecord| b.collision_overrides = vec![1, 2, 3]),
        ),
    ] {
        save(&w, &path).unwrap();
        let mut base = read_baseline(&path).unwrap();
        mutator(&mut base);
        base.baseline_digest = base.compute_digest();

        // Re-encode the baseline row directly.
        let conn = Connection::open(&path).unwrap();
        let mut bytes: Vec<u8> = conn
            .query_row("SELECT data FROM baseline WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        // Rebuild baseline blob via layout:
        // seed(8) + dataset_id(4) + dataset_version(8) + sha256(32) = 52
        // + b_gen(4) + r_gen(4) + rules(4) + policy(4) = 68
        // + pop_min(4) + pop_max(4) + speed(8) + wait(4) + start(8) = 96
        // + b_count(4) + r_count(4) + struct_digest(8) + base_digest(8) = 120
        // + vec_len(4) = 124
        bytes.truncate(120);
        bytes[52..56].copy_from_slice(&base.building_generator_version.to_le_bytes());
        bytes[56..60].copy_from_slice(&base.resident_generator_version.to_le_bytes());
        bytes[60..64].copy_from_slice(&base.rules_version.to_le_bytes());
        bytes[64..68].copy_from_slice(&base.identity_policy_version.to_le_bytes());
        bytes[112..120].copy_from_slice(&base.baseline_digest.to_le_bytes());
        bytes.extend_from_slice(&(base.collision_overrides.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&base.collision_overrides);
        conn.execute("UPDATE baseline SET data = ? WHERE id = 1", [&bytes])
            .unwrap();
        drop(conn);

        match load(&path) {
            Err(LoadError::UnsupportedBaseline { field: f, .. }) => assert_eq!(f, field),
            other => panic!("expected UnsupportedBaseline for {field}, got {other:?}"),
        }
    }
}

#[test]
fn duplicate_or_conflicting_delta_ownership_is_rejected() {
    let dir = Scratch::new("conflicts");
    let mut w = world(109);
    let b0 = w.toy().buildings().ids()[0];
    let r0 = w.toy().residents().ids()[0];

    // 1. Redundant structural_overrides row identical to baseline -> Corrupt.
    let path = dir.path("redundant.wssave");
    let mut w_mod = w.clone();
    let mut b_mod = *w_mod.toy().buildings().get(b0).unwrap();
    b_mod.height_m += 5.0;
    w_mod.override_building(b0, b_mod).unwrap();
    save(&w_mod, &path).unwrap();

    // Replace the override data with the unmodified baseline building bytes.
    let orig_b = *w.toy().buildings().get(b0).unwrap();
    let conn = Connection::open(&path).unwrap();
    let (cell_k, mut data): (i64, Vec<u8>) = conn
        .query_row(
            "SELECT cell, data FROM structural_overrides WHERE id = ?",
            [sql_key(b0.raw())],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let len = data.len();
    data[len - 4..].copy_from_slice(&orig_b.height_m.to_bits().to_le_bytes());
    conn.execute(
        "UPDATE structural_overrides SET data = ? WHERE id = ?",
        rusqlite::params![data, sql_key(b0.raw())],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        load(&path),
        Err(LoadError::Corrupt {
            table: "structural_overrides",
            ..
        })
    ));

    // 2. Simultaneous presence in notables and structural_overrides -> Corrupt.
    let mut w_notable = w.clone();
    w_notable.promote_notable(b0).unwrap();
    let path_notable = dir.path("notable_conflict.wssave");
    save(&w_notable, &path_notable).unwrap();
    let conn = Connection::open(&path_notable).unwrap();
    let override_blob: Vec<u8> = {
        let p2 = dir.path("tmp_ov.wssave");
        save(&w_mod, &p2).unwrap();
        let c2 = Connection::open(&p2).unwrap();
        c2.query_row(
            "SELECT data FROM structural_overrides WHERE id = ?",
            [sql_key(b0.raw())],
            |r| r.get(0),
        )
        .unwrap()
    };
    conn.execute(
        "INSERT INTO structural_overrides (cell, id, component_tag, schema, data) VALUES (?, ?, ?, 1, ?)",
        rusqlite::params![
            cell_k,
            sql_key(b0.raw()),
            u32::from(COMPONENT_BUILDING),
            override_blob
        ],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        load(&path_notable),
        Err(LoadError::Corrupt {
            table: "structural_overrides",
            ..
        })
    ));

    // 3. Simultaneous presence in tombstones and l1_state -> Corrupt.
    w.tombstone_resident(r0, 0).unwrap();
    let path_tomb = dir.path("tomb_conflict.wssave");
    save(&w, &path_tomb).unwrap();
    let conn = Connection::open(&path_tomb).unwrap();
    let (l1_cell, any_l1): (i64, Vec<u8>) = conn
        .query_row("SELECT cell, data FROM l1_state LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    conn.execute(
        "INSERT INTO l1_state (cell, id, schema, data) VALUES (?, ?, 1, ?)",
        rusqlite::params![l1_cell, sql_key(r0.raw()), any_l1],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        load(&path_tomb),
        Err(LoadError::Corrupt {
            table: "l1_state",
            ..
        })
    ));
}

#[test]
fn format_1_and_2_saves_migrate_atomically_to_format_3() {
    let dir = Scratch::new("migrate");
    for name in ["toy-v1.wssave", "toy-v2.wssave"] {
        let src = fixture(name);
        let src_bytes = std::fs::read(&src).unwrap();
        let dst = dir.path(&format!("migrated-{name}"));

        migrate_to_v3(&src, &dst).unwrap();

        // Source file is byte-for-byte untouched.
        assert_eq!(std::fs::read(&src).unwrap(), src_bytes);

        // Destination is a valid format-3 save with 0 structural override rows.
        let h = read_header(&dst).unwrap();
        assert_eq!(h.format_version, 3);
        assert_eq!(count_rows(&dst, "baseline"), 1);
        assert_eq!(count_rows(&dst, "structural_overrides"), 0);
        assert_eq!(count_rows(&dst, "notables"), 0);
        assert_eq!(count_rows(&dst, "tombstones"), 0);
        assert_eq!(count_rows(&dst, "spawned"), 0);

        // Loaded world from migrated file equals loaded world from original fixture.
        let w_orig = load(&src).unwrap();
        let w_mig = load(&dst).unwrap();
        assert_eq!(w_mig, w_orig);
        assert_eq!(w_mig.checkpoint(), w_orig.checkpoint());
    }

    // Failed migration leaves destination and staging untouched.
    let bad_src = dir.path("not-a-save.wssave");
    std::fs::write(&bad_src, b"not a save").unwrap();
    let dst = dir.path("keep.wssave");
    std::fs::write(&dst, b"existing destination").unwrap();
    assert!(migrate_to_v3(&bad_src, &dst).is_err());
    assert_eq!(std::fs::read(&dst).unwrap(), b"existing destination");
    assert!(!dir.path("keep.wssave.saving").exists());
}

#[test]
fn migration_preserves_source_when_it_matches_the_old_staging_name() {
    let dir = Scratch::new("migration-staging-alias");
    let source = dir.path("target.wssave.saving");
    let target = dir.path("target.wssave");
    std::fs::copy(fixture("toy-v2.wssave"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    migrate_to_v3(&source, &target).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(load(&source).unwrap(), load(&target).unwrap());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
}

#[test]
fn migration_rejects_parent_path_alias_without_rewriting_source() {
    let dir = Scratch::new("migration-parent-alias");
    std::fs::create_dir(dir.path("child")).unwrap();
    let source = dir.path("source.wssave");
    let target = dir.path("child/../source.wssave");
    std::fs::copy(fixture("toy-v2.wssave"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    assert!(migrate_to_v3(&source, &target).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn migration_rejects_symlink_alias_without_rewriting_source() {
    let dir = Scratch::new("migration-symlink-alias");
    let source = dir.path("source.wssave");
    let target = dir.path("alias.wssave");
    std::fs::copy(fixture("toy-v2.wssave"), &source).unwrap();
    std::os::unix::fs::symlink(&source, &target).unwrap();
    let original = std::fs::read(&source).unwrap();
    assert!(migrate_to_v3(&source, &target).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert!(target.is_symlink());
}

#[test]
fn migration_publication_failure_preserves_source_and_destination() {
    let dir = Scratch::new("migration-publish-failure");
    let source = dir.path("source.wssave");
    let target = dir.path("target.wssave");
    std::fs::copy(fixture("toy-v2.wssave"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"existing destination").unwrap();
    assert!(migrate_to_v3(&source, &target).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(
        std::fs::read(target.join("keep")).unwrap(),
        b"existing destination"
    );
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
}

#[test]
fn oversized_or_inconsistent_baseline_is_rejected_before_regeneration() {
    let dir = Scratch::new("baseline-preflight");
    let path = dir.path("oversized.wssave");
    for population in [Some(u32::MAX), Some(200_001), None] {
        save(&world(109), &path).unwrap();
        let mut baseline = read_baseline(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        let mut bytes: Vec<u8> = conn
            .query_row("SELECT data FROM baseline WHERE id=1", [], |r| r.get(0))
            .unwrap();
        let reason = if let Some(count) = population {
            baseline.population_min = count;
            baseline.population_max = count;
            bytes[68..72].copy_from_slice(&count.to_le_bytes());
            bytes[72..76].copy_from_slice(&count.to_le_bytes());
            conn.execute(
                "UPDATE header SET population_min=?1, population_max=?1",
                [count],
            )
            .unwrap();
            "baseline exceeds entity limit"
        } else {
            baseline.resident_count += 1;
            bytes[100..104].copy_from_slice(&baseline.resident_count.to_le_bytes());
            "regenerated baseline manifest mismatch"
        };
        bytes[112..120].copy_from_slice(&baseline.compute_digest().to_le_bytes());
        conn.execute("UPDATE baseline SET data=? WHERE id=1", [&bytes])
            .unwrap();
        drop(conn);
        assert!(
            matches!(load(&path), Err(LoadError::Corrupt {table: "baseline", reason: r}) if r==reason)
        );
    }
}

/// `tests/saves/toy-v3.wssave` is the committed format-3 fixture (ADR 0025).
#[test]
fn the_format_3_fixture_loads() {
    let path = fixture("toy-v3.wssave");
    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, 3);
    assert_eq!((h.seed, h.tick), (2026, 600));
    assert_eq!(count_rows(&path, "baseline"), 1);
    assert_eq!(count_rows(&path, "structural_overrides"), 0);
    assert_eq!(count_rows(&path, "notables"), 0);
    assert_eq!(count_rows(&path, "tombstones"), 0);
    assert_eq!(count_rows(&path, "spawned"), 0);

    let w = load(&path).unwrap();
    assert_eq!(w.state_hash(), h.state_hash);
    assert!(!w.toy().illness().is_empty());
    let (x, _) = w.toy().illness().iter().next().unwrap();
    assert_eq!(w.why_ill(x).unwrap().journal.sequence, 1);
    assert_eq!(w.journal().len(), 6);
    assert!(w.journal().iter().any(|o| o.result.is_err()));
    assert_eq!(w.to_parts().scheduled.len(), 1);

    // Matches the format-2 fixture generated from the same seed and script.
    let w2 = load(&fixture("toy-v2.wssave")).unwrap();
    assert_eq!(w, w2);
    assert_eq!(w.checkpoint(), w2.checkpoint());

    let mut w = w;
    steps(&mut w, 20);
}

#[test]
fn save_with_unregistered_baseline_refuses_without_touching_target() {
    let dir = Scratch::new("unregistered-save");
    let w = world(110);
    let path = dir.path("target.wssave");
    std::fs::write(&path, b"untouched").unwrap();
    let empty_reg = BaselineRegistry::new();
    assert!(matches!(
        save_with_registry(&w, &path, &empty_reg),
        Err(SaveError::Unsaveable(_))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"untouched");
}

#[test]
fn sha256_matches_canonical_dataset_bytes() {
    let ds = Dataset::toy().unwrap();
    assert_eq!(ds.content_sha256(), sha256(&ds.canonical_bytes()));
}

#[test]
#[ignore = "measurement, not a test"]
#[allow(clippy::disallowed_methods)]
fn measure_v3_save_and_load_and_row_counts() {
    use std::time::{Duration, Instant};
    fn median(mut v: Vec<Duration>) -> Duration {
        v.sort();
        v[v.len() / 2]
    }

    let v2_fixture = fixture("toy-v2.wssave");
    let v3_fixture = fixture("toy-v3.wssave");
    println!(
        "fixture comparison: toy-v2.wssave = {} bytes (buildings={}, residents={}), toy-v3.wssave = {} bytes (baseline={}, structural_overrides={}, node_lod={})",
        std::fs::metadata(&v2_fixture).unwrap().len(),
        count_rows(&v2_fixture, "buildings"),
        count_rows(&v2_fixture, "residents"),
        std::fs::metadata(&v3_fixture).unwrap().len(),
        count_rows(&v3_fixture, "baseline"),
        count_rows(&v3_fixture, "structural_overrides"),
        count_rows(&v3_fixture, "node_lod"),
    );

    for agents in [50_u32, 5_000] {
        let dir = Scratch::new("measure-v3");
        let mut config = WorldConfig::new(1, SimTime::EPOCH);
        config.population = Population::exactly(agents / 5);
        let mut w = World::new(config).unwrap();
        steps(&mut w, 600);
        let b0 = w.toy().buildings().ids()[0];
        w.submit(vec![Command::new(
            IssuedBy::Test,
            Action::SetNodeLod(vec![(b0, Lod::L2)]),
        )])
        .unwrap();
        w.flush_commands().unwrap();
        steps(&mut w, 3_000);
        let path = dir.path("m.wssave");
        save(&w, &path).unwrap();
        drop(load(&path).unwrap());
        let (mut saves, mut loads) = (Vec::new(), Vec::new());
        for _ in 0..7 {
            let t = Instant::now();
            save(&w, &path).unwrap();
            saves.push(t.elapsed());
            let t = Instant::now();
            let loaded = load(&path).unwrap();
            loads.push(t.elapsed());
            assert_eq!(loaded.state_hash(), w.state_hash());
        }
        println!(
            "{agents} agents (format 3): save {:?}, load {:?}, {} bytes, baseline_rows={}, override_rows={}, node_lod_rows={}, l1_rows={}, l2_rows={}, events={}, journal={}",
            median(saves),
            median(loads),
            std::fs::metadata(&path).unwrap().len(),
            count_rows(&path, "baseline"),
            count_rows(&path, "structural_overrides"),
            count_rows(&path, "node_lod"),
            count_rows(&path, "l1_state"),
            count_rows(&path, "l2_state"),
            w.events().records().len(),
            w.journal().len()
        );
    }
}
