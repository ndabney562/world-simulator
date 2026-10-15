//! Property tests (proptest) for conversions, round trips, and cell
//! invariants. The RNG seed is fixed, so every run checks the same cases
//! (docs/process/testing-policy.md §4).

use core::f64::consts::{FRAC_PI_2, PI};

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use ws_math::cell::MAX_LEVEL;
use ws_math::{CellFrame, CellId, Enu, EnuFrame, Geodetic, Position, SECTOR_LEVEL, SurfacePos};

fn config() -> Config {
    Config {
        cases: 2048,
        rng_seed: RngSeed::Fixed(0x5745_5f4d_4154_4801),
        failure_persistence: None,
        ..Config::default()
    }
}

fn geodetic(h_min: f64, h_max: f64) -> impl Strategy<Value = Geodetic> {
    (-FRAC_PI_2..=FRAC_PI_2, -PI..=PI, h_min..=h_max)
        .prop_map(|(lat, lon, h)| Geodetic::new(lat, lon, h).unwrap())
}

/// Latitudes in a band, plus exact poles and the equator.
fn any_lat_lon() -> impl Strategy<Value = Geodetic> {
    prop_oneof![
        8 => geodetic(0.0, 0.0),
        1 => (prop_oneof![Just(FRAC_PI_2), Just(-FRAC_PI_2), Just(0.0)], -PI..=PI)
            .prop_map(|(lat, lon)| Geodetic::new(lat, lon, 0.0).unwrap()),
    ]
}

proptest! {
    #![proptest_config(config())]

    /// Acceptance criterion: geodetic → ECEF → geodetic error < 1 mm over
    /// the globe, measured as the ECEF distance between the original and
    /// the round-tripped position (heights −100 km to +1000 km).
    #[test]
    fn geodetic_ecef_round_trip_under_1_mm(g in geodetic(-100_000.0, 1_000_000.0)) {
        let p = g.to_ecef();
        let back = Geodetic::from_ecef(p).unwrap();
        let err = back.to_ecef().distance(p);
        prop_assert!(err < 1e-3, "{g:?} -> {back:?}: {err} m");
        prop_assert!((back.h() - g.h()).abs() < 1e-3);
        prop_assert!((back.lat() - g.lat()).abs() < 1e-3 / 6.3e6);
    }

    #[test]
    fn geodetic_ecef_round_trip_at_poles(lon in -PI..=PI, h in -10_000.0..=100_000.0f64, north in any::<bool>()) {
        let lat = if north { FRAC_PI_2 } else { -FRAC_PI_2 };
        let p = Geodetic::new(lat, lon, h).unwrap().to_ecef();
        let back = Geodetic::from_ecef(p).unwrap();
        prop_assert!(back.to_ecef().distance(p) < 1e-3);
        prop_assert!((back.h() - h).abs() < 1e-3);
    }

    #[test]
    fn enu_round_trip(origin in geodetic(-500.0, 3000.0), e in -5000.0..5000.0f64, n in -5000.0..5000.0f64, u in -500.0..500.0f64) {
        let f = EnuFrame::new(origin);
        let local = Position::<Enu>::new(e, n, u);
        let back = f.to_enu(f.to_ecef(local));
        prop_assert!(back.distance(local) < 1e-6);
        // Rotations preserve length.
        let d = f.to_ecef(local) - f.origin_ecef();
        prop_assert!((d.length() - local.to_vector().length()).abs() < 1e-6);
    }

    /// Acceptance criterion: SurfacePos → ECEF → SurfacePos error < 0.1 mm
    /// within a level-13 cell.
    #[test]
    fn surface_pos_round_trip_under_0_1_mm(g in geodetic(-500.0, 1000.0)) {
        let p = g.to_ecef();
        let s = SurfacePos::from_ecef(p).unwrap();
        prop_assert_eq!(s.cell().level(), SECTOR_LEVEL);
        let q = s.to_ecef();
        prop_assert!(q.distance(p) < 1e-4, "ECEF -> SurfacePos -> ECEF error {} m", q.distance(p));
        let s2 = SurfacePos::from_ecef_in(q, &CellFrame::new(s.cell())).unwrap();
        let d = s2.offset_enu().distance(s.offset_enu());
        prop_assert!(d < 1e-4, "SurfacePos -> ECEF -> SurfacePos error {d} m");
    }

    #[test]
    fn surface_pos_offsets_stay_within_a_sector_cell(g in geodetic(0.0, 0.0)) {
        let s = SurfacePos::from_ecef(g.to_ecef()).unwrap();
        let [e, n, _] = s.offset();
        // Level-13 cells are at most about 1.3 km across.
        prop_assert!(e.abs() < 1000.0 && n.abs() < 1000.0, "{s:?}");
    }

    #[test]
    fn cell_hierarchy_invariants(g in any_lat_lon(), level in 0..=MAX_LEVEL) {
        let leaf = CellId::from_geodetic(&g);
        prop_assert!(leaf.is_leaf());
        let c = leaf.parent(level).unwrap();
        prop_assert_eq!(c.level(), level);
        prop_assert!(c.contains(leaf));
        prop_assert_eq!(CellId::from_raw(c.raw()), Ok(c));
        if level > 0 {
            let parent = c.parent(level - 1).unwrap();
            prop_assert!(parent.contains(c) && !c.contains(parent));
            let kids = parent.children().unwrap();
            prop_assert!(kids.contains(&c));
        }
        if let Some(kids) = c.children() {
            prop_assert_eq!(kids.iter().filter(|k| k.contains(leaf)).count(), 1);
            for w in kids.windows(2) {
                prop_assert!(w[0] < w[1], "Hilbert order");
            }
        }
        // The centre lies in the cell.
        let centre = CellId::from_geodetic(&c.center_geodetic());
        prop_assert!(c.contains(centre), "centre of {c:?}");
    }

    #[test]
    fn edge_neighbour_invariants(g in any_lat_lon(), level in 0..=MAX_LEVEL) {
        let c = CellId::from_geodetic(&g).parent(level).unwrap();
        let ns = c.edge_neighbors();
        for (k, n) in ns.iter().enumerate() {
            prop_assert_eq!(n.level(), level);
            prop_assert_ne!(*n, c);
            prop_assert!(!n.contains(c) && !c.contains(*n));
            // Symmetric: c is a neighbour of each of its neighbours.
            prop_assert!(n.edge_neighbors().contains(&c), "{c:?} -> {n:?}");
            for m in &ns[k + 1..] {
                prop_assert_ne!(n, m);
            }
        }
    }

    #[test]
    fn reexpression_is_deterministic_and_preserves_position(
        g in geodetic(0.0, 0.0),
        e in -2500.0..2500.0f32,
        n in -2500.0..2500.0f32,
        u in -50.0..50.0f32,
    ) {
        let sector = CellId::from_geodetic_at_level(&g, SECTOR_LEVEL).unwrap();
        let p = SurfacePos::new(sector, [e, n, u]).unwrap();
        let q = p.reexpress().unwrap();
        prop_assert_eq!(q, p.reexpress().unwrap(), "same input, same output");
        prop_assert_eq!(q.cell(), p.occupied_cell().unwrap());
        prop_assert!(q.to_ecef().distance(p.to_ecef()) < 1e-4);
        if q.cell() == sector {
            prop_assert_eq!(q, p, "inside the cell, re-expression is the identity");
        }
    }
}
