use super::*;
use commands::{Action, Command, IngressError, IssuedBy, Rejection};
fn world() -> World {
    World::new(WorldConfig::new(42, SimTime::EPOCH)).unwrap()
}
fn cmd(a: Action) -> Command {
    Command::new(IssuedBy::Test, a)
}
#[test]
fn ingress_is_atomic_and_never_allocates_sequences_on_failure() {
    let mut w = world();
    let before = w.checkpoint();
    assert_eq!(
        w.submit(vec![cmd(Action::NoOp); 1025]),
        Err(IngressError::Capacity)
    );
    assert_eq!(w.checkpoint(), before);
    assert_eq!(w.submit_bytes(&[2, 0, 0]), Err(IngressError::Malformed));
    assert_eq!(w.checkpoint(), before);
    let mut batch = commands::encode_batch(&[cmd(Action::NoOp)]).unwrap();
    batch[16] = 2;
    assert_eq!(w.submit_bytes(&batch), Err(IngressError::Version));
    assert_eq!(w.checkpoint(), before);
    assert_eq!(
        w.submit(vec![cmd(Action::NoOp); 1024])
            .unwrap()
            .first_sequence,
        1
    );
    let queued = w.checkpoint();
    assert_eq!(
        w.submit(vec![cmd(Action::NoOp)]),
        Err(IngressError::Capacity)
    );
    assert_eq!(w.checkpoint(), queued);
    w.flush_commands().unwrap();
    assert_eq!(w.journal().len(), 1024);
    assert_eq!(
        w.submit(vec![cmd(Action::NoOp)]).unwrap().first_sequence,
        1025
    );
}
#[test]
fn semantic_rejections_and_noops_preserve_state_and_fifo() {
    let mut w = world();
    let id = w.toy.buildings().ids()[0];
    let hash = w.state_hash();
    let mut bad = cmd(Action::NoOp);
    bad.schema_version = 99;
    let unknown = EntityId::spawned(1).unwrap();
    w.submit(vec![
        bad,
        cmd(Action::SetNodeLod(vec![(unknown, Lod::L1)])),
        cmd(Action::SetNodeLod(vec![(id, Lod::L2), (id, Lod::L1)])),
        cmd(Action::NoOp),
    ])
    .unwrap();
    assert_eq!(w.state_hash(), hash);
    w.flush_commands().unwrap();
    assert_eq!(w.state_hash(), hash);
    assert_eq!(w.journal()[0].result, Err(Rejection::Schema));
    assert!(w.journal()[1].result.is_err());
    assert!(w.journal()[2].result.is_err());
    assert_eq!(w.journal()[3].result, Ok(()));
    assert_eq!(
        w.journal().iter().map(|o| o.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(w.journal().iter().all(|o| o.boundary == 1));
    assert_eq!(w.events.records().len(), 2);
}
#[test]
fn pause_resume_repeated_flushes_and_explicit_step() {
    let mut w = world();
    let original = w.state_hash();
    w.submit(vec![cmd(Action::SetTimeScale(TimeScale::Paused))])
        .unwrap();
    assert_eq!(w.advance_by_host_elapsed(1_000_000), Ok(0));
    assert_ne!(w.state_hash(), original);
    w.step().unwrap();
    assert_eq!(w.tick(), 1);
    w.submit(vec![cmd(Action::NoOp)]).unwrap();
    w.flush_commands().unwrap();
    w.flush_commands().unwrap();
    w.submit(vec![cmd(Action::SetTimeScale(TimeScale::X10))])
        .unwrap();
    assert_eq!(w.advance_by_host_elapsed(10_000), Ok(1));
    assert_eq!(
        w.journal().iter().map(|o| o.boundary).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(w.tick(), 2);
}
#[test]
fn lod_events_are_actual_changes_and_have_honest_cause() {
    use ws_events::{Payload, Target};
    let mut w = world();
    let req: Vec<_> = w
        .toy
        .buildings()
        .ids()
        .iter()
        .map(|id| (*id, Lod::L2))
        .collect();
    let c = cmd(Action::SetNodeLod(req));
    w.submit(vec![c.clone(), c]).unwrap();
    w.flush_commands().unwrap();
    let events = w.events.records();
    assert_eq!(events.len(), 8);
    assert!(matches!(
        events[1].record.payload,
        Payload::CommandApplied { effect: 5, .. }
    ));
    for e in &events[2..7] {
        assert!(matches!(e.record.payload, Payload::LodChanged { .. }));
        assert_eq!(
            e.record.causes.iter().next().unwrap().target,
            Target::Id(events[1].id)
        );
    }
    assert!(matches!(
        events[7].record.payload,
        Payload::CommandApplied { effect: 0, .. }
    ));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.record.payload, Payload::Arrived { .. }))
    );
}
#[test]
fn partial_flush_resource_failure_preserves_suffix_and_retry_ordinals() {
    let mut w = world();
    let mut replay = w.clone();
    let req = vec![(w.toy.buildings().ids()[0], Lod::L2)];
    let commands = vec![
        cmd(Action::NoOp),
        cmd(Action::SetNodeLod(req)),
        cmd(Action::NoOp),
    ];
    w.submit(commands.clone()).unwrap();
    replay.submit(commands).unwrap();
    w.fail_after = Some(1);
    assert_eq!(w.flush_commands(), Err(ControlError::Resource));
    assert_eq!(w.journal.len(), 1);
    assert_eq!(w.pending.len(), 2);
    assert_eq!(w.events.records().len(), 2);
    replay.flush_prefix(Some(1)).unwrap();
    assert_eq!(w.checkpoint(), replay.checkpoint());
    assert_eq!(w.toy, replay.toy);
    w.fail_after = None;
    w.flush_commands().unwrap();
    replay.flush_commands().unwrap();
    assert_eq!(w, replay);
    assert_eq!(
        w.journal.iter().map(|o| o.boundary).collect::<Vec<_>>(),
        vec![1, 2, 2]
    );
}
#[test]
fn zero_prefix_and_failed_tick_publish_nothing_then_retry() {
    let mut w = world();
    w.submit(vec![cmd(Action::SetTimeScale(TimeScale::Paused))])
        .unwrap();
    let before = w.checkpoint();
    w.fail_after = Some(0);
    assert_eq!(w.flush_commands(), Err(ControlError::Resource));
    assert_eq!(w.checkpoint(), before);
    assert_eq!(w.boundary, 0);
    w.fail_after = None;
    w.flush_commands().unwrap();
    let after = w.clone();
    w.fail_after = Some(0);
    assert_eq!(
        w.physical_step(),
        Err(StepError::History(EventError::Resource))
    );
    assert_eq!(w.checkpoint(), after.checkpoint());
    assert_eq!(w.toy, after.toy);
    assert_eq!(w.clock, after.clock);
    w.fail_after = None;
    w.step().unwrap();
    assert_eq!(w.journal.len(), 1);
    assert_eq!(w.tick(), 1);
}
#[test]
fn failed_physical_tick_retains_committed_commands() {
    let rate = Clock::with_defaults(SimTime::EPOCH).base_step();
    let start = rate.tick_start(rate.tick_index(SimTime::MAX)).unwrap();
    let mut w = World::new(WorldConfig::new(1, start)).unwrap();
    w.submit(vec![cmd(Action::SetTimeScale(TimeScale::Paused))])
        .unwrap();
    assert_eq!(w.step(), Err(StepError::Time(TimeOverflow)));
    assert_eq!(w.clock.scale(), TimeScale::Paused);
    assert_eq!(w.journal.len(), 1);
    let after = w.checkpoint();
    assert_eq!(w.step(), Err(StepError::Time(TimeOverflow)));
    assert_eq!(w.checkpoint(), after);
}
#[test]
fn domain_evolution_does_not_consume_history() {
    let mut w = world();
    let mut toy = w.toy.clone();
    for _ in 0..10_000 {
        toy.tick(w.now(), w.clock.base_step()).unwrap();
        w.step().unwrap();
        assert_eq!(w.toy, toy);
    }
    assert!(w.events.records().len() > 1);
}
#[test]
fn tick_reservation_failure_rolls_back_reconciliation_and_arrivals() {
    let mut w = world();
    for _ in 0..1000 {
        let mut expected = w.clone();
        expected.step().unwrap();
        let before = w.clone();
        w.fail_after = Some(0);
        assert!(w.physical_step().is_err());
        assert_eq!(w.toy, before.toy);
        assert_eq!(w.checkpoint(), before.checkpoint());
        w.fail_after = None;
        w.step().unwrap();
        assert_eq!(w, expected);
    }
}

