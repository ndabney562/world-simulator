//! Independent interval reference and integer/Tier-1 schedule tests.
use proptest::prelude::*;
use std::fmt::Write;
use ws_life::{Activity, Answer, Location, Place, Schedules};
use ws_time::SimTime;
use ws_world::slice::population::{PopulatedSlice, Role, WorkplaceLocation};
const MINUTE: i128 = 60_000_000;
const DAY: i128 = 1440 * MINUTE;
fn t(us: i64) -> SimTime {
    SimTime::from_micros_since_epoch(us)
}
fn reference(
    pop: &PopulatedSlice,
    id: ws_ecs::EntityId,
    j: i16,
    epoch: SimTime,
    time: SimTime,
) -> Answer {
    let r = pop.residents().get(id).unwrap();
    let home = Place::Building(r.home);
    let shop = pop
        .workplaces()
        .iter()
        .find_map(|(_, w)| match w.location {
            WorkplaceLocation::Building(b) => Some(Place::Building(b)),
            _ => None,
        })
        .unwrap();
    let duty = r
        .workplace
        .map(|id| match pop.workplaces().get(id).unwrap().location {
            WorkplaceLocation::Building(b) => Place::Building(b),
            WorkplaceLocation::Exit(e) => Place::OffMap(id, e),
        })
        .unwrap_or(home);
    let delta =
        i128::from(time.as_micros_since_epoch()) - i128::from(epoch.as_micros_since_epoch());
    let day = delta.div_euclid(DAY);
    let day_start = i128::from(epoch.as_micros_since_epoch()) + day * DAY;
    let phase = delta.rem_euclid(DAY);
    // Independently written interval list from the accepted minute policy.
    let working = day.rem_euclid(7) < 5 && matches!(r.role, Role::Student | Role::Employed);
    let end = if r.role == Role::Student { 900 } else { 1020 };
    let intervals = if working {
        vec![
            (0, 390, Activity::Sleep, home, None),
            (390, 450, Activity::Home, home, None),
            (450, 480, Activity::Commute, duty, Some(home)),
            (
                480,
                end,
                if r.role == Role::Student {
                    Activity::School
                } else {
                    Activity::Work
                },
                duty,
                None,
            ),
            (end, end + 30, Activity::Commute, home, Some(duty)),
            (end + 30, 1080, Activity::Home, home, None),
            (1080, 1090, Activity::Commute, shop, Some(home)),
            (1090, 1120, Activity::Errands, shop, None),
            (1120, 1130, Activity::Commute, home, Some(shop)),
            (1130, 1320, Activity::Home, home, None),
            (1320, 1440, Activity::Sleep, home, None),
        ]
    } else {
        vec![
            (0, 480, Activity::Sleep, home, None),
            (480, 660, Activity::Home, home, None),
            (660, 670, Activity::Commute, shop, Some(home)),
            (670, 700, Activity::Errands, shop, None),
            (700, 710, Activity::Commute, home, Some(shop)),
            (710, 1320, Activity::Home, home, None),
            (1320, 1440, Activity::Sleep, home, None),
        ]
    };
    let (start, end, activity, destination, from) = intervals
        .into_iter()
        .find(|(a, b, _, _, _)| {
            let a = if *a == 0 { 0 } else { a + i32::from(j) };
            let b = if *b == 1440 { 1440 } else { b + i32::from(j) };
            i128::from(a) * MINUTE <= phase && phase < i128::from(b) * MINUTE
        })
        .unwrap();
    let location = if let Some(from) = from {
        Location::InTransit {
            from,
            to: destination,
            departure: day_start + i128::from(start + i32::from(j)) * MINUTE,
            arrival: day_start + i128::from(end + i32::from(j)) * MINUTE,
        }
    } else {
        destination.into()
    };
    Answer { activity, location }
}
#[test]
fn every_resident_follows_independent_intervals_for_a_week_and_before_epoch() {
    for seed in [0, 42, u64::MAX] {
        let pop = PopulatedSlice::new(seed).unwrap();
        let epoch = t(-123_400_000);
        let schedules = Schedules::new(&pop, epoch).unwrap();
        for minute in -1440..=7 * 1440 {
            let now = t(epoch.as_micros_since_epoch() + i64::from(minute) * 60_000_000);
            for p in schedules.profiles() {
                assert_eq!(
                    p.at(epoch, now),
                    reference(&pop, p.resident, p.jitter, epoch, now)
                );
            }
        }
        for day in -1..8i64 {
            for p in schedules.profiles() {
                for row in p.weekday().iter().chain(p.weekend()) {
                    let boundary = epoch.as_micros_since_epoch()
                        + day * 86_400_000_000
                        + i64::from(row.minute) * 60_000_000;
                    for offset in [-1, 0, 1] {
                        let now = t(boundary + offset);
                        assert_eq!(
                            p.at(epoch, now),
                            reference(&pop, p.resident, p.jitter, epoch, now)
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn boundaries_are_strict_sorted_and_midnight_does_not_fabricate_facts() {
    let pop = PopulatedSlice::new(42).unwrap();
    let s = Schedules::new(&pop, t(0)).unwrap();
    let mut now = t(-86_400_000_000);
    while let Some(next) = s.next_change(now) {
        if next > t(7 * 86_400_000_000) {
            break;
        }
        assert!(next > now);
        assert_eq!(next.as_micros_since_epoch() % 100_000, 0);
        let facts = s.facts_at(next).unwrap();
        assert!(!facts.is_empty());
        for fact in facts {
            assert_eq!(fact.time, next);
        }
        now = next;
    }
    for day in -1..8 {
        assert!(s.facts_at(t(day * 86_400_000_000)).unwrap().is_empty());
    }
}
#[test]
fn new_goldens_freeze_full_tables_and_independent_household_draws() {
    let mut text = String::new();
    for seed in [0, 42, u64::MAX] {
        let pop = PopulatedSlice::new(seed).unwrap();
        let s = Schedules::new(&pop, t(0)).unwrap();
        for p in s.profiles() {
            write!(
                text,
                "{seed:016x} {:016x} {:016x} {} {}",
                p.resident.raw(),
                p.household.raw(),
                p.role as u8,
                p.jitter
            )
            .unwrap();
            for table in [p.weekday(), p.weekend()] {
                text.push(' ');
                text.push_str(
                    &table
                        .iter()
                        .map(|r| format!("{}:{}", r.minute, r.activity as u8))
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            text.push('\n');
        }
    }
    assert_eq!(text, include_str!("golden/schedules.txt"));
}
#[test]
fn every_known_activity_tag_is_explicit_and_others_are_rejected() {
    for tag in 0..=255 {
        assert_eq!(
            Activity::from_tag(tag).map(|a| a as u8),
            if tag < 6 { Some(tag) } else { None }
        );
    }
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]
    #[test]
    fn total_extreme_time_queries_have_one_place_and_preserve_off_map_identity(seed in any::<u64>(),epoch in any::<i64>(),now in any::<i64>()) {
        let pop=PopulatedSlice::new(seed).unwrap();let s=Schedules::new(&pop,t(epoch)).unwrap();
        for p in s.profiles() {
            let a=p.at(t(epoch),t(now));prop_assert_eq!(a,reference(&pop,p.resident,p.jitter,t(epoch),t(now)));
            if let Location::OffMap(id,e)=a.location { prop_assert_eq!(pop.workplaces().get(id).unwrap().location,WorkplaceLocation::Exit(e)); }
            if let Location::InTransit {departure,arrival,..}=a.location { prop_assert!(departure<=i128::from(now)&&i128::from(now)<arrival); }
            if let Some(next)=p.next_change(t(epoch),t(now)) {prop_assert!(next>t(now));prop_assert_ne!(p.at(t(epoch),t(next.as_micros_since_epoch()-1)).activity,p.at(t(epoch),next).activity);}
        }
    }
}
#[test]
fn i64_endpoints_and_opposite_epochs_do_not_overflow_or_clamp_journeys() {
    let pop = PopulatedSlice::new(0).unwrap();
    for epoch in [i64::MIN, i64::MAX, 0] {
        let s = Schedules::new(&pop, t(epoch)).unwrap();
        for now in [i64::MIN, i64::MAX, -1, 0, 1] {
            for p in s.profiles() {
                assert_eq!(
                    p.at(t(epoch), t(now)),
                    reference(&pop, p.resident, p.jitter, t(epoch), t(now))
                );
            }
            if now == i64::MAX {
                assert_eq!(s.next_change(t(now)), None);
            }
        }
    }
}
