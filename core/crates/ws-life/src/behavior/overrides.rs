//! ADR0034 continuous temporary routines. All endpoints stay on existing routes.
use super::*;
use crate::{Answer, Schedule};

/// A complete logical journey; wide endpoints preserve extreme-time intervals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Journey {
    /// Registered origin.
    pub from: Place,
    /// Registered destination.
    pub to: Place,
    /// Inclusive absolute departure microseconds.
    pub departure: i128,
    /// Exclusive absolute arrival microseconds.
    pub arrival: i128,
}
impl Journey {
    pub(super) fn answer(self) -> Answer {
        Answer {
            activity: Activity::Commute,
            location: Location::InTransit {
                from: self.from,
                to: self.to,
                departure: self.departure,
                arrival: self.arrival,
            },
        }
    }
}
/// Retained temporary routine, independent of detail membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutineOverride {
    /// 1: illness/home hold, 2: neighbour visit, 3: gathering (target is `original.to`).
    pub kind: u32,
    /// Producing intervention EventId, represented without a history dependency.
    pub root: u64,
    /// Registered home building.
    pub home: EntityId,
    /// Already-started leg, completed unchanged.
    pub original: Option<Journey>,
    /// New homebound leg, if illness actually produced one.
    pub homeward: Option<Journey>,
    /// Committed start; diagnostic projections before this use the baseline.
    pub started: SimTime,
    /// Illness end or intended visit end.
    pub until: SimTime,
    /// First continuous baseline rejoin; None if beyond SimTime.
    pub rejoin: Option<SimTime>,
}
fn place(location: Location) -> Place {
    match location {
        Location::AtBuilding(id) => Place::Building(id),
        Location::OffMap(id, e) => Place::OffMap(id, e),
        Location::InTransit { to, .. } => to,
    }
}
fn rejoin(profile: &Schedule, epoch: SimTime, home: EntityId, at: i128) -> Option<SimTime> {
    rejoin_at(profile, epoch, Place::Building(home), at)
}
/// First instant at or after `at` where the baseline is at `place` or departs it then.
pub(super) fn rejoin_at(
    profile: &Schedule,
    epoch: SimTime,
    place: Place,
    at: i128,
) -> Option<SimTime> {
    let mut t = SimTime::from_micros_since_epoch(i64::try_from(at).ok()?);
    // At most a week of the finite daily table; no unbounded search at extremes.
    for _ in 0..128 {
        let a = profile.at(epoch, t);
        if a.location == Location::from(place)
            || matches!(a.location,
            Location::InTransit{from,departure,..} if from==place&&departure==micros(t))
        {
            return Some(t);
        }
        t = profile.next_change(epoch, t)?;
    }
    None
}
impl RoutineOverride {
    pub(super) fn illness(
        world: &Behavior,
        id: EntityId,
        current: Answer,
        time: SimTime,
        until: SimTime,
        root: u64,
        home: EntityId,
    ) -> Result<Self, BehaviorError> {
        let original = match current.location {
            Location::InTransit {
                from,
                to,
                departure,
                arrival,
            } => Some(Journey {
                from,
                to,
                departure,
                arrival,
            }),
            _ => None,
        };
        let from = place(current.location);
        let departure = original.map_or(micros(time), |j| j.arrival);
        let homeward = if from == Place::Building(home) {
            None
        } else {
            let walking = i128::from(
                world
                    .navigation
                    .route(from, Place::Building(home))?
                    .walking_ticks(),
            ) * STEP;
            let duration = if matches!(from, Place::OffMap(..)) {
                walking.max(30 * MINUTE)
            } else {
                walking
            };
            Some(Journey {
                from,
                to: Place::Building(home),
                departure,
                arrival: departure + duration,
            })
        };
        let home_at = homeward.or(original).map_or(micros(time), |j| j.arrival);
        Ok(Self {
            started: time,
            kind: 1,
            root,
            home,
            original,
            homeward,
            until,
            rejoin: rejoin(
                world
                    .schedules
                    .get(id)
                    .ok_or(BehaviorError::UnknownResident)?,
                world.schedules.epoch(),
                home,
                home_at.max(micros(until)),
            ),
        })
    }
    pub(super) fn visit(
        world: &Behavior,
        id: EntityId,
        time: SimTime,
        until: SimTime,
        root: u64,
        home: EntityId,
    ) -> Result<Self, BehaviorError> {
        if micros(until) <= micros(time) {
            return Err(BehaviorError::Time);
        }
        Ok(Self {
            started: time,
            kind: 2,
            root,
            home,
            original: None,
            homeward: None,
            until,
            rejoin: rejoin(
                world
                    .schedules
                    .get(id)
                    .ok_or(BehaviorError::UnknownResident)?,
                world.schedules.epoch(),
                home,
                micros(until),
            ),
        })
    }
    /// Home to `target` now, stay until `until` (or arrival if later), then walk home.
    pub(super) fn gathering(
        world: &Behavior,
        id: EntityId,
        time: SimTime,
        until: SimTime,
        root: u64,
        home: EntityId,
        target: EntityId,
    ) -> Result<Self, BehaviorError> {
        if micros(until) <= micros(time) {
            return Err(BehaviorError::Time);
        }
        let leg =
            |from: EntityId, to: EntityId, departure: i128| -> Result<Journey, BehaviorError> {
                let walking = i128::from(
                    world
                        .navigation
                        .route(Place::Building(from), Place::Building(to))?
                        .walking_ticks(),
                ) * STEP;
                Ok(Journey {
                    from: Place::Building(from),
                    to: Place::Building(to),
                    departure,
                    arrival: departure + walking,
                })
            };
        let outbound = leg(home, target, micros(time))?;
        let homeward = leg(target, home, outbound.arrival.max(micros(until)))?;
        Ok(Self {
            started: time,
            kind: 3,
            root,
            home,
            original: Some(outbound),
            homeward: Some(homeward),
            until,
            rejoin: rejoin(
                world
                    .schedules
                    .get(id)
                    .ok_or(BehaviorError::UnknownResident)?,
                world.schedules.epoch(),
                home,
                homeward.arrival,
            ),
        })
    }
    pub(super) fn answer(self, time: SimTime) -> Answer {
        let mut previous: Option<Journey> = None;
        for journey in [self.original, self.homeward].into_iter().flatten() {
            if let Some(stay) = previous.filter(|_| micros(time) < journey.departure) {
                return Answer {
                    activity: Activity::Home,
                    location: Location::AtBuilding(match stay.to {
                        Place::Building(id) => id,
                        Place::OffMap(..) => self.home,
                    }),
                };
            }
            if micros(time) < journey.arrival {
                return journey.answer();
            }
            previous = Some(journey);
        }
        Answer {
            activity: Activity::Home,
            location: Location::AtBuilding(self.home),
        }
    }
    pub(super) fn action(self, time: SimTime) -> Action {
        if self.answer(time).activity == Activity::Commute {
            Action::Travel
        } else if matches!(self.kind, 2 | 3) && time < self.until {
            Action::Socialize
        } else {
            Action::Rest
        }
    }
    pub(super) fn next(self, after: SimTime) -> Option<SimTime> {
        [
            self.original.map(|j| j.arrival),
            self.homeward.map(|j| j.arrival),
            self.homeward
                .filter(|_| self.kind == 3)
                .map(|j| j.departure),
            Some(micros(self.until)),
            self.rejoin.map(micros),
        ]
        .into_iter()
        .flatten()
        .filter(|v| *v > micros(after))
        .filter_map(|v| i64::try_from(v).ok().map(SimTime::from_micros_since_epoch))
        .min()
    }
    pub(super) fn hash(self, h: &mut SipHasher24) {
        h.write_words(&[
            micros(self.started) as u64,
            u64::from(self.kind),
            self.root,
            self.home.raw(),
            micros(self.until) as u64,
            u64::from(self.rejoin.is_some()),
            self.rejoin.map_or(0, |t| micros(t) as u64),
        ]);
        for journey in [self.original, self.homeward] {
            h.write_u64(u64::from(journey.is_some()));
            if let Some(j) = journey {
                j.from.visit(h);
                j.to.visit(h);
                h.write_words(&[
                    j.departure as u64,
                    (j.departure >> 64) as u64,
                    j.arrival as u64,
                    (j.arrival >> 64) as u64,
                ]);
            }
        }
    }
}
