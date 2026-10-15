//! Unit tests that need the world's private columns: injected faults the
//! checker must catch (ADR 0017 §12), drain ordering, and sizes.

use super::*;
use crate::{Dataset, Population, Resident, WalkParams};

const STEP: Rate = match Rate::new(SimDuration::from_micros(100_000)) {
    Ok(r) => r,
    Err(_) => unreachable!(),
};

fn toy(seed: u64) -> ToyWorld {
    ToyWorld::generate(
        seed,
        &Dataset::toy().unwrap(),
        Population::TOY,
        WalkParams::DEFAULT,
    )
    .unwrap()
}

fn t(ticks: i64) -> SimTime {
    SimTime::EPOCH
        .checked_add(SimDuration::from_micros(ticks * 100_000))
        .unwrap()
}

/// A world run for `ticks` ticks with every other building at L2, so both
/// stores hold walking and waiting agents.
fn mixed(seed: u64, ticks: i64) -> ToyWorld {
    let mut w = toy(seed);
    let req: Vec<(EntityId, Lod)> = w
        .buildings
        .ids()
        .iter()
        .enumerate()
        .map(|(i, b)| (*b, if i % 2 == 0 { Lod::L2 } else { Lod::L1 }))
        .collect();
    w.set_lod(t(0), STEP, &req).unwrap();
    for k in 0..ticks {
        w.tick(t(k), STEP).unwrap();
    }
    w
}

fn violations(w: &ToyWorld, now: SimTime) -> Vec<LodViolation> {
    w.check_lod_invariants(now, STEP, false)
        .err()
        .unwrap_or_default()
}

fn any_l1(w: &ToyWorld) -> EntityId {
    w.walks.ids()[0]
}

fn any_l2(w: &ToyWorld) -> EntityId {
    w.background.ids()[0]
}

#[test]
fn a_clean_mixed_world_has_no_violations() {
    for seed in 0..5 {
        let w = mixed(seed, 3_000);
        assert!(!w.walks.is_empty() && !w.background.is_empty());
        assert_eq!(violations(&w, t(3_000)), []);
    }
}

#[test]
fn checker_catches_an_agent_in_both_stores() {
    let mut w = mixed(1, 2_000);
    let id = any_l1(&w);
    let bg = Background {
        activity: Activity::Waiting {
            at: w.walks.get(id).unwrap().target,
            depart: t(5_000),
        },
        targets_drawn: 0,
    };
    w.background.insert(id, bg).unwrap();
    w.queue.insert((bg.next_event(), id));
    assert!(violations(&w, t(2_000)).contains(&LodViolation::Duplicated(id)));
}

#[test]
fn checker_catches_an_agent_in_neither_store() {
    let mut w = mixed(1, 2_000);
    let id = any_l2(&w);
    let bg = w.background.remove(id).unwrap();
    w.queue.remove(&(bg.next_event(), id));
    assert_eq!(violations(&w, t(2_000)), [LodViolation::Lost(id)]);

    let mut w = mixed(1, 2_000);
    let id = any_l1(&w);
    w.walks.remove(id);
    let v = violations(&w, t(2_000));
    assert!(v.contains(&LodViolation::PartialL1(id)), "{v:?}");
}

#[test]
fn checker_catches_state_without_identity() {
    let mut w = mixed(1, 2_000);
    let id = any_l2(&w);
    w.residents.remove(id);
    assert!(violations(&w, t(2_000)).contains(&LodViolation::Orphan(id)));
}

#[test]
fn checker_catches_an_identity_not_derived_from_its_address() {
    let mut w = mixed(1, 10);
    let id = any_l1(&w);
    w.residents.get_mut(id).unwrap().address.slot += 1;
    assert!(violations(&w, t(10)).contains(&LodViolation::IdentityMismatch(id)));
}

#[test]
fn checker_catches_invalid_building_references() {
    let ghost = EntityId::procedural(0x1234_5678_9abc_def0);
    // An L2 schedule naming a building that does not exist.
    let mut w = mixed(2, 2_000);
    let id = any_l2(&w);
    let bg = w.background.get_mut(id).unwrap();
    match &mut bg.activity {
        Activity::Waiting { at, .. } => *at = ghost,
        Activity::Walking { dest, .. } => *dest = ghost,
    }
    assert!(
        violations(&w, t(2_000)).contains(&LodViolation::UnknownBuilding {
            id,
            building: ghost
        })
    );
    // An L1 target and a home that do not exist.
    let mut w = mixed(2, 2_000);
    let id = any_l1(&w);
    w.walks.get_mut(id).unwrap().target = ghost;
    w.residents.get_mut(id).unwrap().home = ghost;
    let v = violations(&w, t(2_000));
    assert_eq!(
        v.iter()
            .filter(|x| **x
                == LodViolation::UnknownBuilding {
                    id,
                    building: ghost
                })
            .count(),
        2,
        "{v:?}"
    );
}

