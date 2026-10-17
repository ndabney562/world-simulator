//! Simulation LOD costs (ADR 0017 §15, #8): demotion and promotion of 30,
//! 3,000, and 100,000 agents; one base step of all-L1 and all-L2 worlds;
//! and one simulated hour of an all-L2 world, from which the cost per
//! background event follows. Run with `cargo bench -p ws-sim --bench lod`.
//! Results are printed, not asserted: timings depend on the machine. The
//! benchmark measures code and never feeds simulation state.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use ws_sim::{EntityId, Lod, Population, SimTime, World, WorldConfig};

/// Agents in the five toy buildings: 30, 3,000, and 100,000.
const SIZES: [u32; 3] = [6, 600, 20_000];

/// A world with `per_building` residents in each of the five toy buildings,
/// run for 600 ticks (one minute) so agents are a mix of walking and
/// waiting, at `level`.
fn warmed_world(per_building: u32, level: Lod) -> World {
    let config = WorldConfig {
        population: Population::exactly(per_building),
        ..WorldConfig::new(1, SimTime::EPOCH)
    };
    let mut w = World::new(config).unwrap();
    for _ in 0..600 {
        w.step().unwrap();
    }
    let req = all(&w, level);
    apply_lod(&mut w, &req);
    w
}

fn all(w: &World, level: Lod) -> Vec<(EntityId, Lod)> {
    w.toy()
        .buildings()
        .ids()
        .iter()
        .map(|id| (*id, level))
        .collect()
}

fn transitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("lod_transition");
    group.sample_size(20);
    for per_building in SIZES {
        let agents = per_building * 5;
        for (name, from, to) in [("demote", Lod::L1, Lod::L2), ("promote", Lod::L2, Lod::L1)] {
            let world = warmed_world(per_building, from);
            let req = all(&world, to);
            group.bench_with_input(BenchmarkId::new(name, agents), &world, |b, w| {
                b.iter_batched(
                    || w.clone(),
                    |mut w| {
                        black_box(apply_lod(&mut w, &req));
                        w
                    },
                    BatchSize::LargeInput,
                );
            });
        }
    }
    group.finish();
}

fn steps(c: &mut Criterion) {
    let mut group = c.benchmark_group("lod_step");
    group.sample_size(20);
    for per_building in [600, 20_000] {
        let agents = per_building * 5;
        for (name, level) in [("l1", Lod::L1), ("l2", Lod::L2)] {
            let world = warmed_world(per_building, level);
            group.bench_with_input(BenchmarkId::new(name, agents), &world, |b, w| {
                let mut w = w.clone();
                b.iter(|| {
                    w.step().unwrap();
                    black_box(&w);
                });
            });
        }
    }
    group.finish();
}

/// Departures plus arrivals processed by an all-L2 world over `ticks`.
fn events(w: &World, ticks: u32) -> u64 {
    let drawn = |w: &World| -> u64 {
        w.toy()
            .background()
            .iter()
            .map(|(_, bg)| u64::from(bg.targets_drawn))
            .sum()
    };
    let walking = |w: &World| -> u64 {
        w.toy()
            .background()
            .iter()
            .filter(|(_, bg)| matches!(bg.activity, ws_world::Activity::Walking { .. }))
            .count() as u64
    };
    let mut v = w.clone();
    let (d0, k0) = (drawn(&v), walking(&v));
    for _ in 0..ticks {
        v.step().unwrap();
    }
    let departures = drawn(&v) - d0;
    // Legs walking at the start and legs begun by a departure each end in
    // an arrival, unless still walking at the end.
    let arrivals = k0 + departures - walking(&v);
    departures + arrivals
}

fn hour(c: &mut Criterion) {
    const HOUR: u32 = 36_000;
    let mut group = c.benchmark_group("lod_l2_hour");
    group.sample_size(10);
    for per_building in [600, 20_000] {
        let agents = per_building * 5;
        let world = warmed_world(per_building, Lod::L2);
        let n = events(&world, HOUR);
        println!(
            "lod_l2_hour/{agents}: {n} events in one simulated hour ({:.1} per agent)",
            n as f64 / f64::from(agents)
        );
        group.bench_with_input(BenchmarkId::from_parameter(agents), &world, |b, w| {
            b.iter_batched(
                || w.clone(),
                |mut w| {
                    for _ in 0..HOUR {
                        w.step().unwrap();
                    }
                    w
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, transitions, steps, hour);
criterion_main!(benches);

fn apply_lod(
    w: &mut ws_sim::World,
    request: &[(ws_sim::EntityId, ws_sim::Lod)],
) -> ws_sim::TransitionReport {
    use ws_sim::commands::{Action, Command, IssuedBy};
    w.submit(vec![Command::new(
        IssuedBy::Test,
        Action::SetNodeLod(request.to_vec()),
    )])
    .unwrap();
    w.flush_commands().unwrap();
    assert!(w.journal().last().unwrap().result.is_ok());
    let o = w.journal().last().unwrap();
    ws_sim::TransitionReport {
        demoted: o.demoted as usize,
        promoted: o.promoted as usize,
    }
}
