//! ADR0029 whole-slice transitions and bulk view cost; desktop evidence only.
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use ws_life::{lod::CameraInterest, navigation::Point};
use ws_sim::{
    SimTime,
    slice::{Operation, SliceConfig, SliceSimulation},
};
fn interest(radius_mm: u64) -> CameraInterest {
    CameraInterest {
        center: Point { east: 0, north: 0 },
        radius_mm,
    }
}
fn world(seed: u64) -> SliceSimulation {
    SliceSimulation::new(SliceConfig {
        seed,
        start: SimTime::EPOCH,
        schedule_epoch: SimTime::EPOCH,
    })
    .unwrap()
}
fn step(world: &mut SliceSimulation, n: u64) {
    let report = world.apply(Operation::Steps(n)).unwrap();
    assert_eq!(report.committed, n);
    assert!(report.error.is_none());
}
fn camera(world: &mut SliceSimulation, r: u64) {
    assert!(
        world
            .apply(Operation::CameraInterest(interest(r)))
            .unwrap()
            .error
            .is_none()
    );
}
fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("slice_camera");
    for seed in [0, 42, u64::MAX, 71_449] {
        // First eligible policy tick is1.0s. Start at0.9s so exactly one measured
        // physical tick promotes every node/resident from L2 to L1.
        let mut promote = world(seed);
        camera(&mut promote, 1_000_000_000);
        step(&mut promote, 9);
        let cohort = promote.population().residents().len();
        let label = format!("{cohort}-{seed}");
        group.bench_with_input(
            BenchmarkId::new("all_node_promote_tick", &label),
            &promote,
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        step(&mut w, 1);
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        let mut demote = promote.clone();
        step(&mut demote, 1);
        camera(&mut demote, 0);
        step(&mut demote, 1);
        //1.2s: forced overview demotion includes bounded exact-observation planning.
        group.bench_with_input(
            BenchmarkId::new("all_node_demote_tick_and_cache", &label),
            &demote,
            |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        step(&mut w, 1);
                        black_box(w)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        let mut fine = promote.clone();
        step(&mut fine, 1);
        group.bench_with_input(BenchmarkId::new("bulk_fine_view", &label), &fine, |b, w| {
            b.iter(|| black_box(w.view().unwrap()));
        });
    }
    group.finish();
}
criterion_group!(benches, bench);
criterion_main!(benches);