#[test]
fn checker_catches_an_event_queue_that_disagrees_with_the_l2_store() {
    // A missing entry.
    let mut w = mixed(3, 2_000);
    let id = any_l2(&w);
    let entry = (w.background.get(id).unwrap().next_event(), id);
    w.queue.remove(&entry);
    assert_eq!(violations(&w, t(2_000)), [LodViolation::QueueMismatch(id)]);
    // A wrong time.
    w.queue.insert((t(999_999), id));
    assert_eq!(violations(&w, t(2_000)), [LodViolation::QueueMismatch(id)]);
    // An entry for an L1 agent.
    let mut w = mixed(3, 2_000);
    let l1 = any_l1(&w);
    w.queue.insert((t(9_000), l1));
    assert_eq!(violations(&w, t(2_000)), [LodViolation::QueueMismatch(l1)]);
    // The drain refuses to run on it, and changes nothing.
    let before = w.clone();
    assert!(matches!(
        w.advance_background(t(10_000), STEP),
        Err(LodError::Invariant(LodViolation::QueueMismatch(_)))
    ));
    assert_eq!(w, before);
}

#[test]
fn checker_catches_an_unreconciled_index_that_disagrees_with_residency() {
    // A matching agent listed as unreconciled.
    let mut w = mixed(4, 2_000);
    let id = *w
        .walks
        .ids()
        .iter()
        .find(|id| !w.unreconciled.contains(id))
        .unwrap();
    w.unreconciled.insert(id);
    assert_eq!(
        violations(&w, t(2_000)),
        [LodViolation::UnreconciledMismatch(id)]
    );
    // A mismatched agent missing from the index: a tick would not move it.
    let mut w = mixed(4, 2_000);
    let id = *w
        .background
        .ids()
        .iter()
        .find(|id| !w.unreconciled.contains(id))
        .unwrap();
    let b = w.background.get(id).unwrap().governing();
    let before = w.clone();
    *w.node_lod.get_mut(b).unwrap() = Lod::L1;
    let v = violations(&w, t(2_000));
    assert!(v.contains(&LodViolation::UnreconciledMismatch(id)), "{v:?}");
    assert!(
        v.iter()
            .all(|x| matches!(x, LodViolation::UnreconciledMismatch(_))),
        "{v:?}"
    );
    // A clean run keeps the index equal to its rebuild.
    assert!(violations(&before, t(2_000)).is_empty());
}

