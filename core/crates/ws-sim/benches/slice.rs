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
    let healthy = slice(42, SimTime::EPOCH);
    let id = healthy.population().residents().ids()[0];
    let mut ill = healthy.clone();
    ill.apply(ws_sim::slice::Operation::Afflict {
        resident: id,
        severity: 5,
        duration_us: 7_200_000_000,
    })
    .unwrap();
    for (label, world) in [
        ("slice_illness_healthy_100_ticks", &healthy),
        ("slice_illness_active_100_ticks", &ill),
    ] {
        c.bench_function(label, |b| {
            b.iter_batched(
                || world.clone(),
                |mut w| {
                    w.apply(ws_sim::slice::Operation::Steps(100)).unwrap();
                    black_box(w)
                },
                BatchSize::SmallInput,
            )
        });
    }
    c.bench_function("slice_illness_apply", |b| {
        b.iter_batched(
            || healthy.clone(),
            |mut w| {
                w.apply(ws_sim::slice::Operation::Afflict {
                    resident: id,
                    severity: 5,
                    duration_us: 7_200_000_000,
                })
                .unwrap();
                black_box(w)
            },
            BatchSize::SmallInput,
        )
    });
    let mut history = healthy.clone();
    let mut root = 0;
    for _ in 0..1000 {
        history
            .apply(ws_sim::slice::Operation::Afflict {
                resident: id,
                severity: 5,
                duration_us: 100_000,
            })
            .unwrap();
        root = history.illness_state(id).unwrap().own_root;
        history.step().unwrap();
    }
    eprintln!(
        "illness history: roots=1000 events={} event_capacity_bytes={} consequence_index_capacity_bytes={}",
        history.events().records().len(),
        history.events().capacity_bytes(),
        history.consequence_index_capacity_bytes()
    );
    c.bench_function("slice_illness_query_1000_roots", |b| {
        b.iter(|| {
            black_box(
                history
                    .consequences(ws_sim::ws_events::EventId(root))
                    .unwrap(),
            )
        })
    });
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
