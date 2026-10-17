//! TP-009 acceptance tests (#10): round trips at every level, continuing
//! after a load, events and journal preserved, typed errors, and the
//! committed fixture.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use ws_persist::{FORMAT_VERSION, LoadError, SaveError, load, read_header, save};
use ws_sim::commands::{Action, Command, IssuedBy};
use ws_sim::{Lod, Population, SimTime, TimeScale, World, WorldConfig};

/// A fresh scratch directory per test, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ws-persist-{name}-{}", std::process::id()));
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

fn world(seed: u64) -> World {
    World::new(WorldConfig::new(seed, SimTime::EPOCH)).unwrap()
}

fn command(action: Action) -> Command {
    Command::new(IssuedBy::Test, action)
}

fn set_all(w: &mut World, level: Lod) {
    let req = w
        .toy()
        .buildings()
        .ids()
        .iter()
        .map(|id| (*id, level))
        .collect();
    w.submit(vec![command(Action::SetNodeLod(req))]).unwrap();
    w.flush_commands().unwrap();
}

fn set_first(w: &mut World, level: Lod) {
    let id = w.toy().buildings().ids()[0];
    w.submit(vec![command(Action::SetNodeLod(vec![(id, level)]))])
        .unwrap();
    w.flush_commands().unwrap();
}

fn steps(w: &mut World, n: u64) {
    for _ in 0..n {
        w.step().unwrap();
    }
}

/// Saves and loads `w`, asserting the loaded world is the same world.
fn round_trip(w: &World, dir: &Scratch) -> World {
    let path = dir.path("w.wssave");
    save(w, &path).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.state_hash(), w.state_hash());
    assert_eq!(loaded.checkpoint(), w.checkpoint());
    assert_eq!(&loaded, w);
    loaded
}

#[test]
fn a_fresh_world_round_trips() {
    let dir = Scratch::new("fresh");
    round_trip(&world(1), &dir);
}

#[test]
fn worlds_at_l1_l2_and_mixed_round_trip() {
    let dir = Scratch::new("levels");

    let mut l1 = world(2);
    steps(&mut l1, 400);
    round_trip(&l1, &dir);

    let mut l2 = world(3);
    set_all(&mut l2, Lod::L2);
    steps(&mut l2, 400);
    assert!(!l2.toy().background().is_empty() && l2.toy().walks().is_empty());
    round_trip(&l2, &dir);

    let mut mixed = world(4);
    steps(&mut mixed, 200);
    set_first(&mut mixed, Lod::L2);
    steps(&mut mixed, 600);
    assert!(!mixed.toy().background().is_empty() && !mixed.toy().walks().is_empty());
    round_trip(&mixed, &dir);
}

#[test]
fn save_at_tick_t_then_load_then_run_matches_an_uninterrupted_run() {
    let dir = Scratch::new("continue");
    let mut straight = world(5);
    set_all(&mut straight, Lod::L2);
    steps(&mut straight, 250);
    set_first(&mut straight, Lod::L1);
    steps(&mut straight, 250);

    let path = dir.path("t.wssave");
    save(&straight, &path).unwrap();
    let mut resumed = load(&path).unwrap();

    // The same later commands go to both; every tick's checkpoint (state,
    // pending queue, journal and event digests) must agree.
    let mixed_later = |w: &mut World| {
        w.submit(vec![
            command(Action::SetTimeScale(TimeScale::X10)),
            command(Action::SetNodeLod(vec![(
                w.toy().buildings().ids()[1],
                Lod::L1,
            )])),
        ])
        .unwrap();
    };
    mixed_later(&mut straight);
    mixed_later(&mut resumed);
    for _ in 0..1_500 {
        straight.step().unwrap();
        resumed.step().unwrap();
        assert_eq!(straight.checkpoint(), resumed.checkpoint());
    }
    assert_eq!(straight, resumed);
}

#[test]
fn events_causal_links_and_journal_are_identical_after_load() {
    let dir = Scratch::new("history");
    let mut w = world(6);
    steps(&mut w, 100);
    set_all(&mut w, Lod::L2);
    steps(&mut w, 2_000);
    set_all(&mut w, Lod::L1);
    steps(&mut w, 300);

    let events = w.events().records();
    assert!(events.len() > 10, "the run produced history");
    assert!(
        events.iter().any(|e| !e.record.causes.is_empty()),
        "the run produced causal links"
    );

    let loaded = round_trip(&w, &dir);
    assert_eq!(loaded.events().records(), events);
    assert_eq!(loaded.events().digest(), w.events().digest());
    assert_eq!(loaded.journal(), w.journal());
    assert_eq!(loaded.next_sequence(), w.next_sequence());
    for e in events {
        // Event IDs are stable and every cause still resolves.
        assert_eq!(loaded.events().get(e.id), Some(e));
        for id in e.record.causes.iter() {
            let ws_events::Target::Id(cause) = id.target else {
                panic!("committed cause is a reference");
            };
            assert!(loaded.events().get(cause).is_some());
        }
    }
}

