//! Cell-edge re-expression (ADR 0016 §4): every re-expressed position lies
//! in its own cell, re-expression settles instead of alternating, and the
//! old failure mode is shown to occur in these very samples, so the tests
//! cannot pass by not reaching it.
//!
//! Each sweep finds a sector cell's edge along a ray from its centre by
//! 64-bit bisection and re-expresses every distinct `f32` offset within
//! ±200 µm of it.

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use ws_math::reference::BAY_RIDGE;
use ws_math::{CellFrame, CellId, Geodetic, Position, SECTOR_LEVEL, SurfacePos};

/// Re-expression as it was before ADR 0016: plain rounding in the target
/// cell's frame. Used only to show what the new rule prevents.
fn old_reexpress(p: SurfacePos) -> SurfacePos {
    let e = p.to_ecef();
    let target = CellId::from_ecef(e).unwrap().parent(SECTOR_LEVEL).unwrap();
    if target == p.cell() {
        p
    } else {
        SurfacePos::from_ecef_in(e, &CellFrame::new(target)).unwrap()
    }
}

fn occupied_64(cell: CellId, east: f64, north: f64) -> CellId {
    let p = cell.enu_frame().to_ecef(Position::new(east, north, 0.0));
    CellId::from_ecef(p).unwrap().parent(SECTOR_LEVEL).unwrap()
}

#[derive(Debug, Default)]
struct Sweep {
    samples: u32,
    /// Samples where the old rule left the result outside its own cell.
    old_outside: u32,
    /// Samples where the old rule never settled within 20 calls.
    old_never_settles: u32,
    /// Largest position change of the new rule, in metres.
    max_change: f64,
}

/// Sweeps the edge of `cell` in direction `angle` (radians from east).
fn sweep(cell: CellId, angle: f64, step_m: f64, out: &mut Sweep) {
    let (dx, dy) = (angle.cos(), angle.sin());
    let (mut lo, mut hi) = (0.0_f64, 2000.0_f64);
    assert_ne!(
        occupied_64(cell, dx * hi, dy * hi),
        cell,
        "2 km leaves any sector cell"
    );
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if occupied_64(cell, dx * mid, dy * mid) == cell {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let steps = (200e-6 / step_m) as i64;
    let mut last = None;
    for k in -steps..=steps {
        let t = lo + k as f64 * step_m;
        let offset = [(dx * t) as f32, (dy * t) as f32, 0.0];
        if last == Some(offset) {
            continue;
        }
        last = Some(offset);
        let p = SurfacePos::new(cell, offset).unwrap();
        let q = p.reexpress().unwrap();
        // The cached path is not allowed to call an outside point inside,
        // or alter the edge-settling bits. These fixed/global sweeps cover
        // every distinct f32 sample within +/-200 micrometres of the edge.
        let cached = p.reexpress_with(&CellFrame::new(cell)).unwrap();
        assert_eq!(cached.cell(), q.cell());
        assert_eq!(
            cached.offset().map(f32::to_bits),
            q.offset().map(f32::to_bits)
        );
        if cached.cell() == cell {
            assert_eq!(p.occupied_cell(), Ok(cell));
        }
        out.samples += 1;

        // The invariant, idempotence, settling, and the position bound.
        assert_eq!(q.occupied_cell(), Ok(q.cell()), "{p:?} -> {q:?}");
        let mut r = q;
        for _ in 0..5 {
            r = r.reexpress().unwrap();
            assert_eq!(r, q, "{p:?} does not settle");
        }
        assert_eq!(
            q.cell(),
            p.occupied_cell().unwrap(),
            "lands in the containing cell"
        );
        let change = q.to_ecef().distance(p.to_ecef());
        assert!(change < 1e-4, "{p:?} moved {change} m");
        out.max_change = out.max_change.max(change);

        // Where the old rule was already self-consistent, nothing changed.
        let old = old_reexpress(p);
        if old.occupied_cell() == Ok(old.cell()) {
            assert_eq!(q, old, "{p:?}");
        } else {
            out.old_outside += 1;
            let mut s = old;
            let mut settled = false;
            for _ in 0..20 {
                let next = old_reexpress(s);
                if next == s {
                    settled = true;
                    break;
                }
                s = next;
            }
            if !settled {
                out.old_never_settles += 1;
            }
        }
    }
}

fn sector_at(lat_deg: f64, lon_deg: f64) -> CellId {
    let g = Geodetic::from_degrees(lat_deg, lon_deg, 0.0).unwrap();
    CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap()
}

/// Deterministic sweeps of 96 directions around one cell.
fn region(name: &str, cell: CellId) {
    let mut s = Sweep::default();
    for k in 0..96 {
        // Golden-angle spacing gives directions that are not aligned with
        // the cell's axes.
        let angle = (f64::from(k) * 2.399_963_229_728_653) % TAU;
        sweep(cell, angle, 1e-6, &mut s);
    }
    assert!(s.samples > 1000, "{name}: {s:?}");
    // The old failure mode occurs in these samples, so this region's checks
    // above really exercise the edge rule.
    assert!(s.old_outside > 0, "{name}: {s:?}");
    // Including inputs that alternated forever under the old rule.
    assert!(s.old_never_settles > 0, "{name}: {s:?}");
}

#[test]
fn bay_ridge_edges_are_self_consistent() {
    let cell = CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap();
    region("Bay Ridge", cell);
}

#[test]
fn equator_edges_are_self_consistent() {
    region("equator", sector_at(0.3, 10.0));
}

#[test]
fn high_latitude_edges_are_self_consistent() {
    region("60 N", sector_at(60.0, -20.0));
    region("80 N", sector_at(80.0, 135.0));
}

#[test]
fn cube_face_and_vertex_edges_are_self_consistent() {
    // A cell touching a cube vertex, and cells on either side of the face
    // boundary at the equator (longitude 45°).
    region("cube vertex", sector_at(35.264_389_682_754_654, 44.999));
    region("face edge, face 0 side", sector_at(0.001, 44.999));
    region("face edge, face 1 side", sector_at(0.001, 45.001));
}

fn config() -> Config {
    Config {
        cases: 96,
        rng_seed: RngSeed::Fixed(0x5745_5f45_4447_4501),
        failure_persistence: None,
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Anywhere on the globe, in any direction: the invariant, settling, and
    /// the 0.1 mm bound hold across the edge band.
    #[test]
    fn edges_anywhere_are_self_consistent(
        lat in -FRAC_PI_2..=FRAC_PI_2,
        lon in -PI..=PI,
        angle in 0.0..TAU,
    ) {
        let g = Geodetic::new(lat, lon, 0.0).unwrap();
        let cell = CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap();
        let mut s = Sweep::default();
        sweep(cell, angle, 2e-6, &mut s);
        prop_assert!(s.samples > 0);
    }
}
