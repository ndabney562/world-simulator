//! Bounded, frozen New York labelling and model-week alignment (ADR0030).
use crate::civil::{civil_from_days, days_from_civil};
use crate::{SimTime, UtcDateTime};

/// Frozen EST/EDT rules for UTC years2000–2099; not an operating-system timezone.
pub const NY_POLICY_VERSION: u32 = 1;

/// A local civil label, deliberately distinct from a validated UTC label.
/// `offset_seconds` distinguishes the two occurrences of the repeated fall hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    /// Astronomical Gregorian year (can be1999 at the supported UTC lower edge).
    pub year: i32,
    /// Month1..=12.
    pub month: u8,
    /// Day of month.
    pub day: u8,
    /// Hour0..=23.
    pub hour: u8,
    /// Minute0..=59.
    pub minute: u8,
    /// Second0..=60, retaining a tabulated UTC leap second at its local minute.
    pub second: u8,
    /// Microsecond0..=999999.
    pub microsecond: u32,
    /// EST−18000 or EDT−14400; also the unambiguous zone tag.
    pub offset_seconds: i32,
}

/// Creation is outside the calendar-alignment policy's UTC2000–2099 window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedAlignment;
impl core::fmt::Display for UnsupportedAlignment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("model-Monday alignment requires a UTC2000–2099 creation instant")
    }
}
impl std::error::Error for UnsupportedAlignment {}

// Sunday=0. January1,2000 was Saturday.
fn weekday(day: i64) -> i64 {
    (day + 6).rem_euclid(7)
}
fn first_sunday(year: i32, month: i64) -> i64 {
    let first = days_from_civil(i64::from(year), month, 1);
    first + (-weekday(first)).rem_euclid(7)
}
fn transition_days(year: i32) -> (i64, i64) {
    if year < 2007 {
        let last_october = days_from_civil(i64::from(year), 10, 31);
        (first_sunday(year, 4), last_october - weekday(last_october))
    } else {
        (first_sunday(year, 3) + 7, first_sunday(year, 11))
    }
}
fn supported(utc: UtcDateTime) -> bool {
    (2000..=2099).contains(&utc.year())
}

/// Exact UTC→New York label under policy1, or explicit unavailability outside
/// UTC2000–2099. No timezone database, wall clock or floating-point arithmetic.
#[must_use]
pub fn new_york(utc: UtcDateTime) -> Option<LocalDateTime> {
    if !supported(utc) {
        return None;
    }
    let (spring, fall) = transition_days(utc.year());
    // 02:00 EST =07:00 UTC;02:00 EDT =06:00 UTC. A leap label
    // remains ordered within its previous minute, never across either transition.
    let nominal = utc.naive_seconds() + 43_200;
    let offset_seconds =
        if nominal >= spring * 86_400 + 7 * 3600 && nominal < fall * 86_400 + 6 * 3600 {
            -14_400
        } else {
            -18_000
        };
    let shifted = nominal + i64::from(offset_seconds);
    let (year, month, day) = civil_from_days(shifted.div_euclid(86_400));
    let sod = shifted.rem_euclid(86_400);
    Some(LocalDateTime {
        year: year as i32,
        month,
        day,
        hour: (sod / 3600) as u8,
        minute: (sod / 60 % 60) as u8,
        second: if utc.is_leap_second() {
            60
        } else {
            (sod % 60) as u8
        },
        microsecond: utc.microsecond(),
        offset_seconds,
    })
}