#[test]
fn rejected_commands_survive_in_the_journal() {
    let dir = Scratch::new("rejected");
    let mut w = world(7);
    let unknown = ws_sim::EntityId::from_raw(0x4000_0000_0000_0001).unwrap();
    let b = w.toy().buildings().ids()[0];
    w.submit(vec![
        command(Action::SetNodeLod(vec![(unknown, Lod::L2)])),
        command(Action::SetNodeLod(vec![(b, Lod::L2), (b, Lod::L1)])),
        command(Action::SetNodeLod(vec![(b, Lod::L3)])),
        command(Action::NoOp),
    ])
    .unwrap();
    w.flush_commands().unwrap();
    assert_eq!(w.journal().iter().filter(|o| o.result.is_err()).count(), 3);
    round_trip(&w, &dir);
}

#[test]
fn pending_commands_refuse_to_save_and_leave_the_target_alone() {
    let dir = Scratch::new("pending");
    let path = dir.path("slot.wssave");
    std::fs::write(&path, b"previous contents").unwrap();
    let mut w = world(8);
    w.submit(vec![command(Action::NoOp)]).unwrap();
    assert!(matches!(save(&w, &path), Err(SaveError::PendingCommands)));
    assert_eq!(std::fs::read(&path).unwrap(), b"previous contents");
    assert!(!dir.path("slot.wssave.saving").exists());
}

#[test]
fn saving_replaces_a_previous_save_and_leaves_no_temporary_file() {
    let dir = Scratch::new("replace");
    let path = dir.path("slot.wssave");
    let mut w = world(9);
    save(&w, &path).unwrap();
    let first = read_header(&path).unwrap();
    steps(&mut w, 50);
    save(&w, &path).unwrap();
    let second = read_header(&path).unwrap();
    assert_eq!((first.tick, second.tick), (0, 50));
    assert_eq!(load(&path).unwrap(), w);
    assert!(!dir.path("slot.wssave.saving").exists());
}

#[test]
fn the_header_records_every_version() {
    let dir = Scratch::new("header");
    let mut config = WorldConfig::new(0xfeed, SimTime::EPOCH);
    config.population = Population::exactly(3);
    let mut w = World::new(config).unwrap();
    steps(&mut w, 12);
    let path = dir.path("h.wssave");
    save(&w, &path).unwrap();
    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, FORMAT_VERSION);
    assert!(!h.build.is_empty());
    assert_eq!(h.seed, 0xfeed);
    assert_eq!(h.dataset_version, ws_sim::TOY_DATASET_VERSION);
    assert_eq!((h.generator_version, h.rules_version), (1, 1));
    assert_eq!((h.population_min, h.population_max), (3, 3));
    assert_eq!(h.tick, 12);
    assert_eq!(h.sim_time_us, w.now().as_micros_since_epoch());
    assert_eq!(h.walk_speed_bits, 1.4_f64.to_bits());
    assert_eq!(h.walk_wait_steps, 50);
    assert_eq!(h.state_hash, w.state_hash());
    assert_eq!(h.journal_digest, w.journal_digest());
    assert_eq!(h.events_digest, w.events().digest());
}

#[test]
fn an_unknown_or_newer_format_version_fails_with_the_typed_error() {
    let dir = Scratch::new("version");
    let path = dir.path("v.wssave");
    save(&world(10), &path).unwrap();
    for found in [FORMAT_VERSION + 1, 99, 0] {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", found).unwrap();
        drop(conn);
        match load(&path) {
            Err(LoadError::UnsupportedVersion {
                found: f,
                supported,
            }) => {
                assert_eq!((f, supported), (found, FORMAT_VERSION));
            }
            other => panic!("expected UnsupportedVersion for {found}, got {other:?}"),
        }
        assert!(matches!(
            read_header(&path),
            Err(LoadError::UnsupportedVersion { .. })
        ));
    }
}

#[test]
fn files_that_are_not_saves_are_rejected() {
    let dir = Scratch::new("notsave");
    let text = dir.path("text");
    std::fs::write(&text, b"this is not a database at all, just some text").unwrap();
    assert!(matches!(load(&text), Err(LoadError::NotASave)));

    let other = dir.path("other.db");
    let conn = Connection::open(&other).unwrap();
    conn.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
        .unwrap();
    drop(conn);
    assert!(matches!(load(&other), Err(LoadError::NotASave)));

    assert!(matches!(
        load(&dir.path("missing.wssave")),
        Err(LoadError::Io(_))
    ));
}

