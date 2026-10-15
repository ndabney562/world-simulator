//! Tier 1 golden tests: fixed coordinates must map to the same cell IDs on
//! every platform (25-determinism-policy.md §25.2, ADR 0015).

use ws_math::{CellId, Geodetic, SECTOR_LEVEL, SurfacePos};

const GOLDEN: &str = include_str!("golden/cell_ids.txt");

fn hex(s: &str) -> CellId {
    CellId::from_raw(u64::from_str_radix(s, 16).expect("hex cell ID")).expect("valid cell ID")
}

fn lines(kind: &str) -> Vec<Vec<&'static str>> {
    GOLDEN
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}

#[test]
fn golden_cell_ids_match_exactly() {
    let rows = lines("cell");
    assert!(rows.len() >= 30, "golden file has {} cell rows", rows.len());
    let mut failures = Vec::new();
    for f in &rows {
        assert_eq!(f.len(), 9, "malformed row {f:?}");
        let g = Geodetic::from_degrees(f[1].parse().unwrap(), f[2].parse().unwrap(), 0.0).unwrap();
        let leaf = CellId::from_geodetic(&g);
        let sector = leaf.parent(SECTOR_LEVEL).unwrap();
        let got = [leaf, sector]
            .into_iter()
            .chain(sector.edge_neighbors())
            .map(|c| c.to_string())
            .collect::<Vec<_>>();
        if got != f[3..] {
            failures.push(format!(
                "({}, {}): expected {:?}, got {:?}",
                f[1],
                f[2],
                &f[3..],
                got
            ));
        }
        // Height never changes the cell.
        let high = g.with_height(8848.0).unwrap();
        assert_eq!(CellId::from_geodetic(&high), leaf);
    }
    assert!(
        failures.is_empty(),
        "Tier 1 golden mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn golden_sector_membership_through_ecef_matches() {
    // The ECEF path (Geodetic -> ECEF -> Geodetic -> cell) agrees with the
    // direct path for every golden point at ordinary heights, except the
    // two cube vertices, which sit exactly on a face boundary.
    for f in lines("cell") {
        let (lat, lon): (f64, f64) = (f[1].parse().unwrap(), f[2].parse().unwrap());
        if (lat.abs() - 35.264_389_682_754_654).abs() < 1e-12 {
            continue;
        }
        let expected = hex(f[4]);
        for h in [-30.0, 0.0, 100.0] {
            let p = Geodetic::from_degrees(lat, lon, h).unwrap().to_ecef();
            let got = SurfacePos::from_ecef(p).unwrap().cell();
            assert_eq!(got, expected, "({lat}, {lon}, h = {h})");
        }
    }
}

#[test]
fn golden_reexpression_matches_exactly() {
    let rows = lines("reexpress");
    assert!(rows.len() >= 10);
    for f in rows {
        let offset = [f[2], f[3], f[4]].map(|s| s.parse::<f32>().unwrap());
        let p = SurfacePos::new(hex(f[1]), offset).unwrap();
        let q = p.reexpress().unwrap();
        assert_eq!(q.cell(), hex(f[5]), "{f:?}");
        assert!(q.to_ecef().distance(p.to_ecef()) < 1e-4, "{f:?}");
    }
}
