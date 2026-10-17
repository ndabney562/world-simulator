//! TP-010 persistence (#11, ADR 0021 §7): illness, its events and causal
//! links, and a waiting intervention survive save and load; format-1 files
//! still load.

use std::path::{Path, PathBuf};

use ws_persist::{load, read_header, save};
use ws_sim::commands::{
    Action, Command, Effect, Intervention, InterventionTarget, IssuedBy, Timing,
};
use ws_sim::ws_events::Payload;
use ws_sim::{IllnessKind, Lod, SimTime, World, WorldConfig};
use ws_time::SimDuration;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ws-persist-iv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn steps(w: &mut World, n: u64) {
    for _ in 0..n {
        w.step().unwrap();
    }
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

#[test]
fn effect_events_and_causal_links_survive_save_and_load() {
    let dir = Scratch::new("effect");
    let mut w = World::new(WorldConfig::new(21, SimTime::EPOCH)).unwrap();
    steps(&mut w, 300);
    let b = w.toy().buildings().ids()[0];
    // Saved while some ill agents are at L2 and some at L1.
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        7,
        Timing::Now,
    )])
    .unwrap();
    steps(&mut w, 20);
    set_all(&mut w, Lod::L2);
    steps(&mut w, 12_000);
    assert!(
        w.events()
            .records()
            .iter()
            .any(|e| matches!(e.record.payload, Payload::ReturnedHome { .. })),
        "the saved history contains a return home"
    );

    let path = dir.0.join("ill.wssave");
    save(&w, &path).unwrap();
    assert_eq!(read_header(&path).unwrap().format_version, 2);
    assert_eq!(read_header(&path).unwrap().effect_vocabulary, 1);
    let mut loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert_eq!(loaded.checkpoint(), w.checkpoint());
    assert_eq!(loaded.toy().illness(), w.toy().illness());
    for id in w.toy().illness().ids() {
        let a = w.why_ill(*id).unwrap();
        let c = loaded.why_ill(*id).unwrap();
        assert_eq!(a, c);
    }

    // The loaded world keeps behaving identically, across a level change.
    for i in 0..3_000 {
        if i == 500 {
            set_all(&mut w, Lod::L1);
            set_all(&mut loaded, Lod::L1);
        }
        w.step().unwrap();
        loaded.step().unwrap();
        assert_eq!(w.checkpoint(), loaded.checkpoint());
    }
}

#[test]
fn a_waiting_intervention_survives_save_and_fires_after_load() {
    let dir = Scratch::new("waiting");
    let mut w = World::new(WorldConfig::new(22, SimTime::EPOCH)).unwrap();
    steps(&mut w, 50);
    let b = w.toy().buildings().ids()[1];
    let at = w
        .now()
        .saturating_add(SimDuration::from_micros(100 * 100_000));
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        3,
        Timing::At(at),
    )])
    .unwrap();
    steps(&mut w, 40);
    assert!(w.toy().illness().is_empty());

    let path = dir.0.join("waiting.wssave");
    save(&w, &path).unwrap();
    let mut loaded = load(&path).unwrap();
    assert_eq!(loaded, w);
    assert_eq!(loaded.queue_digest(), w.queue_digest());
    for _ in 0..80 {
        w.step().unwrap();
        loaded.step().unwrap();
        assert_eq!(w.checkpoint(), loaded.checkpoint());
    }
    assert_eq!(loaded.toy().illness().len(), w.toy().residents_of(b).len());
    assert!(!loaded.toy().illness().is_empty());
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/saves")
        .join(name)
}

/// `tests/saves/toy-v2.wssave` holds an intervention in effect, a waiting
/// one and a rejected one. Regenerate it only for an intentional format
/// change, with a migration for the old file:
/// `cargo run -p ws-cli -- save --seed 2026 --ticks 600 --script tests/saves/toy-v2.script --out tests/saves/toy-v2.wssave`
#[test]
fn the_format_2_fixture_loads() {
    let path = fixture("toy-v2.wssave");
    let h = read_header(&path).unwrap();
    assert_eq!(h.format_version, 2);
    assert_eq!((h.seed, h.tick), (2026, 600));
    let w = load(&path).unwrap();
    assert_eq!(w.state_hash(), h.state_hash);
    assert!(!w.toy().illness().is_empty());
    let (x, _) = w.toy().illness().iter().next().unwrap();
    assert_eq!(w.why_ill(x).unwrap().journal.sequence, 1);
    assert_eq!(w.journal().len(), 6);
    assert!(w.journal().iter().any(|o| o.result.is_err()));
    assert_eq!(w.to_parts().scheduled.len(), 1);
    let mut w = w;
    steps(&mut w, 20);
}

#[test]
fn a_format_1_file_loads_with_no_illness() {
    let w = load(&fixture("toy-v1.wssave")).unwrap();
    assert!(w.toy().illness().is_empty());
    assert!(w.to_parts().scheduled.is_empty());
    assert_eq!(
        read_header(&fixture("toy-v1.wssave"))
            .unwrap()
            .effect_vocabulary,
        0
    );
}

#[test]
fn an_illness_without_its_event_is_refused() {
    let dir = Scratch::new("tamper");
    let mut w = World::new(WorldConfig::new(23, SimTime::EPOCH)).unwrap();
    steps(&mut w, 100);
    let b = w.toy().buildings().ids()[0];
    w.submit(vec![afflict(
        InterventionTarget::ResidentsOf(b),
        2,
        Timing::Now,
    )])
    .unwrap();
    steps(&mut w, 5);
    let path = dir.0.join("t.wssave");
    save(&w, &path).unwrap();
    // Point one illness at an event that is not its own.
    let conn = rusqlite::Connection::open(&path).unwrap();
    let id: i64 = conn
        .query_row("SELECT id FROM illness ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let mut data: Vec<u8> = conn
        .query_row("SELECT data FROM illness WHERE id = ?", [id], |r| r.get(0))
        .unwrap();
    data[2] ^= 1;
    conn.execute(
        "UPDATE illness SET data = ? WHERE id = ?",
        rusqlite::params![data, id],
    )
    .unwrap();
    drop(conn);
    assert!(load(&path).is_err());
}
