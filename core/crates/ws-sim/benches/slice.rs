//! Desktop-only slice step/planning measurements; timing never enters simulation.
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use ws_sim::slice::{SliceConfig, SliceSimulation};
use ws_sim::{Population, SimTime, World, WorldConfig};
fn slice(seed: u64, start: SimTime) -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed,
        start,
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("slice_step");
    for seed in [0, 42, u64::MAX] {
        let quiet = slice(seed, SimTime::EPOCH);
        let count = quiet.population().residents().len();
        group.bench_with_input(
            BenchmarkId::new("quiet_1000", format!("{count}-{seed}")),
            &quiet,
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        for _ in 0..1000 {
                            w.step().unwrap();
                        }
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        let next = quiet.schedules().next_change(SimTime::EPOCH).unwrap();
        let boundary = slice(
            seed,
            SimTime::from_micros_since_epoch(next.as_micros_since_epoch() - 100_000),
        );
        group.bench_with_input(
            BenchmarkId::new("transition", format!("{count}-{seed}")),
            &boundary,
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        w.step().unwrap();
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("plan_day", format!("{count}-{seed}")),
            &quiet,
            |b, w| {
                b.iter(|| {
                    let mut now = SimTime::EPOCH;
                    while let Some(next) = w.schedules().next_change(now) {
                        if next.as_micros_since_epoch() >= 86_400_000_000 {
                            break;
                        }
                        black_box(w.schedules().facts_at(next).unwrap());
                        now = next;
                    }
                });
            },
        );
    }
    group.finish();
    let mut toy = World::new(WorldConfig {
        population: Population::exactly(1000),
        ..WorldConfig::new(42, SimTime::EPOCH)
    })
    .unwrap();
    for _ in 0..600 {
        toy.step().unwrap();
    }
    c.bench_function("slice_reference_toy_5000_100_steps", |b| {
        b.iter_batched(
            || toy.clone(),
            |mut w| {
                for _ in 0..100 {
                    w.step().unwrap();
                }
                black_box(w)
            },
            BatchSize::SmallInput,
        )
    });
}
criterion_group!(measurements, benches);
criterion_main!(measurements);