#[test]
fn a_missing_header_or_table_is_corrupt() {
    let dir = Scratch::new("missing");
    let path = dir.path("m.wssave");
    save(&world(11), &path).unwrap();

    let conn = Connection::open(&path).unwrap();
    conn.execute("DELETE FROM header", []).unwrap();
    drop(conn);
    assert!(matches!(
        load(&path),
        Err(LoadError::Corrupt {
            table: "header",
            ..
        })
    ));

    save(&world(11), &path).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE events").unwrap();
    drop(conn);
    assert!(matches!(
        load(&path),
        Err(LoadError::Corrupt {
            table: "events",
            ..
        })
    ));
}

#[test]
fn a_damaged_record_is_detected_never_loaded() {
    let dir = Scratch::new("damaged");
    let path = dir.path("d.wssave");
    let mut w = world(12);
    steps(&mut w, 30);

    // A truncated record fails to decode.
    save(&w, &path).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE l1_state SET data = substr(data, 1, 10) WHERE id = (SELECT id FROM l1_state LIMIT 1)",
        [],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        load(&path),
        Err(LoadError::Corrupt {
            table: "l1_state",
            ..
        })
    ));

    // A record that still decodes but changed value fails the state hash.
    save(&w, &path).unwrap();
    let conn = Connection::open(&path).unwrap();
    let (id, mut data): (i64, Vec<u8>) = conn
        .query_row("SELECT id, data FROM l1_state LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    let last = data.len() - 1;
    data[last] ^= 1; // the high byte of `targets_drawn`
    conn.execute(
        "UPDATE l1_state SET data = ? WHERE id = ?",
        rusqlite::params![data, id],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        load(&path),
        Err(LoadError::HashMismatch { what: "state", .. })
    ));

    // Events whose causal links were removed change the history digest.
    let mut w2 = world(12);
    set_all(&mut w2, Lod::L2);
    steps(&mut w2, 2_000);
    save(&w2, &path).unwrap();
    let conn = Connection::open(&path).unwrap();
    let removed = conn.execute("DELETE FROM event_links", []).unwrap();
    drop(conn);
    assert!(removed > 0, "the run produced links to remove");
    assert!(matches!(
        load(&path),
        Err(LoadError::HashMismatch { what: "events", .. })
    ));
}

/// The committed fixture (`tests/saves/` at the repository root) must keep
/// loading in CI. Regenerate it only for an intentional format change, with
/// a migration for the old file:
/// `cargo run -p ws-cli -- save --seed 2026 --ticks 600 --script tests/saves/toy-v1.script --out tests/saves/toy-v1.wssave`
#[test]
fn the_committed_fixture_loads() {
    let path = fixture("toy-v1.wssave");
    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, 1);
    assert_eq!((h.seed, h.tick), (2026, 600));
    let w = load(&path).unwrap();
    assert_eq!(w.tick(), 600);
    assert_eq!(w.state_hash(), h.state_hash);
    // It exercises both stores and rejected commands.
    assert!(!w.toy().walks().is_empty() && !w.toy().background().is_empty());
    assert!(w.journal().iter().any(|o| o.result.is_err()));
    // And it still continues and re-saves.
    let mut w = w;
    steps(&mut w, 20);
    let dir = Scratch::new("fixture");
    round_trip(&w, &dir);
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/saves")
        .join(name)
}

/// Measurements for the PR (run with `--ignored --nocapture`, release):
/// median of seven saves and loads, after a warm-up.
#[test]
#[ignore = "measurement, not a test"]
// Timing a save is a host measurement, not simulation logic.
#[allow(clippy::disallowed_methods)]
fn measure_save_and_load() {
    use std::time::{Duration, Instant};
    fn median(mut v: Vec<Duration>) -> Duration {
        v.sort();
        v[v.len() / 2]
    }
    for agents in [50_u32, 5_000] {
        let dir = Scratch::new("measure");
        let mut config = WorldConfig::new(1, SimTime::EPOCH);
        config.population = Population::exactly(agents / 5);
        let mut w = World::new(config).unwrap();
        steps(&mut w, 600);
        set_first(&mut w, Lod::L2);
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
            "{agents} agents: save {:?}, load {:?}, {} bytes, {} events, {} journal",
            median(saves),
            median(loads),
            std::fs::metadata(&path).unwrap().len(),
            w.events().records().len(),
            w.journal().len()
        );
    }
}
