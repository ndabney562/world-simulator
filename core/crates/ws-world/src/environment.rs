//! Pure committed-time calendar and solar composition at the registered site.
//! No mutable sky state, simulation decisions, random draws or emitted events.
use crate::slice::{BLOCK_BUILDINGS, GeoPoint};
pub use ws_math::solar::SOLAR_VERSION;
use ws_math::{Enu, EnuFrame, Geodetic, Vector, solar};
pub use ws_time::{LEAP_SECOND_TABLE_VERSION, NY_POLICY_VERSION};
use ws_time::{LocalDateTime, SimTime, UtcDateTime, new_york};

const fn source_site() -> GeoPoint {
    let (mut west, mut east, mut south, mut north) = (i64::MAX, i64::MIN, i64::MAX, i64::MIN);
    let mut b = 0;
    while b < BLOCK_BUILDINGS.len() {
        let polygons = BLOCK_BUILDINGS[b].footprint.polygons;
        let mut p = 0;
        while p < polygons.len() {
            let mut r = 0;
            while r < polygons[p].rings.len() {
                let points = polygons[p].rings[r].points;
                let mut v = 0;
                while v < points.len() {
                    let point = points[v];
                    if point.lon_pdeg < west {
                        west = point.lon_pdeg;
                    }
                    if point.lon_pdeg > east {
                        east = point.lon_pdeg;
                    }
                    if point.lat_pdeg < south {
                        south = point.lat_pdeg;
                    }
                    if point.lat_pdeg > north {
                        north = point.lat_pdeg;
                    }
                    v += 1;
                }
                r += 1;
            }
            p += 1;
        }
        b += 1;
    }
    GeoPoint {
        lon_pdeg: ((west as i128 + east as i128).div_euclid(2)) as i64,
        lat_pdeg: ((south as i128 + north as i128).div_euclid(2)) as i64,
    }
}
/// Complete58-footprint envelope midpoint, evaluated once at compile time.
/// Integer i128 sum and floor division preserve negative-longitude rounding.
pub const SOLAR_SITE: GeoPoint = source_site();
/// Version of the already registered extract supplying the fixed solar site.
pub const SITE_DATASET_VERSION: u64 = crate::slice::DATASET_VERSION;

/// Supported local calendar plus geometric solar result in the requested basis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalSky {
    /// Unambiguous New York civil label with EST/EDT offset.
    pub local: LocalDateTime,
    /// Unit vector toward the sun in the paired render ENU basis.
    pub toward_sun: Vector<Enu>,
    /// Clockwise from north at the fixed site, not at the render origin.
    pub azimuth_deg: f64,
    /// Geometric elevation at the fixed site.
    pub elevation_deg: f64,
}
/// Pure environmental projection. UTC is exact across the full SimTime range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalendarSky {
    /// Exact committed UTC label, including second60.
    pub utc: UtcDateTime,
    /// None outside UTC2000–2099 or without a paired render basis.
    pub supported: Option<LocalSky>,
}
/// Site's geodetic tangent frame. It is not the navigation-millimetre plane.
#[must_use]
pub fn site_frame() -> EnuFrame {
    EnuFrame::new(
        Geodetic::from_degrees(
            SOLAR_SITE.lat_pdeg as f64 / 1e12,
            SOLAR_SITE.lon_pdeg as f64 / 1e12,
            0.0,
        )
        .expect("registered bounded site"),
    )
}
/// Derive immutable calendar/sun output, with the direction explicitly rotated
/// site ENU→ECEF→paired render ENU. The dedicated direction is Tier1 under
/// ADR0030; this does not reclassify general continuous coordinate conversions.
#[must_use]
pub fn calendar_sky(now: SimTime, render_basis: Option<EnuFrame>) -> CalendarSky {
    let utc = now.to_utc();
    let supported = new_york(utc).zip(render_basis).map(|(local, basis)| {
        let sun = solar::position(
            utc.day_index(),
            utc.nominal_micros_of_day(),
            SOLAR_SITE.lat_pdeg as f64 / 1e12,
            SOLAR_SITE.lon_pdeg as f64 / 1e12,
        )
        .expect("supported validated UTC and registered site");
        LocalSky {
            local,
            toward_sun: basis.vector_to_enu(site_frame().vector_to_ecef(sun.toward_sun)),
            azimuth_deg: sun.azimuth_deg,
            elevation_deg: sun.elevation_deg,
        }
    });
    CalendarSky { utc, supported }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    #[test]
    fn complete_source_envelope_midpoint_and_versions_are_pinned() {
        assert_eq!(
            SOLAR_SITE,
            GeoPoint {
                lon_pdeg: -74030160593242,
                lat_pdeg: 40622079256973
            }
        );
        assert_eq!(BLOCK_BUILDINGS.len(), 58);
        assert_eq!((SOLAR_VERSION, NY_POLICY_VERSION), (1, 1));
        assert_eq!(SITE_DATASET_VERSION, 0x85183751d4f793b9);
    }
    #[test]
    fn outside_range_is_exact_utc_and_explicit_unavailable_sky() {
        for now in [SimTime::MIN, SimTime::MAX] {
            let s = calendar_sky(now, Some(site_frame()));
            assert_eq!(s.utc, now.to_utc());
            assert_eq!(s.supported, None);
        }
        assert_eq!(calendar_sky(SimTime::EPOCH, None).supported, None);
    }
    proptest! {
        #[test]
        fn paired_snapshot_basis_rotates_directions_without_translation(day in 0i64..36525,second in 0i64..86400,dlat in -0.01f64..0.01,dlon in -0.01f64..0.01) {
            let t=SimTime::EPOCH.checked_add(ws_time::SimDuration::from_micros((day*86400+second)*1_000_000)).unwrap();
            // Keep final-day input inside the actual UTC window.
            if new_york(t.to_utc()).is_none() {return Ok(());}
            let site=site_frame().origin();
            let basis=EnuFrame::new(Geodetic::from_degrees(site.lat().to_degrees()+dlat,site.lon().to_degrees()+dlon,0.0).unwrap());
            let s=calendar_sky(t,Some(basis)).supported.unwrap();
            prop_assert!((s.toward_sun.length()-1.0).abs()<3e-15);
            let at_site=calendar_sky(t,Some(site_frame())).supported.unwrap();
            let world_vector=basis.vector_to_ecef(s.toward_sun);
            let site_vector=site_frame().vector_to_ecef(at_site.toward_sun);
            prop_assert!((world_vector-site_vector).length()<2e-15);
            prop_assert_eq!(s.azimuth_deg,at_site.azimuth_deg);
            prop_assert_eq!(s.elevation_deg,at_site.elevation_deg);
        }
    }
}
