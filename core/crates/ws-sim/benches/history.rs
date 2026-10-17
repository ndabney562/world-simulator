//! Event memory/query and bounded 5,000-agent history overhead measurements.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use ws_sim::commands::{Action, Command, IssuedBy};
use ws_sim::{EntityId, Population, SimTime, World, WorldConfig, ws_events::*};
fn history(n: usize) -> EventStore {
    let ids: Vec<_> = (0..1001).map(|i| EntityId::spawned(i).unwrap()).collect();
    let mut store = EventStore::new(ids.clone());
    let candidates: Vec<_> = (0..n)
        .map(|i| {
            let resident = ids[i % 1000];
            let destination = ids[1000];
            let mut c = Candidate::new(
                SimTime::from_micros_since_epoch(i as i64 * 100_000),
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
    store.prepare(1, &candidates).unwrap().commit();
    store
}
fn measure(c: &mut Criterion) {
    let small = history(1000);
    println!(
        "HISTORY_MEMORY records={} event_size={} arena_and_indices_bytes={} bytes_per_event={}",
        small.records().len(),
        size_of::<Event>(),
        small.capacity_bytes(),
        small.capacity_bytes() / small.records().len()
    );
    let store = history(10_000);
    let id = EntityId::spawned(42).unwrap();
    c.bench_function("history_actor_query_10_of_10000", |b| {
        b.iter(|| black_box(store.query(Some(id), SimTime::MIN, SimTime::MAX)))
    });
    // Setup and submission are outside timing; flush includes outcome/event publication.
    for (name, population) in [
        ("flush_1024_noops_toy", Population::default()),
        ("flush_1024_noops_5000_agents", Population::exactly(1000)),
    ] {
        let mut group = c.benchmark_group(name);
        group.sample_size(10);
        group.bench_function("flush", |b| {
            b.iter_batched_ref(
                || {
                    let mut world = World::new(WorldConfig {
                        population,
                        ..WorldConfig::new(1, SimTime::EPOCH)
                    })
                    .unwrap();
                    world
                        .submit(vec![Command::new(IssuedBy::Test, Action::NoOp); 1024])
                        .unwrap();
                    world
                },
                |world| black_box(world.flush_commands().unwrap()),
                BatchSize::LargeInput,
            )
        });
        group.finish();
    }
    let mut w = World::new(WorldConfig {
        population: Population::exactly(1000),
        ..WorldConfig::new(1, SimTime::EPOCH)
    })
    .unwrap();
    for _ in 0..3000 {
        w.step().unwrap();
    }
    let mut group = c.benchmark_group("history_5000_agents_500_ticks");
    group.sample_size(10);
    group.bench_function("world_with_history", |b| {
        b.iter_batched(
            || w.clone(),
            |mut world| {
                for _ in 0..500 {
                    world.step().unwrap();
                }
                black_box(world)
            },
            BatchSize::LargeInput,
        )
    });
    group.bench_function("domain_without_history", |b| {
        b.iter_batched(
            || (w.toy().clone(), w.clock().clone()),
            |(mut toy, mut clock)| {
                for _ in 0..500 {
                    toy.tick(clock.now(), clock.base_step()).unwrap();
                    clock.step().unwrap();
                }
                black_box((toy, clock))
            },
            BatchSize::LargeInput,
        )
    });
    group.finish();
}
criterion_group!(benches, measure);
criterion_main!(benches);