/// Latest New York Monday midnight at/before creation, using policy1.
/// The resulting SimTime becomes the stored uniform model-week epoch; later
/// DST/leap boundaries do not retime synthetic appointments. Includes the
/// explicit alignment-only1999-12-27 EST predecessor at the lower edge.
/// # Errors
/// [`UnsupportedAlignment`] for creation outside UTC2000–2099.
pub fn model_monday_v1(creation: SimTime) -> Result<SimTime, UnsupportedAlignment> {
    let local = new_york(creation.to_utc()).ok_or(UnsupportedAlignment)?;
    let day = days_from_civil(
        i64::from(local.year),
        i64::from(local.month),
        i64::from(local.day),
    );
    let monday = day - (day + 5).rem_euclid(7);
    let (year, _, _) = civil_from_days(monday);
    let offset = if year == 1999 {
        // The only out-of-window predecessor permitted by ADR0030.
        debug_assert_eq!(monday, days_from_civil(1999, 12, 27));
        -18_000
    } else {
        let (spring, fall) = transition_days(year as i32);
        // Monday is never a transition Sunday. At local midnight the
        // spring Sunday is still EST and the fall Sunday is still EDT.
        if monday > spring && monday <= fall {
            -14_400
        } else {
            -18_000
        }
    };
    let utc = UtcDateTime::from_naive(monday * 86_400 - offset - 43_200, false, 0);
    Ok(SimTime::from_utc(utc).expect("bounded alignment predecessor fits SimTime"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    fn utc(y: i32, m: u8, d: u8, h: u8, mi: u8, s: u8) -> UtcDateTime {
        UtcDateTime::new(y, m, d, h, mi, s, 0).unwrap()
    }
    #[test]
    fn frozen_policy_offsets_and_repeated_hour_match_reference() {
        // NIST current rules and FDIC2007 change notice, independently checked
        // against host IANA America/New_York labels; never a runtime dependency.
        for (u, expected) in [
            (utc(2000, 4, 2, 6, 59, 59), (2000, 4, 2, 1, 59, 59, -18000)),
            (utc(2000, 4, 2, 7, 0, 0), (2000, 4, 2, 3, 0, 0, -14400)),
            (
                utc(2006, 10, 29, 5, 59, 59),
                (2006, 10, 29, 1, 59, 59, -14400),
            ),
            (utc(2006, 10, 29, 6, 0, 0), (2006, 10, 29, 1, 0, 0, -18000)),
            (
                utc(2007, 3, 11, 6, 59, 59),
                (2007, 3, 11, 1, 59, 59, -18000),
            ),
            (utc(2007, 3, 11, 7, 0, 0), (2007, 3, 11, 3, 0, 0, -14400)),
            (utc(2026, 3, 8, 6, 59, 59), (2026, 3, 8, 1, 59, 59, -18000)),
            (utc(2026, 3, 8, 7, 0, 0), (2026, 3, 8, 3, 0, 0, -14400)),
            (
                utc(2026, 11, 1, 5, 59, 59),
                (2026, 11, 1, 1, 59, 59, -14400),
            ),
            (utc(2026, 11, 1, 6, 0, 0), (2026, 11, 1, 1, 0, 0, -18000)),
        ] {
            let l = new_york(u).unwrap();
            assert_eq!(
                (
                    l.year,
                    l.month,
                    l.day,
                    l.hour,
                    l.minute,
                    l.second,
                    l.offset_seconds
                ),
                expected
            );
        }
    }
    #[test]
    fn lower_upper_edges_leap_day_and_distinct_local_leap_label() {
        let l = new_york(utc(2000, 1, 1, 0, 0, 0)).unwrap();
        assert_eq!((l.year, l.month, l.day, l.hour), (1999, 12, 31, 19));
        assert!(new_york(utc(1999, 12, 31, 23, 59, 59)).is_none());
        assert!(new_york(utc(2099, 12, 31, 23, 59, 59)).is_some());
        assert!(new_york(utc(2100, 1, 1, 0, 0, 0)).is_none());
        assert_eq!(new_york(utc(2024, 2, 29, 12, 0, 0)).unwrap().day, 29);
        for (y, m, d) in [
            (2005, 12, 31),
            (2008, 12, 31),
            (2012, 6, 30),
            (2015, 6, 30),
            (2016, 12, 31),
        ] {
            let u = UtcDateTime::new(y, m, d, 23, 59, 60, 123456).unwrap();
            let l = new_york(u).unwrap();
            assert_eq!(
                (l.year, l.month, l.day, l.minute, l.second, l.microsecond),
                (y, m, d, 59, 60, 123456)
            );
            assert_eq!(i32::from(l.hour), 23 + l.offset_seconds / 3600);
            assert_eq!(u.nominal_micros_of_day(), 86_400_123_456);
        }
        assert!(new_york(SimTime::MIN.to_utc()).is_none());
        assert!(new_york(SimTime::MAX.to_utc()).is_none());
    }
    #[test]
    fn model_monday_predecessor_year_crossing_exact_midnight_and_dst_weeks() {
        for (creation, expected) in [
            (utc(2000, 1, 1, 0, 0, 0), utc(1999, 12, 27, 5, 0, 0)),
            (utc(2000, 1, 3, 5, 0, 0), utc(2000, 1, 3, 5, 0, 0)),
            (utc(2026, 1, 1, 0, 0, 0), utc(2025, 12, 29, 5, 0, 0)),
            (utc(2026, 3, 8, 12, 0, 0), utc(2026, 3, 2, 5, 0, 0)),
            (utc(2026, 3, 9, 4, 0, 0), utc(2026, 3, 9, 4, 0, 0)),
            (utc(2026, 11, 1, 12, 0, 0), utc(2026, 10, 26, 4, 0, 0)),
            (utc(2026, 11, 2, 5, 0, 0), utc(2026, 11, 2, 5, 0, 0)),
        ] {
            assert_eq!(
                model_monday_v1(SimTime::from_utc(creation).unwrap()),
                Ok(SimTime::from_utc(expected).unwrap())
            );
        }
        for t in [
            SimTime::MIN,
            SimTime::MAX,
            SimTime::from_utc(utc(2100, 1, 1, 0, 0, 0)).unwrap(),
        ] {
            assert_eq!(model_monday_v1(t), Err(UnsupportedAlignment));
        }
    }
    proptest! {
        #[test]
        fn local_labels_and_model_alignment_preserve_integer_calendar(day in 0i64..36525,hour in 0u8..24, minute in 0u8..60,second in 0u8..60,micro in 0u32..1_000_000) {
            let (y,m,d)=civil_from_days(day);
            let u=UtcDateTime::new(y as i32,m,d,hour,minute,second,micro).unwrap();
            let l=new_york(u).unwrap();
            let shifted=days_from_civil(i64::from(l.year),i64::from(l.month),i64::from(l.day))*86400+i64::from(l.hour)*3600+i64::from(l.minute)*60+i64::from(l.second);
            prop_assert_eq!(shifted-i64::from(l.offset_seconds),u.naive_seconds()+43200);
            prop_assert_eq!(l.microsecond,micro);
            let t=SimTime::from_utc(u).unwrap();
            let epoch=model_monday_v1(t).unwrap();
            prop_assert!(epoch<=t);
            prop_assert!(t.checked_duration_since(epoch).unwrap().as_micros()<608_402_000_000);
            let e=epoch.to_utc();
            let (ed,eh,em,es)=if e.year()==1999 {(e.day_index(),0,0,0)} else {
                let label=new_york(e).unwrap();
                (days_from_civil(i64::from(label.year),i64::from(label.month),i64::from(label.day)),label.hour,label.minute,label.second)
            };
            prop_assert_eq!((ed+5).rem_euclid(7),0);
            prop_assert_eq!((eh,em,es),(0,0,0));
            prop_assert_eq!(epoch.as_micros_since_epoch().rem_euclid(100_000),0);
        }
    }
}
