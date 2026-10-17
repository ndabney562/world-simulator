//! The conservative inside test (#48): [`CellFrame::certainly_contains`]
//! may only say "inside" where the full Tier 1 lookup agrees, at any
//! distance from an edge and any height, and it must say "inside" for
//! ordinary positions well inside a cell, or movement gains nothing.
//!
//! Each sweep finds a sector cell's edge along a ray from its centre by
//! 64-bit bisection, then checks points from 1 µm to 100 m inside and
//! outside that edge at several heights.

use core::f64::consts::{FRAC_PI_2, PI, TAU};

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use ws_math::reference::BAY_RIDGE;
use ws_math::{CellFrame, CellId, Geodetic, Position, SECTOR_LEVEL, SurfacePos};

/// Heights (ENU up offsets, metres) checked at every distance: below the
/// surface, on it, buildings, aircraft, near and beyond the test's
/// 100 km cut-off.
const HEIGHTS: [f64; 9] = [
    -2_000.0, -50.0, 0.0, 30.0, 500.0, 12_000.0, 60_000.0, 99_000.0, 150_000.0,
];

/// Distances from the edge, in metres, sampled on each side.
fn distances() -> impl Iterator<Item = f64> {
    (0..=80).map(|k| 1e-6 * 10f64.powf(f64::from(k) / 10.0))
}

fn occupied(frame: &CellFrame, enu: Position<ws_math::Enu>) -> CellId {
    let p = frame.frame().to_ecef(enu);
    CellId::from_ecef(p).unwrap().parent(SECTOR_LEVEL).unwrap()
}

#[derive(Debug, Default)]
struct Counts {
    samples: u32,
    proven: u32,
}

/// Checks the ray from `cell`'s centre in direction `angle`.
fn sweep(cell: CellId, angle: f64, out: &mut Counts) {
    let frame = CellFrame::with_inside_test(cell);
    let (dx, dy) = (angle.cos(), angle.sin());
    for up in HEIGHTS {
        let at = |t: f64| Position::new(dx * t, dy * t, up);
        let (mut lo, mut hi) = (0.0_f64, 3000.0_f64);
        if occupied(&frame, at(lo)) != cell || occupied(&frame, at(hi)) == cell {
            // Far from the surface the cell's footprint shifts; skip rays
            // that do not cross exactly one edge.
            continue;
        }
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if occupied(&frame, at(mid)) == cell {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        for d in distances() {
            for t in [lo - d, hi + d] {
                let enu = at(t);
                let p = frame.frame().to_ecef(enu);
                let proven = frame.certainly_contains(p);
                if proven {
                    assert_eq!(
                        occupied(&frame, enu),
                        cell,
                        "{cell:?} angle {angle} up {up} t {t}: proven inside but outside"
                    );
                    out.proven += 1;
                }
                out.samples += 1;
            }
        }
        // Ordinary positions are proven: 50 m inside the edge, at heights
        // a walking agent or a building can have.
        if (-50.0..=500.0).contains(&up) && lo > 50.0 {
            let p = frame.frame().to_ecef(at(lo - 50.0));
            assert!(
                frame.certainly_contains(p),
                "{cell:?} angle {angle} up {up}: 50 m inside not proven"
            );
        }
    }
    // The cell centre on the surface is always proven.
    assert!(frame.certainly_contains(frame.frame().origin_ecef()));
}

fn sector_at(lat_deg: f64, lon_deg: f64) -> CellId {
    let g = Geodetic::from_degrees(lat_deg, lon_deg, 0.0).unwrap();
    CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap()
}

fn region(name: &str, cell: CellId) {
    let mut c = Counts::default();
    for k in 0..48 {
        let angle = (f64::from(k) * 2.399_963_229_728_653) % TAU;
        sweep(cell, angle, &mut c);
    }
    assert!(c.samples > 10_000, "{name}: {c:?}");
    // Half the samples are outside and most of the rest are within
    // millimetres to metres of the edge, so only a small share is proven.
    assert!(c.proven > c.samples / 20, "{name}: {c:?}");
}

#[test]
fn named_regions_are_conservative() {
    region(
        "Bay Ridge",
        CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap(),
    );
    region("equator", sector_at(0.3, 10.0));
    region("60 N", sector_at(60.0, -20.0));
    region("80 N", sector_at(80.0, 135.0));
    region("near the south pole", sector_at(-89.99, 0.0));
    region("cube vertex", sector_at(35.264_389_682_754_654, 44.999));
    region("face edge, face 0 side", sector_at(0.001, 44.999));
    region("face edge, face 1 side", sector_at(0.001, 45.001));
}

#[test]
fn far_from_the_surface_is_never_proven() {
    let frame = CellFrame::with_inside_test(sector_at(10.0, 10.0));
    for up in [100_500.0, 1e6, -100_500.0, -6e6] {
        let p = frame.frame().to_ecef(Position::new(0.0, 0.0, up));
        assert!(!frame.certainly_contains(p), "up {up}");
    }
    let nan = Position::new(f64::NAN, 0.0, 0.0);
    assert!(!frame.certainly_contains(nan));
}

#[test]
fn plain_frames_never_prove() {
    let cell = sector_at(10.0, 10.0);
    let p = CellFrame::new(cell).frame().origin_ecef();
    assert!(CellFrame::with_inside_test(cell).certainly_contains(p));
    assert!(!CellFrame::new(cell).certainly_contains(p));
}

fn config() -> Config {
    Config {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x5745_5f49_4e53_4901),
        failure_persistence: None,
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Anywhere on the globe, in any direction, at every distance and
    /// height above.
    #[test]
    fn anywhere_is_conservative(
        lat in -FRAC_PI_2..=FRAC_PI_2,
        lon in -PI..=PI,
        angle in 0.0..TAU,
    ) {
        let g = Geodetic::new(lat, lon, 0.0).unwrap();
        let cell = CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap();
        let mut c = Counts::default();
        sweep(cell, angle, &mut c);
        prop_assert!(c.samples > 0);
    }

    /// Arbitrary positions in and around a cell: proven inside implies the
    /// full lookup agrees, and the cached re-expression is still bit for
    /// bit the full one.
    #[test]
    fn random_offsets_are_conservative(
        lat in -FRAC_PI_2..=FRAC_PI_2,
        lon in -PI..=PI,
        e in -2500.0..2500.0f32,
        n in -2500.0..2500.0f32,
        u in -5_000.0..200_000.0f32,
    ) {
        let g = Geodetic::new(lat, lon, 0.0).unwrap();
        let cell = CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap();
        let frame = CellFrame::with_inside_test(cell);
        let p = SurfacePos::new(cell, [e, n, u]).unwrap();
        if frame.certainly_contains(p.to_ecef()) {
            prop_assert_eq!(p.occupied_cell(), Ok(cell));
        }
        prop_assert_eq!(p.reexpress_with(&frame), p.reexpress());
    }
}
