//! Desktop CPU headroom only; actual Editor/native graphics evidence is separate.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use ws_life::{lod::CameraInterest, navigation::Point};
use ws_sim::{
    SimTime, TimeScale, UtcDateTime,
    slice::{Operation, SliceConfig, SliceSimulation},
};
fn bench(c: &mut Criterion) {
    let start = SimTime::from_utc(UtcDateTime::new(2026, 6, 21, 17, 0, 0, 0).unwrap()).unwrap();
    let mut slice =
        SliceSimulation::new(SliceConfig::calendar_aligned_v1(71449, start).unwrap()).unwrap();
    assert_eq!(slice.population().residents().len(), 30);
    for op in [
        Operation::CameraInterest(CameraInterest {
            center: Point { east: 0, north: 0 },
            radius_mm: 1_000_000_000,
        }),
        Operation::Steps(10),
        Operation::SetScale(TimeScale::X100),
        Operation::HostElapsed(16667),
    ] {
        assert!(slice.apply(op).unwrap().error.is_none());
    }
    // At100×, accepted ADR0029 policy caps detail at L2. This is the actual
    // real-slice core pacing/domain workload, with calendar/hash projection,
    // not an assertion that#72's real-slice native renderer already exists.
    let basis = ws_world::environment::site_frame();
    c.bench_function(
        "calendar/real_slice30_100x_host_frame_with_calendar_and_hash",
        |b| {
            b.iter_batched(
                || slice.clone(),
                |mut w| {
                    let report = w.apply(Operation::HostElapsed(16667)).unwrap();
                    assert!(report.error.is_none());
                    assert!((16..=17).contains(&report.committed));
                    let sky = ws_world::environment::calendar_sky(w.clock().now(), Some(basis));
                    black_box((w.state_hash(), sky, w))
                },
                BatchSize::SmallInput,
            )
        },
    );
}
criterion_group!(benches, bench);
criterion_main!(benches);
