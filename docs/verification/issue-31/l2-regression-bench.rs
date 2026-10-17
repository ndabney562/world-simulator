//! Focused repeat of TP-007's 100,000-agent L2-hour benchmark.
//! Compile against each revision's release ws-sim and existing Criterion.
use criterion::{BatchSize, Criterion};
use std::hint::black_box;
use std::time::Duration;
use ws_sim::{Lod, Population, SimTime, World, WorldConfig};
fn main() {
    let mut world = World::new(WorldConfig {
        population: Population::exactly(20_000),
        ..WorldConfig::new(1, SimTime::EPOCH)
    })
    .unwrap();
    for _ in 0..600 {
        world.step().unwrap();
    }
    let request: Vec<_> = world
        .toy()
        .buildings()
        .ids()
        .iter()
        .map(|id| (*id, Lod::L2))
        .collect();
    world.set_lod(&request).unwrap();
    let mut c = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(20))
        .configure_from_args();
    c.bench_function("l2_hour_100000_focused", |b| {
        b.iter_batched(
            || world.clone(),
            |mut w| {
                for _ in 0..36_000 {
                    w.step().unwrap();
                }
                black_box(w)
            },
            BatchSize::LargeInput,
        );
    });
    c.final_summary();
}
