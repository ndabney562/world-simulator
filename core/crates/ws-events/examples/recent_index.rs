//! ADR0032 bounded actor-query memory/latency probe, no simulation clock input.
use std::{hint::black_box, time::Instant};
use ws_ecs::EntityId;
use ws_events::{Actor, Candidate, EventStore, Origin, Payload, Role};
use ws_time::SimTime;
#[allow(clippy::disallowed_methods)]
fn main() {
    let resident = EntityId::spawned(1).unwrap();
    let destination = EntityId::spawned(2).unwrap();
    println!("events,index_and_arena_capacity_bytes,recent_result_capacity_bytes,median_recent_ns");
    for count in [1000, 10000, 100000] {
        let mut store = EventStore::new(vec![resident, destination]);
        for from in (0..count).step_by(256) {
            let candidates: Vec<_> = (from..(from + 256).min(count))
                .map(|i| {
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
                    c.actors
                        .push(Actor {
                            id: resident,
                            role: Role::Subject,
                        })
                        .unwrap();
                    c.actors
                        .push(Actor {
                            id: destination,
                            role: Role::Object,
                        })
                        .unwrap();
                    c
                })
                .collect();
            store.prepare(from as u64, &candidates).unwrap().commit();
        }
        let mut durations = Vec::new();
        for _ in 0..9 {
            let before = Instant::now();
            for _ in 0..1000 {
                black_box(store.recent(resident, SimTime::MAX).unwrap());
            }
            durations.push(before.elapsed().as_nanos() / 1000);
        }
        durations.sort_unstable();
        let result = store.recent(resident, SimTime::MAX).unwrap();
        assert_eq!(result.records.len(), 16);
        assert!(result.more);
        assert_eq!(result.records[0].id.0, count as u64);
        println!(
            "{count},{},{},{}",
            store.capacity_bytes(),
            result.records.capacity() * size_of::<&ws_events::Event>(),
            durations[4]
        );
    }
}
