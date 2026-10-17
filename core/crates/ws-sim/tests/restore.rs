//! `World::to_parts` / `World::from_parts` (ADR 0020): a world rebuilt from
//! its parts is the same world, and parts that are not a world are refused.

use core::num::NonZeroU32;

use ws_sim::commands::{Action, Command, IssuedBy};
use ws_sim::{Clock, Lod, RestoreError, SimTime, TimeScale, World, WorldConfig};
use ws_time::{Rate, SimDuration};

fn busy_world() -> World {
    let mut w = World::new(WorldConfig::new(31, SimTime::EPOCH)).unwrap();
    let ids = w.toy().buildings().ids().to_vec();
    w.submit(vec![Command::new(
        IssuedBy::Test,
        Action::SetNodeLod(vec![(ids[0], Lod::L2), (ids[2], Lod::L2)]),
    )])
    .unwrap();
    for _ in 0..400 {
        w.step().unwrap();
    }
    w
}

#[test]
fn parts_round_trip_to_an_equal_world_with_rebuilt_indices() {
    let w = busy_world();
    assert!(!w.toy().event_queue().is_empty());
    let back = World::from_parts(w.to_parts()).unwrap();
    assert_eq!(back, w);
    assert_eq!(back.checkpoint(), w.checkpoint());
    // The derived queue was rebuilt, not copied.
    assert_eq!(back.toy().event_queue(), w.toy().event_queue());
}

#[test]
fn a_rebuilt_world_keeps_running_identically() {
    let mut a = busy_world();
    let mut b = World::from_parts(a.to_parts()).unwrap();
    for _ in 0..300 {
        a.step().unwrap();
        b.step().unwrap();
        assert_eq!(a.checkpoint(), b.checkpoint());
    }
}

#[test]
fn inconsistent_parts_are_refused() {
    let w = busy_world();

    let mut parts = w.to_parts();
    parts.seed ^= 1;
    assert_eq!(World::from_parts(parts), Err(RestoreError::SeedMismatch));

    let mut parts = w.to_parts();
    parts.next_sequence = 0;
    assert!(matches!(
        World::from_parts(parts),
        Err(RestoreError::Journal(_))
    ));

    let mut parts = w.to_parts();
    parts.next_sequence = parts.journal[0].sequence;
    assert!(matches!(
        World::from_parts(parts),
        Err(RestoreError::Journal(_))
    ));

    let mut parts = w.to_parts();
    parts.events.swap(0, 1);
    assert!(matches!(
        World::from_parts(parts),
        Err(RestoreError::History(_))
    ));

    let mut parts = w.to_parts();
    let mut toy = parts.toy.to_parts();
    toy.l2.pop();
    parts.toy = ws_sim::ToyWorld::from_parts(toy).unwrap();
    assert!(matches!(
        World::from_parts(parts),
        Err(RestoreError::Lod(_))
    ));
}

#[test]
fn clock_parts_must_describe_one_clock() {
    let w = busy_world();
    let c = w.clock();
    let rebuilt = Clock::from_parts(
        c.start(),
        c.base_step(),
        c.steps(),
        c.scale(),
        c.max_steps_per_call(),
        c.pending_micros(),
    )
    .unwrap();
    assert_eq!(&rebuilt, c);

    let step = Rate::new(SimDuration::from_micros(100_000)).unwrap();
    let cap = NonZeroU32::new(100).unwrap();
    // The start is not on the tick grid.
    assert!(
        Clock::from_parts(
            SimTime::from_micros_since_epoch(1),
            step,
            0,
            TimeScale::X1,
            cap,
            0
        )
        .is_err()
    );
    // A remainder of a whole step or more.
    assert!(Clock::from_parts(SimTime::EPOCH, step, 0, TimeScale::X1, cap, 100_000).is_err());
    // Steps that run past the end of time.
    assert!(Clock::from_parts(SimTime::MAX, step, u64::MAX, TimeScale::X1, cap, 0).is_err());
}