#[test]
fn zero_enqueue_resource_stop_changes_no_input_state() {
    let mut w = world();
    let before = w.checkpoint();
    w.fail_after = Some(0);
    assert_eq!(
        w.submit(vec![cmd(Action::NoOp)]),
        Err(IngressError::Resource)
    );
    assert_eq!(w.checkpoint(), before);
    w.fail_after = None;
    assert_eq!(w.submit(vec![cmd(Action::NoOp)]).unwrap().first_sequence, 1);
}
#[test]
fn agent_history_answers_actual_arrivals_at_both_lods() {
    use ws_events::Payload;
    for level in [Lod::L1, Lod::L2] {
        let mut w = world();
        let req = w
            .toy()
            .buildings()
            .ids()
            .iter()
            .map(|id| (*id, level))
            .collect();
        w.submit(vec![cmd(Action::SetNodeLod(req))]).unwrap();
        w.flush_commands().unwrap();
        assert!(
            !w.events
                .records()
                .iter()
                .any(|e| matches!(e.record.payload, Payload::Arrived { .. }))
        );
        for _ in 0..10_000 {
            w.step().unwrap();
        }
        let arrival = w
            .events
            .records()
            .iter()
            .find(|e| matches!(e.record.payload, Payload::Arrived { .. }))
            .expect("real arrivals in 1000 seconds");
        let Payload::Arrived { resident, .. } = arrival.record.payload else {
            unreachable!()
        };
        let result = w.events.query(Some(resident), SimTime::EPOCH, w.now());
        assert!(!result.is_empty());
        assert!(
            result.iter().all(
                |e| matches!(e.record.payload,Payload::Arrived{resident:id,..} if id==resident)
            )
        );
        assert!(
            result
                .iter()
                .all(|e| e.record.lod == if level == Lod::L1 { 1 } else { 2 })
        );
        let rebuilt = EventStore::from_records(
            w.toy
                .buildings()
                .ids()
                .iter()
                .chain(w.toy.residents().ids())
                .copied()
                .collect(),
            w.events.records(),
        )
        .unwrap();
        assert_eq!(rebuilt, w.events);
    }
}

