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
    /// Storm root event ID; every hold this window installs carries it.
    pub root: u64,
}
/// Explicit hold phase, independent of any (possibly replaced) storm window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HoldPhase {
    /// Held at `building`; `resume` is the latest deferred baseline row.
    Deferring = 0,
    /// Post-storm (D3): `resume` is the walk now under way, if any, then `rejoin`.
    Resuming = 1,
}
/// Saved per-resident storm state (D1 as amended). Not a routine override, but
/// inspection publishes it as override kind 4 with `root` as the override root.
/// A routine override supersedes it (`Prepared::install` clears it) and a hold is
/// only installed on a routine-free resident, so the two never coexist.
///
/// While `Deferring`, `resume` is the latest deferred baseline row and `rejoin` is
/// None. At the first re-evaluation at or after the storm's end (D3) it becomes
/// `Resuming`: `resume` is the post-storm walk departing at that instant and
/// `rejoin` the first continuous baseline instant at its destination (None means
/// beyond SimTime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StormHold {
    /// Root event ID of the storm that installed this hold.
    pub root: u64,
    /// Featured building the resident is held at.
    pub building: EntityId,
    /// Activity kept from before the first deferral.
    pub activity: Activity,
    /// Deferred row while deferring; post-storm walk afterwards, if any.
    pub resume: Option<Journey>,
    /// Post-storm baseline rejoin.
    pub rejoin: Option<SimTime>,
    /// Deferring or resuming; never derived from the window.
    pub phase: HoldPhase,
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
    /// Held logical answer at `time`, before the rejoin.
    pub(super) fn answer(self, time: SimTime) -> Answer {
        match self.resume.filter(|_| self.phase == HoldPhase::Resuming) {
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
    /// Whether a resuming hold has reached its baseline rejoin at `time`.
    pub(super) fn rejoined(self, time: SimTime) -> bool {
        self.phase == HoldPhase::Resuming && self.rejoin.is_some_and(|r| time >= r)
    }
    /// Next exact re-evaluation strictly after `after`: the end, an arrival or the rejoin.
    pub(super) fn next(self, after: SimTime, end: SimTime) -> Option<SimTime> {
        let times = match self.phase {
            HoldPhase::Deferring => [Some(micros(end)), None],
            HoldPhase::Resuming => [self.resume.map(|j| j.arrival), self.rejoin.map(micros)],
        };
        times
            .into_iter()
            .flatten()
            .filter(|v| *v > micros(after))
            .filter_map(|v| i64::try_from(v).ok().map(SimTime::from_micros_since_epoch))
            .min()
    }
    /// D3 at the first re-evaluation `t` at or after the storm's end. If the deferred
    /// row is still in its walk window, depart now keeping its duration; otherwise
    /// walk to where the baseline places the resident now. Either way, rejoin the
    /// baseline from the destination at arrival.
    fn resume_after(
        self,
        navigation: &Navigation,
        profile: &Schedule,
        epoch: SimTime,
        base: Answer,
        t: SimTime,
    ) -> Result<Self, BehaviorError> {
        let here = Place::Building(self.building);
        let row = self.resume.filter(|row| {
            matches!(base.location, Location::InTransit { from, departure, arrival, .. }
                if from == here && departure == row.departure && arrival == row.arrival)
        });
        let walk = match (row, base.location) {
            (Some(row), _) => Some(Journey {
                departure: micros(t),
                arrival: micros(t) + (row.arrival - row.departure),
                ..row
            }),
            (None, Location::AtBuilding(x)) if x == self.building => None,
            (None, location) => {
                let to = match location {
                    Location::AtBuilding(x) => Place::Building(x),
                    Location::OffMap(w, e) => Place::OffMap(w, e),
                    Location::InTransit { to, .. } => to,
                };
                // Already there (a journey into the held building): no walk.
                (to != here)
                    .then(|| -> Result<_, BehaviorError> {
                        let ticks = navigation.route(here, to)?.walking_ticks();
                        Ok(Journey {
                            from: here,
                            to,
                            departure: micros(t),
                            arrival: micros(t) + i128::from(ticks) * STEP,
                        })
                    })
                    .transpose()?
            }
        };
        let (place, at) = walk.map_or((here, micros(t)), |j| (j.to, j.arrival));
        Ok(Self {
            resume: walk,
            rejoin: rejoin_at(profile, epoch, place, at),
            phase: HoldPhase::Resuming,
            ..self
        })
    }
    pub(super) fn hash(self, h: &mut SipHasher24) {
        h.write_words(&[
            self.phase as u64,
            self.root,
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
    world: &Behavior,
    storm: StormWindow,
    profile: &Schedule,
    s: &mut ResidentState,
    at: SimTime,
) -> Result<(Answer, Option<Deferral>), BehaviorError> {
    let epoch = world.schedules.epoch();
    let base = profile.at(epoch, at);
    if let Some(h) = s.storm_hold
        && h.phase == HoldPhase::Deferring
        && at >= storm.end
    {
        s.storm_hold = Some(h.resume_after(&world.navigation, profile, epoch, base, at)?);
    }
    if s.storm_hold.is_some_and(|h| h.rejoined(at)) {
        s.storm_hold = None;
    }
    let mut new = None;
    // Only someone standing at the featured building is held; a resident on a
    // post-storm walk is in transit and continues, even under a later storm.
    if storm.start <= at
        && at < storm.end
        && let Location::InTransit {
            from: Place::Building(b),
            to,
            departure,
            arrival,
        } = base.location
        && world.featured.contains(&b)
        && s.logical.location == Location::AtBuilding(b)
    {
        let activity = s.storm_hold.map_or(s.logical.activity, |h| h.activity);
        s.storm_hold = Some(StormHold {
            root: storm.root,
            building: b,
            activity,
            resume: Some(Journey {
                from: Place::Building(b),
                to,
                departure,
                arrival,
            }),
            rejoin: None,
            phase: HoldPhase::Deferring,
        });
        if departure > micros(at) - MINUTE {
            new = Some(Deferral {
                resident: s.id,
                building: b,
                activity,
            });
        }
    }
    let answer = s.storm_hold.map_or(base, |h| h.answer(at));
    Ok((answer, new))
}
