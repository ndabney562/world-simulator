//! Dedicated ADR0030 direction conversion is Tier1; general coordinates stay Tier3.
use ws_time::{SimTime, UtcDateTime};
use ws_world::environment::{SOLAR_VERSION, calendar_sky, site_frame};

/// The sun direction in the slice's render basis (the site frame) has exact
/// rule-1 bits. The toy-world basis rows were deleted with the toy (ADR 0040).
#[test]
fn query_direction_in_site_render_basis_has_exact_rule_one_bits() {
    assert_eq!(SOLAR_VERSION, 1);
    let rows: Vec<_> = include_str!("golden/solar_site_v1.txt")
        .lines()
        .filter(|r| !r.starts_with('#') && !r.is_empty())
        .collect();
    assert_eq!(rows.len(), 3);
    for row in rows {
        let p: Vec<_> = row.split_whitespace().collect();
        assert_eq!(p.len(), 7);
        let utc = UtcDateTime::new(
            p[0].parse().unwrap(),
            p[1].parse().unwrap(),
            p[2].parse().unwrap(),
            p[3].parse().unwrap(),
            0,
            0,
            0,
        )
        .unwrap();
        let s = calendar_sky(SimTime::from_utc(utc).unwrap(), Some(site_frame()))
            .supported
            .unwrap();
        let actual = [s.toward_sun.x, s.toward_sun.y, s.toward_sun.z].map(f64::to_bits);
        let expected: Vec<_> = p[4..]
            .iter()
            .map(|x| u64::from_str_radix(x, 16).unwrap())
            .collect();
        assert_eq!(actual.as_slice(), expected, "{row}");
    }
}
