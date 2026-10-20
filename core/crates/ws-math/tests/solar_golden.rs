//! New rule1 Tier1 goldens; no prior coordinate/identity golden is replaced.
use ws_math::solar::{SOLAR_VERSION, position};
#[test]
fn solar_rule_one_exact_bits_on_every_platform() {
    assert_eq!(SOLAR_VERSION, 1);
    for row in include_str!("golden/solar_v1.txt")
        .lines()
        .filter(|r| !r.starts_with('#') && !r.is_empty())
    {
        let p: Vec<_> = row.split_whitespace().collect();
        assert_eq!(p.len(), 7);
        let s = position(
            p[0].parse().unwrap(),
            p[1].parse().unwrap(),
            40.622079256973,
            -74.030160593242,
        )
        .unwrap();
        let actual = [
            s.azimuth_deg,
            s.elevation_deg,
            s.toward_sun.x,
            s.toward_sun.y,
            s.toward_sun.z,
        ]
        .map(f64::to_bits);
        let expected: Vec<_> = p[2..]
            .iter()
            .map(|x| u64::from_str_radix(x, 16).unwrap())
            .collect();
        assert_eq!(actual.as_slice(), expected, "{row}");
    }
}