#[test]
fn checker_catches_malformed_l2_times() {
    let base = mixed(4, 2_000);
    let now = t(2_000);
    let id = any_l2(&base);
    let bad = |w: &ToyWorld| {
        violations(w, now)
            .into_iter()
            .filter_map(|v| match v {
                LodViolation::BadSchedule { id: i, problem } if i == id => Some(problem),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let with = |activity: Activity| {
        let mut w = base.clone();
        let old = *w.background.get(id).unwrap();
        w.queue.remove(&(old.next_event(), id));
        let bg = Background {
            activity,
            targets_drawn: 3,
        };
        *w.background.get_mut(id).unwrap() = bg;
        w.queue.insert((bg.next_event(), id));
        w
    };
    let b = base.buildings.ids()[0];
    let off = now.checked_add(SimDuration::from_micros(50)).unwrap();
    assert_eq!(
        bad(&with(Activity::Waiting { at: b, depart: off })),
        [ScheduleProblem::OffGrid]
    );
    assert_eq!(
        bad(&with(Activity::Waiting { at: b, depart: now })),
        [ScheduleProblem::EventNotInFuture]
    );
    let leg = |origin_time, arrive| Activity::Walking {
        origin: Origin::Building(b),
        origin_time,
        dest: b,
        arrive,
    };
    assert_eq!(
        bad(&with(leg(t(2_001), t(2_005)))),
        [ScheduleProblem::OriginInFuture]
    );
    assert_eq!(
        bad(&with(leg(t(1_990), t(1_995)))),
        [ScheduleProblem::EventNotInFuture]
    );
    // A Point origin outside its own cell.
    let mut w = with(leg(t(1_990), t(2_010)));
    if let Activity::Walking { origin, .. } = &mut w.background.get_mut(id).unwrap().activity {
        let p = base.buildings.get(b).unwrap().pos;
        *origin = Origin::Point {
            cell: p.cell(),
            offset: [5_000.0, 0.0, 0.0],
        };
    }
    assert!(violations(&w, now).contains(&LodViolation::NotInOwnCell(id)));
    // Promoting such schedules fails and changes nothing.
    for activity in [
        Activity::Waiting { at: b, depart: off },
        leg(t(2_001), t(2_005)),
    ] {
        let mut w = with(activity);
        let before = w.clone();
        let err = w.set_lod(now, STEP, &[(b, Lod::L1)]).unwrap_err();
        assert!(
            matches!(err, LodError::Invariant(LodViolation::BadSchedule { .. })),
            "{err:?}"
        );
        assert_eq!(w, before);
    }
}

#[test]
fn checker_catches_an_arrived_agent_away_from_its_centre() {
    let mut w = toy(5);
    let id = any_l1(&w);
    let pos = w.positions.get_mut(id).unwrap();
    *pos = SurfacePos::new(pos.cell(), [pos.offset()[0] + 0.5, pos.offset()[1], 0.0]).unwrap();
    assert!(violations(&w, t(0)).contains(&LodViolation::NotAtCentre(id)));
    // Demotion rejects it rather than snapping it, and changes nothing.
    let target = w.walks.get(id).unwrap().target;
    let before = w.clone();
    assert_eq!(
        w.set_lod(t(0), STEP, &[(target, Lod::L2)]),
        Err(LodError::Invariant(LodViolation::NotAtCentre(id)))
    );
    assert_eq!(w, before);
}

#[test]
fn checker_catches_invalid_node_levels() {
    let mut w = toy(6);
    let b = w.buildings.ids()[2];
    *w.node_lod.get_mut(b).unwrap() = Lod::L3;
    assert!(violations(&w, t(0)).contains(&LodViolation::InvalidNodeLod(b)));
    w.node_lod.remove(b);
    assert!(violations(&w, t(0)).contains(&LodViolation::MissingNodeLod(b)));
}

#[test]
fn checker_catches_residency_that_disagrees_with_the_node() {
    let mut w = mixed(7, 100);
    let b = w.buildings.ids()[0];
    // Change the level without moving anyone.
    let level = w.node_lod(b).unwrap();
    *w.node_lod.get_mut(b).unwrap() = if level == Lod::L1 { Lod::L2 } else { Lod::L1 };
    let v = w.check_lod_invariants(t(100), STEP, true).unwrap_err();
    let governed: BTreeSet<EntityId> = v
        .iter()
        .filter_map(|x| match x {
            LodViolation::ResidencyMismatch { id, governing, .. } if *governing == b => Some(*id),
            _ => None,
        })
        .collect();
    assert!(!governed.is_empty());
    // The derived unreconciled index is now wrong too, but only for agents
    // governed by `b`; nothing else is reported.
    let unindexed: Vec<EntityId> = v
        .iter()
        .filter_map(|x| match x {
            LodViolation::UnreconciledMismatch(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(unindexed.len() + governed.len(), v.len(), "{v:?}");
    for id in unindexed {
        assert_eq!(w.governing_building(id), Some(b), "{id}");
    }
}

/// Broken transitions: each fault the issue and ADR 0017 §12 name is
/// reported by the conservation comparison or the checker.
#[test]
fn checker_catches_injected_faults() {
    let w = mixed(8, 1_500);
    let now = t(1_500);
    let before = w.conservation_summary();
    let id = any_l1(&w);
    let walk = *w.walks.get(id).unwrap();
    let pos = *w.positions.get(id).unwrap();
    let bg = w.demote(id, pos, walk, now, STEP).unwrap();

    // Inserted into L2 without being removed from L1.
    let mut dup = w.clone();
    dup.background.insert(id, bg).unwrap();
    dup.queue.insert((bg.next_event(), id));
    let v = violations(&dup, now);
    assert!(v.contains(&LodViolation::Duplicated(id)), "{v:?}");
    assert!(
        before
            .compare_transaction(&dup.conservation_summary())
            .contains(&LodViolation::CountMismatch {
                quantity: Quantity::Population,
                before: before.population,
                after: before.population + 1,
            })
    );

    // Removed from L1 without being inserted into L2.
    let mut lost = w.clone();
    lost.walks.remove(id);
    lost.positions.remove(id);
    assert_eq!(violations(&lost, now), [LodViolation::Lost(id)]);
    assert!(
        before
            .compare_transaction(&lost.conservation_summary())
            .contains(&LodViolation::Lost(id))
    );

    // A counter changed by a transition.
    let mut counter = w.clone();
    counter.move_to_l2(vec![(
        id,
        Background {
            targets_drawn: bg.targets_drawn + 1,
            ..bg
        },
    )]);
    // The demotion bypasses `set_lod`, so its building stays L1; keep the
    // derived index consistent so only the injected fault remains.
    counter.note_residency(id, bg.governing(), Lod::L2);
    assert_eq!(violations(&counter, now), []);
    assert_eq!(
        before.compare_transaction(&counter.conservation_summary()),
        [LodViolation::CounterChanged(id)]
    );

    // A correct transition: nothing.
    let mut ok = w.clone();
    ok.move_to_l2(vec![(id, bg)]);
    ok.note_residency(id, bg.governing(), Lod::L2);
    assert_eq!(violations(&ok, now), []);
    assert_eq!(before.compare_transaction(&ok.conservation_summary()), []);
}

#[test]
fn an_identity_change_across_a_transition_is_reported() {
    let w = mixed(9, 10);
    let before = w.conservation_summary();
    let mut changed = w.clone();
    let id = any_l2(&changed);
    let other = changed.buildings.ids()[4];
    changed.residents.get_mut(id).unwrap().home = other;
    let v = before.compare_transaction(&changed.conservation_summary());
    assert!(v.contains(&LodViolation::IdentityChanged(id)), "{v:?}");
}

#[test]
fn a_failed_tick_is_undone_completely() {
    // A counter that is about to overflow fails the L2 drain after the
    // reconciliation has already moved agents; the tick undoes both.
    let mut w = mixed(10, 50);
    let flip = w.buildings.ids()[1];
    let level = w.node_lod(flip).unwrap();
    *w.node_lod.get_mut(flip).unwrap() = if level == Lod::L1 { Lod::L2 } else { Lod::L1 };
    let (id, bg) = w
        .background
        .iter()
        .find(|(_, bg)| bg.governing() != flip)
        .map(|(id, bg)| (id, *bg))
        .unwrap();
    let at = bg.governing();
    w.queue.remove(&(bg.next_event(), id));
    let soon = Background {
        activity: Activity::Waiting { at, depart: t(51) },
        targets_drawn: u32::MAX,
    };
    *w.background.get_mut(id).unwrap() = soon;
    w.queue.insert((soon.next_event(), id));
    let before = w.clone();
    assert_eq!(
        w.tick(t(50), STEP),
        Err(WorldError::Lod(LodError::CounterExhausted(id)))
    );
    assert_eq!(w, before);
}

#[test]
fn drain_processes_events_in_time_then_id_order() {
    let mut w = toy(11);
    let all: Vec<(EntityId, Lod)> = w.buildings.ids().iter().map(|b| (*b, Lod::L2)).collect();
    w.set_lod(t(0), STEP, &all).unwrap();
    // Every agent departs at t(1): equal times, so ID order decides.
    let drain = w.plan_drain(t(1), STEP).unwrap();
    let order: Vec<(SimTime, EntityId)> = drain.stale.clone();
    assert_eq!(order.len(), w.background.len());
    assert!(order.windows(2).all(|p| p[0] < p[1]));
    assert!(order.iter().all(|(time, _)| *time == t(1)));
    // Over a longer span the first touches are still in (time, ID) order.
    let drain = w.plan_drain(t(20_000), STEP).unwrap();
    assert!(drain.stale.windows(2).all(|p| p[0] < p[1]));
}

#[test]
fn l2_records_fit_the_memory_target() {
    // ADR 0017 §3: at most 64 bytes per agent in the L2 column, ID copy
    // included.
    assert_eq!(size_of::<Origin>(), 24);
    assert_eq!(size_of::<Activity>(), 48);
    assert_eq!(size_of::<Background>(), 56);
    assert_eq!(size_of::<Background>() + size_of::<EntityId>(), 64);
    // Identity entry (shared by every level) and queue entry.
    assert_eq!(size_of::<Resident>() + size_of::<EntityId>(), 24);
    assert_eq!(size_of::<(SimTime, EntityId)>(), 16);
}
