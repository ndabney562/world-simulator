//! Micro-benchmark: 10,000 surface-position conversions (TP-005).
//!
//! Run with `cargo bench -p ws-math`. It measures the per-instance paths
//! used when building snapshots: `SurfacePos` → ECEF with a prebuilt
//! `CellFrame` (the hot path), without one, and ECEF → `SurfacePos`.
//! It also compares re-expression with and without the inside fast path
//! (#48) and building frames with and without it. None of these
//! allocate. Results are printed, not asserted: timings
//! depend on the machine.

// A benchmark is the one place in this crate that reads the wall clock; it
// measures the code and never feeds simulation state.
#![allow(clippy::disallowed_methods)]

use std::hint::black_box;
use std::time::Instant;

use ws_math::reference::BAY_RIDGE;
use ws_math::{CellFrame, CellId, Ecef, Position, SECTOR_LEVEL, SurfacePos};

const N: usize = 10_000;
const ROUNDS: usize = 50;

fn positions() -> Vec<SurfacePos> {
    let cell = CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap();
    (0..N)
        .map(|i| {
            let k = i as f32;
            SurfacePos::new(
                cell,
                [
                    (k * 0.37) % 500.0 - 250.0,
                    (k * 0.71) % 500.0 - 250.0,
                    k % 20.0,
                ],
            )
            .unwrap()
        })
        .collect()
}

/// Best-of-`ROUNDS` time for one pass, in microseconds.
fn time(mut pass: impl FnMut()) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        pass();
        best = best.min(start.elapsed().as_secs_f64() * 1e6);
    }
    best
}

fn report(name: &str, micros: f64) {
    println!(
        "{name:<44} {micros:>9.1} us per {N} ({:.1} ns each)",
        micros * 1000.0 / N as f64
    );
}

fn main() {
    let input = positions();
    let frame = CellFrame::new(input[0].cell());
    let mut out: Vec<Position<Ecef>> = vec![Position::origin(); N];

    report(
        "SurfacePos -> ECEF (prebuilt CellFrame)",
        time(|| {
            for (o, s) in out.iter_mut().zip(&input) {
                *o = black_box(s).to_ecef_with(&frame).unwrap();
            }
        }),
    );
    report(
        "SurfacePos -> ECEF (frame built per call)",
        time(|| {
            for (o, s) in out.iter_mut().zip(&input) {
                *o = black_box(s).to_ecef();
            }
        }),
    );
    let ecef = out.clone();
    let mut back = input.clone();
    report(
        "ECEF -> SurfacePos (prebuilt CellFrame)",
        time(|| {
            for (b, p) in back.iter_mut().zip(&ecef) {
                *b = SurfacePos::from_ecef_in(*black_box(p), &frame).unwrap();
            }
        }),
    );
    report(
        "ECEF -> SurfacePos (cell lookup + frame)",
        time(|| {
            for (b, p) in back.iter_mut().zip(&ecef) {
                *b = SurfacePos::from_ecef(*black_box(p)).unwrap();
            }
        }),
    );
    report(
        "reexpress (frame built per call)",
        time(|| {
            for (b, p) in back.iter_mut().zip(&input) {
                *b = black_box(p).reexpress().unwrap();
            }
        }),
    );
    report(
        "reexpress (prebuilt CellFrame)",
        time(|| {
            for (b, p) in back.iter_mut().zip(&input) {
                *b = black_box(p).reexpress_with(&frame).unwrap();
            }
        }),
    );
    let fast = CellFrame::with_inside_test(input[0].cell());
    report(
        "reexpress (prebuilt CellFrame, inside test)",
        time(|| {
            for (b, p) in back.iter_mut().zip(&input) {
                *b = black_box(p).reexpress_with(&fast).unwrap();
            }
        }),
    );
    let cells: Vec<CellId> = (0..N)
        .map(|i| {
            let g = ws_math::Geodetic::from_degrees(40.0 + i as f64 * 1e-4, -74.0, 0.0).unwrap();
            CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap()
        })
        .collect();
    let mut frames = vec![frame; N];
    report(
        "CellFrame::new",
        time(|| {
            for (f, c) in frames.iter_mut().zip(&cells) {
                *f = CellFrame::new(*black_box(c));
            }
        }),
    );
    let mut fast_frames = vec![fast; N];
    report(
        "CellFrame::with_inside_test",
        time(|| {
            for (f, c) in fast_frames.iter_mut().zip(&cells) {
                *f = CellFrame::with_inside_test(*black_box(c));
            }
        }),
    );
    black_box((&out, &back, &frames, &fast_frames));
}
