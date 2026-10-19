//! Reproducible actor-index publication/query/capacity probe for issue #49.
//! Run: cargo run --release --locked -p ws-events --example actor_index
use std::{hint::black_box, time::Instant};
use ws_ecs::EntityId;
use ws_events::{Actor, Candidate, Event, EventStore, Origin, Payload, Role};
use ws_time::SimTime;

fn candidate(i: usize, ids: &[EntityId]) -> Candidate {
    let resident = ids[i % (ids.len() - 1)];
    let destination = *ids.last().unwrap();
    let mut c = Candidate::new(
        SimTime::from_micros_since_epoch(i as i64),
        30,
        i as u64,
        Payload::Arrived {
            resident,
            destination,
            leg: i as u32,
        },
        Origin::System(30),
    );
    c.lod = 1;
    for (id, role) in [(resident, Role::Subject), (destination, Role::Object)] {
        c.actors.push(Actor { id, role }).unwrap();
    }
    c
}
fn append(store: &mut EventStore, ids: &[EntityId], from: usize, len: usize) {
    let candidates: Vec<_> = (from..from + len).map(|i| candidate(i, ids)).collect();
    store.prepare(from as u64, &candidates).unwrap().commit();
}
// Diagnostic executable only: timing is its output, never simulation input.
// clippy.toml permits justified local clock use, as in ws-math benches.
#[allow(clippy::disallowed_methods)]
fn main() {
    let ids: Vec<_> = (0..1001).map(|i| EntityId::spawned(i).unwrap()).collect();
    println!(
        "events,event_size,allocated_arena_indices_bytes,registered_actor_capacity_bytes,median_publish_ns_per_event,median_actor_query_ns"
    );
    for n in [1_000, 10_000, 100_000] {
        let mut store = EventStore::new(ids.clone());
        for from in (0..n).step_by(256) {
            append(&mut store, &ids, from, 256.min(n - from));
        }
        let bytes = store.capacity_bytes();
        let mut publish = Vec::new();
        for sample in 0..9 {
            let from = n + sample * 256;
            let candidates: Vec<_> = (from..from + 256).map(|i| candidate(i, &ids)).collect();
            let start = Instant::now();
            store.prepare(from as u64, &candidates).unwrap().commit();
            publish.push(start.elapsed().as_nanos() / 256);
        }
        publish.sort_unstable();
        let actor = ids[42];
        let expected: Vec<_> = store
            .records()
            .iter()
            .filter(|e| e.record.actors.iter().any(|a| a.id == actor))
            .collect();
        assert_eq!(
            store.query(Some(actor), SimTime::MIN, SimTime::MAX),
            expected
        );
        let mut queries = Vec::new();
        for _ in 0..9 {
            let start = Instant::now();
            for _ in 0..100 {
                black_box(store.query(Some(actor), SimTime::MIN, SimTime::MAX));
            }
            queries.push(start.elapsed().as_nanos() / 100);
        }
        queries.sort_unstable();
        println!(
            "{n},{},{bytes},{},{},{}",
            size_of::<Event>(),
            ids.len() * size_of::<EntityId>(),
            publish[4],
            queries[4]
        );
    }
}
