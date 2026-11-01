//! Coarse pure UTC configuration and the committed calendar/sky record (ADR0030).
use super::*;

/// Validated UTC input label, exactly40 bytes aligned to4.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WsUtcLabel {
    /// Current ABI (11).
    pub abi_version: u32,
    /// Exactly40.
    pub struct_size: u32,
    /// Astronomical Gregorian year; conversion separately checks SimTime range.
    pub year: i32,
    /// Month1..=12.
    pub month: u32,
    /// Day of month.
    pub day: u32,
    /// Hour0..=23.
    pub hour: u32,
    /// Minute0..=59.
    pub minute: u32,
    /// Second0..=60,60 only at a tabulated UTC leap second.
    pub second: u32,
    /// Microsecond0..=999999.
    pub microsecond: u32,
    /// Must be zero.
    pub reserved: u32,
}

/// Coherent environment output, exactly160 bytes aligned to8. Initialize only
/// abi_version/struct_size before the call; all fields are written on success.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WsCalendarSky {
    /// Current ABI (11).
    pub abi_version: u32,
    /// Exactly160.
    pub struct_size: u32,
    /// Committed physical tick; pair with the render snapshot under one gate.
    pub tick: u64,
    /// Committed uniform-SI microseconds from the simulation epoch.
    pub sim_time_us: i64,
    /// Canonical state hash; querying never changes it.
    pub state_hash: u64,
    /// Exact UTC astronomical Gregorian year.
    pub utc_year: i32,
    /// UTC month.
    pub utc_month: u32,
    /// UTC day.
    pub utc_day: u32,
    /// UTC hour.
    pub utc_hour: u32,
    /// UTC minute.
    pub utc_minute: u32,
    /// UTC second, including60.
    pub utc_second: u32,
    /// UTC microsecond.
    pub utc_microsecond: u32,
    /// New York Gregorian year; zero if unavailable.
    pub local_year: i32,
    /// New York month; zero if unavailable.
    pub local_month: u32,
    /// New York day; zero if unavailable.
    pub local_day: u32,
    /// New York hour; zero if unavailable.
    pub local_hour: u32,
    /// New York minute; zero if unavailable.
    pub local_minute: u32,
    /// New York second, preserving60; zero if unavailable.
    pub local_second: u32,
    /// New York microsecond; zero if unavailable.
    pub local_microsecond: u32,
    /// EST−18000/EDT−14400, or zero if unavailable.
    pub utc_offset_seconds: i32,
    /// Committed scale0/1/10/100, never pending commands.
    pub scale_tag: u32,
    /// Pinned solar rule version.
    pub solar_version: u32,
    /// Pinned New York rule version.
    pub ny_policy_version: u32,
    /// Bit0=supported local/solar; all other bits zero.
    pub flags: u32,
    /// Embedded UTC leap table version.
    pub leap_table_version: u32,
    /// Registered block dataset providing the solar site.
    pub site_dataset_version: u64,
    /// Toward-sun east in the paired render ENU basis; zero if unavailable.
    pub sun_east: f64,
    /// Toward-sun north in the paired render ENU basis; zero if unavailable.
    pub sun_north: f64,
    /// Toward-sun up in the paired render ENU basis; zero if unavailable.
    pub sun_up: f64,
    /// Geometric azimuth clockwiseNorth at the fixed site; zero if unavailable.
    pub azimuth_deg: f64,
    /// Geometric elevation at the fixed site; zero if unavailable.
    pub elevation_deg: f64,
}
impl WsCalendarSky {
    pub(super) fn from_view(view: ws_sim::CalendarSky) -> Self {
        let utc = view.environment.utc;
        let mut out = Self {
            abi_version: ABI_VERSION,
            struct_size: 160,
            tick: view.tick,
            sim_time_us: view.time.as_micros_since_epoch(),
            state_hash: view.state_hash,
            utc_year: utc.year(),
            utc_month: u32::from(utc.month()),
            utc_day: u32::from(utc.day()),
            utc_hour: u32::from(utc.hour()),
            utc_minute: u32::from(utc.minute()),
            utc_second: u32::from(utc.second()),
            utc_microsecond: utc.microsecond(),
            local_year: 0,
            local_month: 0,
            local_day: 0,
            local_hour: 0,
            local_minute: 0,
            local_second: 0,
            local_microsecond: 0,
            utc_offset_seconds: 0,
            scale_tag: view.scale_tag,
            solar_version: ws_sim::SOLAR_VERSION,
            ny_policy_version: ws_sim::NY_POLICY_VERSION,
            flags: 0,
            leap_table_version: ws_sim::LEAP_SECOND_TABLE_VERSION,
            site_dataset_version: ws_sim::SITE_DATASET_VERSION,
            sun_east: 0.0,
            sun_north: 0.0,
            sun_up: 0.0,
            azimuth_deg: 0.0,
            elevation_deg: 0.0,
        };
        if let Some(s) = view.environment.supported {
            let l = s.local;
            out.local_year = l.year;
            out.local_month = u32::from(l.month);
            out.local_day = u32::from(l.day);
            out.local_hour = u32::from(l.hour);
            out.local_minute = u32::from(l.minute);
            out.local_second = u32::from(l.second);
            out.local_microsecond = l.microsecond;
            out.utc_offset_seconds = l.offset_seconds;
            out.flags = 1;
            out.sun_east = s.toward_sun.x;
            out.sun_north = s.toward_sun.y;
            out.sun_up = s.toward_sun.z;
            out.azimuth_deg = s.azimuth_deg;
            out.elevation_deg = s.elevation_deg;
        }
        out
    }
}
fn convert(input: WsUtcLabel) -> Result<i64, (WsStatus, &'static str)> {
    if input.abi_version != ABI_VERSION {
        return Err((VERSION_MISMATCH, "UTC-label ABI version mismatch"));
    }
    if input.struct_size != 40 || input.reserved != 0 {
        return Err((INVALID_ARGUMENT, "UTC-label size/reserved invalid"));
    }
    let byte =
        |v: u32| u8::try_from(v).map_err(|_| (INVALID_ARGUMENT, "UTC-label field out of range"));
    let utc = ws_sim::UtcDateTime::new(
        input.year,
        byte(input.month)?,
        byte(input.day)?,
        byte(input.hour)?,
        byte(input.minute)?,
        byte(input.second)?,
        input.microsecond,
    )
    .map_err(|_| (INVALID_ARGUMENT, "invalid UTC civil label"))?;
    ws_sim::SimTime::from_utc(utc)
        .map(|t| t.as_micros_since_epoch())
        .map_err(|_| (INVALID_ARGUMENT, "UTC instant outside SimTime range"))
}

