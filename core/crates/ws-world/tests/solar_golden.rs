//! Dedicated ADR0030 direction conversion is Tier1; general coordinates stay Tier3.
use ws_time::{SimTime, UtcDateTime};
use ws_world::{
    Dataset, Population, ToyWorld, WalkParams,
    environment::{SOLAR_VERSION, calendar_sky},
};
#[test]
fn query_direction_in_actual_render_basis_has_exact_rule_one_bits() {
    assert_eq!(SOLAR_VERSION, 1);
    for row in include_str!("golden/solar_frame_v1.txt")
        .lines()
        .filter(|r| !r.starts_with('#') && !r.is_empty())
    {
        let p: Vec<_> = row.split_whitespace().collect();
        assert_eq!(p.len(), 8);
        let seed = p[0].parse().unwrap();
        let utc = UtcDateTime::new(
            p[1].parse().unwrap(),
            p[2].parse().unwrap(),
            p[3].parse().unwrap(),
            p[4].parse().unwrap(),
            0,
            0,
            0,
        )
        .unwrap();
        let world = ToyWorld::generate(
            seed,
            &Dataset::toy().unwrap(),
            Population::TOY,
            WalkParams::DEFAULT,
        )
        .unwrap();
        let s = calendar_sky(SimTime::from_utc(utc).unwrap(), world.render_basis())
            .supported
            .unwrap();
        let actual = [s.toward_sun.x, s.toward_sun.y, s.toward_sun.z].map(f64::to_bits);
        let expected: Vec<_> = p[5..]
            .iter()
            .map(|x| u64::from_str_radix(x, 16).unwrap())
            .collect();
        assert_eq!(actual.as_slice(), expected, "{row}");
    }
}
