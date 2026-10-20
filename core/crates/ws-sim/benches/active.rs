//! Desktop-only timing; clocks never enter simulation logic.
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use ws_life::behavior::Detail;
use ws_sim::{
    SimTime,
    slice::{Operation, SliceConfig, SliceSimulation},
};
fn slice(seed: u64, start: i64) -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed,
        start: SimTime::from_micros_since_epoch(start),
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("active_slice");
    for seed in [0, 42, u64::MAX] {
        let mut w = slice(seed, 7 * 3_600_000_000 + 30 * 60_000_000);
        let ids = w.population().residents().ids().to_vec();
        for &id in &ids {
            assert!(
                w.apply(Operation::SetDetail {
                    resident: id,
                    level: Detail::L1
                })
                .unwrap()
                .error
                .is_none()
            );
        }
        let label = format!("{}-{seed}", ids.len());
        group.bench_with_input(
            BenchmarkId::new("100_ticks_and_all_fine_views", &label),
            &w,
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        for _ in 0..100 {
                            w.step().unwrap();
                            for &id in &ids {
                                black_box(w.resident(id).unwrap());
                            }
                        }
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                )
            },
        );
        let mut boundary = slice(seed, 7 * 3_600_000_000 + 31 * 60_000_000 - 100_000);
        for &id in &ids {
            boundary
                .apply(Operation::SetDetail {
                    resident: id,
                    level: Detail::L1,
                })
                .unwrap();
        }
        group.bench_with_input(
            BenchmarkId::new("minute_decision_tick", &label),
            &boundary,
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        w.step().unwrap();
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                )
            },
        );
        let coarse = slice(seed, 18 * 3_600_000_000);
        group.bench_with_input(
            BenchmarkId::new("coarse_hour_domain", &label),
            coarse.behavior(),
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        w.advance_to(SimTime::from_micros_since_epoch(19 * 3_600_000_000))
                            .unwrap();
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                )
            },
        );
        let graph = w.behavior().navigation().graph();
        group.bench_function(BenchmarkId::new("all_pairs_cold_paths", &label), |b| {
            b.iter(|| {
                for a in 0..graph.points().len() {
                    for b in 0..graph.points().len() {
                        black_box(
                            black_box(graph)
                                .shortest(black_box(a), black_box(b))
                                .unwrap(),
                        );
                    }
                }
            })
        });
    }
    group.finish();
}
criterion_group!(measurements, benchmarks);
criterion_main!(measurements);
