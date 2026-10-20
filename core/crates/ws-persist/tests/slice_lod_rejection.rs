//! A slice-only LOD fact cannot silently extend legacy toy save formats.
use ws_events::{Actor, Candidate, EventStore, Origin, Payload, Role};
use ws_persist::{SaveError, save};
use ws_sim::{SimTime, World, WorldConfig};
#[test]
fn new_lod_fact_is_rejected_before_existing_destination_or_staging_bytes_change() {
    let world = World::new(WorldConfig::new(42, SimTime::EPOCH)).unwrap();
    let mut parts = world.to_parts();
    let building = parts.toy.buildings().ids()[0];
    let known = parts
        .toy
        .buildings()
        .ids()
        .iter()
        .chain(parts.toy.residents().ids())
        .copied()
        .collect();
    let mut events = EventStore::from_records(known, &parts.events).unwrap();
    let mut c = Candidate::new(
        SimTime::EPOCH,
        70,
        0,
        Payload::SliceLodChanged {
            building,
            from: 1,
            to: 3,
            demoted: 1,
            promoted: 0,
        },
        Origin::System(70),
    );
    c.lod = 3;
    c.actors
        .push(Actor {
            id: building,
            role: Role::Subject,
        })
        .unwrap();
    events.prepare(0, &[c]).unwrap().commit();
    parts.events = events.records().to_vec();
    let unsupported = World::from_parts(parts).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "worldsim-slice-lod-rejection-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let dest = dir.join("keep.wssave");
    let staged = dir.join("keep.wssave.saving");
    std::fs::write(&dest, b"destination sentinel").unwrap();
    std::fs::write(&staged, b"staging sentinel").unwrap();
    assert!(matches!(
        save(&unsupported, &dest),
        Err(SaveError::Unsaveable(_))
    ));
    assert_eq!(std::fs::read(&dest).unwrap(), b"destination sentinel");
    assert_eq!(std::fs::read(&staged).unwrap(), b"staging sentinel");
    std::fs::remove_dir_all(dir).unwrap();
}
