//! ADR 0038 §3 storm: a baseline departure from a featured building waits for the
//! storm's end. Override legs and journeys already started are never touched.
use super::overrides::rejoin_at;
use super::*;
use crate::{Answer, Schedule};

/// Block-level storm episode; residents are held only while `start <= t < end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StormWindow {
    /// Cast time.
    pub start: SimTime,
    /// Exclusive end; held residents are re-evaluated exactly here.
    pub end: SimTime,
}
/// Saved per-resident storm state (D1). Not an override: inspection never reports it.
///
/// While deferring, `resume` is the latest deferred baseline row (its departure is
/// before the storm's end) and `rejoin` is None. At the first re-evaluation at or
/// after the end (D3) it becomes either the post-storm journey (departing at that
/// instant, keeping the row's duration) or None for a stay, with the first
/// continuous baseline `rejoin`; None there means beyond SimTime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StormHold {
    /// Featured building the resident is held at.
    pub building: EntityId,
    /// Activity kept from before the first deferral.
    pub activity: Activity,
    /// Deferred row while deferring; post-storm journey afterwards, if any.
    pub resume: Option<Journey>,
    /// Post-storm baseline rejoin.
    pub rejoin: Option<SimTime>,
}
/// One new deferred departure from a prepare, for the `StayedInside` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deferral {
    /// Held resident.
    pub resident: EntityId,
    /// Featured building the departure would have left.
    pub building: EntityId,
    /// Kept activity.
    pub activity: Activity,
}
impl StormHold {
    /// True until D3 has been applied at the storm's end.
    fn deferring(self, end: SimTime) -> bool {
        self.resume.is_some_and(|j| j.departure < micros(end))
    }
    /// Held logical answer at `time`, before the rejoin.
    pub(super) fn answer(self, time: SimTime, end: SimTime) -> Answer {
        match self.resume.filter(|_| !self.deferring(end)) {
            Some(j) if micros(time) < j.arrival => j.answer(),
            Some(j) => Answer {
                activity: self.activity,
                location: j.to.into(),
            },
            None => Answer {
                activity: self.activity,
                location: Location::AtBuilding(self.building),
            },
        }
    }
    /// Whether a post-storm hold has reached its baseline rejoin at `time`.
    pub(super) fn rejoined(self, time: SimTime, end: SimTime) -> bool {
        !self.deferring(end) && self.rejoin.is_some_and(|r| time >= r)
    }
    /// Next exact re-evaluation strictly after `after`: the end, an arrival or the rejoin.
    pub(super) fn next(self, after: SimTime, end: SimTime) -> Option<SimTime> {
        let times = if self.deferring(end) {
            [Some(micros(end)), None]
        } else {
            [self.resume.map(|j| j.arrival), self.rejoin.map(micros)]
        };
        times
            .into_iter()
            .flatten()
            .filter(|v| *v > micros(after))
            .filter_map(|v| i64::try_from(v).ok().map(SimTime::from_micros_since_epoch))
            .min()
    }
    /// D3 at the first re-evaluation `t` at or after the storm's end.
    fn resume_after(self, profile: &Schedule, epoch: SimTime, base: Answer, t: SimTime) -> Self {
        let Some(row) = self.resume else {
            return self;
        };
        let due = matches!(base.location, Location::InTransit { from, departure, arrival, .. }
            if from == Place::Building(self.building)
                && departure == row.departure
                && arrival == row.arrival);
        if due {
            let journey = Journey {
                departure: micros(t),
                arrival: micros(t) + (row.arrival - row.departure),
                ..row
            };
            Self {
                resume: Some(journey),
                rejoin: rejoin_at(profile, epoch, row.to, journey.arrival),
                ..self
            }
        } else {
            Self {
                resume: None,
                rejoin: rejoin_at(profile, epoch, Place::Building(self.building), micros(t)),
                ..self
            }
        }
    }
    pub(super) fn hash(self, h: &mut SipHasher24) {
        h.write_words(&[
            self.building.raw(),
            self.activity as u64,
            u64::from(self.rejoin.is_some()),
            self.rejoin.map_or(0, |t| micros(t) as u64),
            u64::from(self.resume.is_some()),
        ]);
        if let Some(j) = self.resume {
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
/// Storm rule for a routine-free resident re-evaluated at `at`, whose `logical` is
/// still the previous answer. Returns the new logical answer and any new deferral.
pub(super) fn step(
    storm: StormWindow,
    featured: &[EntityId],
    profile: &Schedule,
    epoch: SimTime,
    s: &mut ResidentState,
    at: SimTime,
) -> (Answer, Option<Deferral>) {
    let base = profile.at(epoch, at);
    if let Some(h) = s.storm_hold
        && at >= storm.end
        && h.deferring(storm.end)
    {
        s.storm_hold = Some(h.resume_after(profile, epoch, base, at));
    }
    if s.storm_hold.is_some_and(|h| h.rejoined(at, storm.end)) {
        s.storm_hold = None;
    }
    let mut new = None;
    if storm.start <= at
        && at < storm.end
        && let Location::InTransit {
            from: Place::Building(b),
            to,
            departure,
            arrival,
        } = base.location
        && featured.contains(&b)
        && s.logical.location == Location::AtBuilding(b)
    {
        let activity = s.storm_hold.map_or(s.logical.activity, |h| h.activity);
        s.storm_hold = Some(StormHold {
            building: b,
            activity,
            resume: Some(Journey {
                from: Place::Building(b),
                to,
                departure,
                arrival,
            }),
            rejoin: None,
        });
        if departure > micros(at) - MINUTE {
            new = Some(Deferral {
                resident: s.id,
                building: b,
                activity,
            });
        }
    }
    let answer = s.storm_hold.map_or(base, |h| h.answer(at, storm.end));
    (answer, new)
}