#[test]
fn lod_history_failure_leaves_entire_command_pending_and_domain_unchanged() {
    let mut w = world();
    let req = w
        .toy
        .buildings()
        .ids()
        .iter()
        .map(|id| (*id, Lod::L2))
        .collect();
    w.submit(vec![cmd(Action::SetNodeLod(req))]).unwrap();
    let before = w.clone();
    w.fail_history = true;
    assert_eq!(w.flush_commands(), Err(ControlError::Resource));
    assert_eq!(w.checkpoint(), before.checkpoint());
    assert_eq!(w.toy, before.toy);
    assert_eq!(w.clock, before.clock);
    w.fail_history = false;
    w.flush_commands().unwrap();
    assert_eq!(w.pending.len(), 0);
    assert_eq!(w.journal.len(), 1);
    assert_eq!(w.events.records().len(), 7);
}

#[test]
fn physical_primitive_rejects_unflushed_commands() {
    let mut w = world();
    w.submit(vec![cmd(Action::NoOp)]).unwrap();
    let before = w.checkpoint();
    assert_eq!(
        w.physical_step(),
        Err(StepError::Control(ControlError::Fence))
    );
    assert_eq!(w.checkpoint(), before);
    w.step().unwrap();
    assert_eq!(w.journal.len(), 1);
    assert_eq!(w.tick(), 1);
}

#[test]
fn environmental_ingress_stops_cannot_mask_malformed_or_logically_full_input() {
    let mut w = world();
    let batch = commands::encode_batch(&[cmd(Action::NoOp)]).unwrap();
    w.fail_after = Some(0);
    let before = w.checkpoint();
    let mut bad = batch.clone();
    bad[16] = 99;
    assert_eq!(w.submit_bytes(&bad), Err(IngressError::Version));
    assert_eq!(w.submit_bytes(&batch), Err(IngressError::Resource));
    assert_eq!(w.checkpoint(), before);
    w.fail_after = None;
    w.submit(vec![cmd(Action::NoOp); 1024]).unwrap();
    w.fail_after = Some(0);
    assert_eq!(w.submit_bytes(&batch), Err(IngressError::Capacity));
}

#[test]
fn exhausted_command_counters_never_wrap_or_publish() {
    let mut w = world();
    w.next_sequence = u64::MAX;
    let before = w.checkpoint();
    assert_eq!(
        w.submit(vec![cmd(Action::NoOp)]),
        Err(IngressError::Capacity)
    );
    assert_eq!(w.checkpoint(), before);
    w.next_sequence = 1;
    w.submit(vec![cmd(Action::NoOp)]).unwrap();
    w.boundary = u64::MAX;
    let before = w.checkpoint();
    assert_eq!(w.flush_commands(), Err(ControlError::Counter));
    assert_eq!(w.checkpoint(), before);
    assert!(w.journal.is_empty());
}

#[test]
fn maximum_noop_flush_preserves_world_and_records_every_outcome() {
    let mut w = World::new(WorldConfig {
        population: Population::exactly(1000),
        ..WorldConfig::new(42, SimTime::EPOCH)
    })
    .unwrap();
    let state = w.state_hash();
    w.submit(vec![cmd(Action::NoOp); 1024]).unwrap();
    let report = w.flush_commands().unwrap();
    assert_eq!(report.committed, 1024);
    assert_eq!(report.next, 0);
    assert_eq!(w.state_hash(), state);
    assert_eq!(w.tick(), 0);
    assert_eq!(w.events().records().len(), 1025);
    for (i, outcome) in w.journal().iter().enumerate() {
        assert_eq!(outcome.sequence, i as u64 + 1);
        assert_eq!(outcome.result, Ok(()));
        assert_eq!(outcome.command.action, Action::NoOp);
    }
    let checkpoint = w.checkpoint();
    w.flush_commands().unwrap();
    assert_eq!(w.checkpoint(), checkpoint);
}