/// Pure UTC configuration conversion; no world, clock read or grid rounding.
/// Only success writes output. World creation separately enforces100-ms grid.
/// Errors and caught unwinds use thread-local diagnostics; no handle is poisoned.
/// # Safety
/// Non-null input points to one initialized aligned40-byte label; non-null
/// output points to writable aligned i64 storage, disjoint from input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_utc_to_sim(input: *const WsUtcLabel, out_time: *mut i64) -> WsStatus {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if input.is_null() || !input.is_aligned() || out_time.is_null() || !out_time.is_aligned() {
            return Err((
                INVALID_ARGUMENT,
                "UTC conversion pointer is null or unaligned",
            ));
        }
        // SAFETY: caller supplies initialized aligned input, retained only for this call.
        convert(unsafe { input.read() })
    }));
    match result {
        Ok(Ok(time)) => {
            // SAFETY: pointer validated before calculation; caller guarantees writable/disjoint storage.
            unsafe {
                out_time.write(time);
            }
            thread_error("");
            OK
        }
        Ok(Err((status, message))) => {
            thread_error(message);
            status
        }
        Err(_) => {
            thread_error("panic during UTC conversion");
            PANIC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WsSliceConfig, ws_slice_create, ws_slice_last_error};
    use std::ptr::{null, null_mut};
    fn label() -> WsUtcLabel {
        WsUtcLabel {
            abi_version: ABI_VERSION,
            struct_size: 40,
            year: 2026,
            month: 6,
            day: 21,
            hour: 17,
            minute: 0,
            second: 0,
            microsecond: 0,
            reserved: 0,
        }
    }
    fn config(start: i64) -> WsSliceConfig {
        WsSliceConfig {
            abi_version: ABI_VERSION,
            struct_size: 32,
            seed: 7,
            start_us: start,
            reserved: 0,
        }
    }
    #[test]
    fn calendar_abi_layout_matches_every_contract_offset() {
        assert_eq!((size_of::<WsUtcLabel>(), align_of::<WsUtcLabel>()), (40, 4));
        macro_rules! utc_offsets {($($field:ident:$offset:expr),+)=>{$(assert_eq!(std::mem::offset_of!(WsUtcLabel,$field),$offset);)+};}
        utc_offsets!(abi_version:0,struct_size:4,year:8,month:12,day:16,hour:20,minute:24,second:28,microsecond:32,reserved:36);
        assert_eq!(
            (size_of::<WsCalendarSky>(), align_of::<WsCalendarSky>()),
            (160, 8)
        );
        macro_rules! sky_offsets {($($field:ident:$offset:expr),+)=>{$(assert_eq!(std::mem::offset_of!(WsCalendarSky,$field),$offset);)+};}
        sky_offsets!(abi_version:0,struct_size:4,tick:8,sim_time_us:16,state_hash:24,utc_year:32,utc_month:36,utc_day:40,utc_hour:44,utc_minute:48,utc_second:52,utc_microsecond:56,local_year:60,local_month:64,local_day:68,local_hour:72,local_minute:76,local_second:80,local_microsecond:84,utc_offset_seconds:88,scale_tag:92,solar_version:96,ny_policy_version:100,flags:104,leap_table_version:108,site_dataset_version:112,sun_east:120,sun_north:128,sun_up:136,azimuth_deg:144,elevation_deg:152);
    }

    #[test]
    fn utc_conversion_is_exact_unrounded_and_errors_never_write() {
        // SAFETY: initialized disjoint stack inputs/outputs and documented null/misaligned paths.
        unsafe {
            let mut out = 999i64;
            let mut input = label();
            assert_eq!(ws_utc_to_sim(&input, &mut out), OK);
            let expected = ws_sim::SimTime::from_utc(
                ws_sim::UtcDateTime::new(2026, 6, 21, 17, 0, 0, 0).unwrap(),
            )
            .unwrap();
            assert_eq!(out, expected.as_micros_since_epoch());
            for t in [ws_sim::SimTime::MIN, ws_sim::SimTime::MAX] {
                let u = t.to_utc();
                let input = WsUtcLabel {
                    year: u.year(),
                    month: u.month().into(),
                    day: u.day().into(),
                    hour: u.hour().into(),
                    minute: u.minute().into(),
                    second: u.second().into(),
                    microsecond: u.microsecond(),
                    ..label()
                };
                assert_eq!(ws_utc_to_sim(&input, &mut out), OK);
                assert_eq!(out, t.as_micros_since_epoch());
            }
            input = WsUtcLabel {
                year: 2016,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 60,
                microsecond: 123456,
                ..label()
            };
            assert_eq!(ws_utc_to_sim(&input, &mut out), OK);
            let leap = ws_sim::SimTime::from_micros_since_epoch(out).to_utc();
            assert_eq!((leap.second(), leap.microsecond()), (60, 123456));
            input = WsUtcLabel {
                microsecond: 1,
                ..label()
            };
            assert_eq!(ws_utc_to_sim(&input, &mut out), OK);
            assert_eq!(out.rem_euclid(100000), 1);
            assert!(
                ws_slice_create(&config(out)).is_null(),
                "conversion does not silently snap creation grid"
            );
            for bad in [
                WsUtcLabel {
                    year: 2023,
                    month: 2,
                    day: 29,
                    ..label()
                },
                WsUtcLabel {
                    year: i32::MAX,
                    ..label()
                },
                WsUtcLabel {
                    month: 257,
                    ..label()
                },
                WsUtcLabel { day: 0, ..label() },
                WsUtcLabel {
                    hour: 24,
                    ..label()
                },
                WsUtcLabel {
                    minute: 60,
                    ..label()
                },
                WsUtcLabel {
                    second: 60,
                    ..label()
                },
                WsUtcLabel {
                    microsecond: 1000000,
                    ..label()
                },
                WsUtcLabel {
                    reserved: 1,
                    ..label()
                },
                WsUtcLabel {
                    struct_size: 36,
                    ..label()
                },
            ] {
                out = 999;
                assert_eq!(ws_utc_to_sim(&bad, &mut out), INVALID_ARGUMENT);
                assert_eq!(out, 999);
            }
            out = 999;
            assert_eq!(
                ws_utc_to_sim(
                    &WsUtcLabel {
                        abi_version: ABI_VERSION - 1,
                        ..label()
                    },
                    &mut out
                ),
                VERSION_MISMATCH
            );
            assert_eq!(out, 999);
            assert_eq!(ws_utc_to_sim(null(), &mut out), INVALID_ARGUMENT);
            assert_eq!(out, 999);
            assert_eq!(ws_utc_to_sim(&label(), null_mut()), INVALID_ARGUMENT);
            let mut aligned = [0x5a5a5a5a5a5a5a5au64; 6];
            let unaligned = aligned.as_mut_ptr().cast::<u8>().add(1);
            assert_eq!(ws_utc_to_sim(&label(), unaligned.cast()), INVALID_ARGUMENT);
            assert_eq!(ws_utc_to_sim(unaligned.cast(), &mut out), INVALID_ARGUMENT);
            assert_eq!(aligned, [0x5a5a5a5a5a5a5a5a; 6]);
            assert_eq!(out, 999);
            assert!(ws_slice_last_error(null_mut(), null_mut(), 0) > 1);
        }
    }
}
