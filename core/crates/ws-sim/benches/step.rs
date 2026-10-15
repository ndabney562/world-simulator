//! `World::step` with 50 and 5,000 agents, and the `ws-ecs` column
//! operations materialization relies on (#7, ADR 0016). Run with
//! `cargo bench -p ws-sim`. Results are printed, not asserted: timings
//! depend on the machine. The benchmark measures code and never feeds
//! simulation state.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use ws_ecs::{Column, EntityId};
use ws_sim::{Population, SimTime, World, WorldConfig};

/// A world with `per_building` residents in each of the five toy buildings,
/// run for 600 ticks (one minute) so most agents are walking.
fn warmed_world(per_building: u32) -> World {
    let config = WorldConfig {
        population: Population::exactly(per_building),
        ..WorldConfig::new(1, SimTime::EPOCH)
    };
    let mut w = World::new(config).unwrap();
    for _ in 0..600 {
        w.step().unwrap();
    }
    w
}

fn step(c: &mut Criterion) {
    let mut group = c.benchmark_group("world_step");
    for per_building in [10, 1_000] {
        let agents = per_building * 5;
        let world = warmed_world(per_building);
        group.bench_with_input(BenchmarkId::from_parameter(agents), &world, |b, w| {
            let mut w = w.clone();
            b.iter(|| {
                w.step().unwrap();
                black_box(&w);
            });
        });
    }
    group.finish();
}

fn generate(c: &mut Criterion) {
    c.bench_function("world_new_5000_agents", |b| {
        let config = WorldConfig {
            population: Population::exactly(1_000),
            ..WorldConfig::new(1, SimTime::EPOCH)
        };
        b.iter(|| black_box(World::new(config).unwrap()));
    });
}

fn ids(n: u64) -> Vec<(EntityId, [u64; 8])> {
    (0..n)
        .map(|i| (EntityId::procedural(ws_rng::hash(7, 7, &[i])), [i; 8]))
        .collect()
}

fn column(c: &mut Criterion) {
    let batch = ids(5_000);
    let mut full = Column::new();
    full.insert_batch(batch.clone()).unwrap();
    let probe: Vec<EntityId> = batch.iter().step_by(7).map(|(id, _)| *id).collect();
    c.bench_function("column_insert_batch_5000", |b| {
        b.iter_batched(
            || batch.clone(),
            |items| {
                let mut col = Column::new();
                col.insert_batch(items).unwrap();
                col
            },
            BatchSize::SmallInput,
        );
    });
    c.bench_function("column_insert_one_into_5000", |b| {
        let extra = EntityId::procedural(ws_rng::hash(8, 8, &[0]));
        b.iter_batched(
            || full.clone(),
            |mut col| {
                col.insert(extra, [0; 8]).unwrap();
                col
            },
            BatchSize::SmallInput,
        );
    });
    c.bench_function("column_get_5000", |b| {
        b.iter(|| probe.iter().filter_map(|id| full.get(*id)).count());
    });
    c.bench_function("column_iterate_5000", |b| {
        b.iter(|| full.iter().map(|(_, v)| v[0]).sum::<u64>());
    });
}

criterion_group!(benches, step, generate, column);
criterion_main!(benches);
