//! Pure NOAA/Meeus geometric solar model (ADR0030), Tier1 rule version1.
//! Inputs are civil day/label components, never uniform SimTime interpreted as
//! UTC days. Every transcendental is pinned libm; no platform trig or FMA.
use crate::{Enu, Vector};
use core::f64::consts::PI;

/// Version of constants, approximations and evaluation order used below.
pub const SOLAR_VERSION: u32 = 1;

/// Geometric sun at the input geodetic site, without atmospheric refraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolarPosition {
    /// Clockwise from true north, degrees in[0,360).
    pub azimuth_deg: f64,
    /// Geometric degrees above the ellipsoidal horizon, in[-90,90].
    pub elevation_deg: f64,
    /// Unit vector pointing toward the sun in the site's geodetic ENU basis.
    pub toward_sun: Vector<Enu>,
}

/// Unsupported civil day/label component, or nonfinite/out-of-range site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSolarInput;
impl core::fmt::Display for InvalidSolarInput {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("solar input requires a UTC2000–2099 civil day, valid nominal day label and finite geodetic site")
    }
}
impl std::error::Error for InvalidSolarInput {}

fn radians(degrees: f64) -> f64 {
    PI * degrees / 180.0
}
fn degrees(radians: f64) -> f64 {
    180.0 * radians / PI
}
fn wrap(value: f64, period: f64) -> f64 {
    let wrapped = value - period * libm::floor(value / period);
    // A negative angle smaller than half an ULP at `period` can round
    // the addition up to exactly period. Keep the promised half-open range.
    if wrapped >= period { 0.0 } else { wrapped }
}

/// Solar model1 with nominal UTC approximating rotational time, not TT/TDB.
/// `day` is the exact integer Gregorian day since2000-01-01; `micros_of_day`
/// retains UTC second60 and may reach86,400,999,999. Only the caller with a
/// validated UTC label decides whether that leap label is legal. Latitude and
/// east longitude are degrees. No site height, refraction or terrain model.
/// # Errors
/// [`InvalidSolarInput`] outside the bounded model or for invalid site inputs.
pub fn position(
    day: i64,
    micros_of_day: u64,
    latitude_deg: f64,
    longitude_deg: f64,
) -> Result<SolarPosition, InvalidSolarInput> {
    if !(0..36525).contains(&day)
        || micros_of_day >= 86_401_000_000
        || !latitude_deg.is_finite()
        || !longitude_deg.is_finite()
        || !(-90.0..=90.0).contains(&latitude_deg)
        || !(-180.0..=180.0).contains(&longitude_deg)
    {
        return Err(InvalidSolarInput);
    }
    // Julian day at2000-01-01 UTC midnight is2451544.5. This deliberately
    // uses the civil label, not uniform elapsed seconds since J2000 TT.
    // Normalize the nominal leap label with integer arithmetic first, so
    // second60 and the next midnight have exactly the same model phase.
    let phase_day = day + (micros_of_day / 86_400_000_000) as i64;
    let phase_micros = micros_of_day % 86_400_000_000;
    let julian_day = 2_451_544.5 + phase_day as f64 + phase_micros as f64 / 86_400_000_000.0;
    let t = (julian_day - 2_451_545.0) / 36_525.0;
    let mean_long = wrap(280.46646 + t * (36000.76983 + t * 0.0003032), 360.0);
    let anomaly = radians(357.52911 + t * (35999.05029 - 0.0001537 * t));
    let eccentricity = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
    let center = libm::sin(anomaly) * (1.914602 - t * (0.004817 + 0.000014 * t))
        + libm::sin(anomaly + anomaly) * (0.019993 - 0.000101 * t)
        + libm::sin(anomaly + anomaly + anomaly) * 0.000289;
    let omega = radians(125.04 - 1934.136 * t);
    let apparent_long = radians(mean_long + center - 0.00569 - 0.00478 * libm::sin(omega));
    let obliquity_seconds = 21.448 - t * (46.8150 + t * (0.00059 - t * 0.001813));
    let obliquity =
        radians(23.0 + (26.0 + obliquity_seconds / 60.0) / 60.0 + 0.00256 * libm::cos(omega));
    let declination = libm::asin(libm::sin(obliquity) * libm::sin(apparent_long));
    let tangent = libm::tan(obliquity / 2.0);
    let y = tangent * tangent;
    let l0 = radians(mean_long);
    let equation = y * libm::sin(2.0 * l0) - 2.0 * eccentricity * libm::sin(anomaly)
        + 4.0 * eccentricity * y * libm::sin(anomaly) * libm::cos(2.0 * l0)
        - 0.5 * y * y * libm::sin(4.0 * l0)
        - 1.25 * eccentricity * eccentricity * libm::sin(2.0 * anomaly);
    let equation_minutes = degrees(equation) * 4.0;
    let solar_minutes = wrap(
        phase_micros as f64 / 60_000_000.0 + equation_minutes + 4.0 * longitude_deg,
        1440.0,
    );
    let hour_angle = radians(solar_minutes / 4.0 - 180.0);
    let latitude = radians(latitude_deg);
    let sin_dec = libm::sin(declination);
    let cos_dec = libm::cos(declination);
    let sin_lat = libm::sin(latitude);
    let cos_lat = libm::cos(latitude);
    let cos_hour = libm::cos(hour_angle);
    let east = -cos_dec * libm::sin(hour_angle);
    let north = cos_lat * sin_dec - sin_lat * cos_dec * cos_hour;
    let up = sin_lat * sin_dec + cos_lat * cos_dec * cos_hour;
    Ok(SolarPosition {
        azimuth_deg: wrap(degrees(libm::atan2(east, north)), 360.0),
        elevation_deg: degrees(libm::asin(up.clamp(-1.0, 1.0))),
        toward_sun: Vector::new(east, north, up),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const LAT: f64 = 40.622079256973;
    const LON: f64 = -74.030160593242;
    #[test]
    fn circular_wrap_keeps_rounding_edges_in_half_open_range() {
        assert_eq!(wrap(-f64::EPSILON, 360.0), 0.0);
        assert_eq!(wrap(-f64::EPSILON, 1440.0), 0.0);
        for latitude in [-90.0, 0.0, 90.0] {
            for longitude in [-180.0, 0.0, 180.0] {
                let s = position(0, 0, latitude, longitude).unwrap();
                assert!((0.0..360.0).contains(&s.azimuth_deg));
                assert!((s.toward_sun.length() - 1.0).abs() < 2e-15);
            }
        }
    }
    #[test]
    fn published_noaa_geometric_reference_at_three_block_instants() {
        // Independent published main.js execution, with its refraction
        // correction explicitly disabled; source/input/method in verification docs.
        for (day, hour, az, el) in [
            (9575, 12, 99.32855895324268, 10.63581004236454),
            (9668, 17, 181.5600314275837, 72.81041186202198),
            (9851, 2, 281.7577476901047, -49.8282297457456),
        ] {
            let s = position(day, hour * 3600 * 1_000_000, LAT, LON).unwrap();
            let circular = wrap(s.azimuth_deg - az + 180.0, 360.0) - 180.0;
            assert!(circular.abs() <= 0.1, "{day}: {s:?}");
            assert!((s.elevation_deg - el).abs() <= 0.1, "{day}: {s:?}");
        }
    }
    #[test]
    fn model_rejects_unsupported_and_nonfinite_inputs() {
        for (day, us, lat, lon) in [
            (-1, 0, LAT, LON),
            (36525, 0, LAT, LON),
            (0, 86_401_000_000, LAT, LON),
            (0, 0, f64::NAN, LON),
            (0, 0, f64::INFINITY, LON),
            (0, 0, LAT, f64::NEG_INFINITY),
            (0, 0, 90.1, LON),
            (0, 0, LAT, -180.1),
        ] {
            assert_eq!(position(day, us, lat, lon), Err(InvalidSolarInput));
        }
    }
    #[test]
    fn leap_label_solar_phase_is_finite_and_continuous() {
        //2016-12-31 second59,60 and2017-01-01 midnight. Uniform integer
        // instants remain distinct in ws-time, though60/midnight share phase.
        let a = position(6209, 86_399_000_000, LAT, LON).unwrap();
        let b = position(6209, 86_400_000_000, LAT, LON).unwrap();
        let c = position(6210, 0, LAT, LON).unwrap();
        assert!((a.toward_sun - b.toward_sun).length() < 0.0001);
        assert_eq!(b, c);
    }
    proptest! {
        #[test]
        fn bounded_model_is_finite_unit_and_consistent_with_angles(day in 0i64..36525,us in 0u64..86_401_000_000,lat in -90.0f64..90.0,lon in -180.0f64..180.0) {
            let s=position(day,us,lat,lon).unwrap();
            prop_assert!((0.0..360.0).contains(&s.azimuth_deg));
            prop_assert!((-90.0..=90.0).contains(&s.elevation_deg));
            prop_assert!((s.toward_sun.length()-1.0).abs()<2e-15);
            let az=radians(s.azimuth_deg);let el=radians(s.elevation_deg);
            let from_angles=Vector::<Enu>::new(libm::sin(az)*libm::cos(el),libm::cos(az)*libm::cos(el),libm::sin(el));
            prop_assert!((s.toward_sun-from_angles).length()<2e-14);
        }
    }
}
