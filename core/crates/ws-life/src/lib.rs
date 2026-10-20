//! Integer real-slice schedules (ADR 0027), needs and proxy navigation (ADR 0028).
//! No clock ownership, mutable RNG, event-store reads, I/O or presentation.
use core::num::NonZeroU64;
use ws_ecs::EntityId;
pub use ws_events::Activity;
use ws_events::{Actor, Candidate, EventError, Origin, Payload, Role as ActorRole};
use ws_rng::{Purpose, SipHasher24, Stream, below};
use ws_time::SimTime;
use ws_world::slice::population::{PopulatedSlice, Role, WorkplaceLocation};

/// Integer need/utility state and conservative fine/coarse advancement.
pub mod behavior;
/// Versioned synthetic perimeter navigation; not surveyed sidewalk geometry.
pub mod navigation;

/// Frozen tables and household jitter policy.
pub const RULES_VERSION: u32 = 1;
/// Independent counter stream; draw zero, household ID address, -15..=15 minutes.
pub const JITTER: Purpose = Purpose::new("ws-life/schedule/household-jitter");
const MINUTE: i128 = 60_000_000;
const DAY: i128 = 1440 * MINUTE;

/// A stable coarse destination; off-map people remain in the population.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A registered on-block building.
    Building(EntityId),
    /// A registered off-map workplace and population exit index.
    OffMap(EntityId, u8),
}
impl Place {
    /// Building or workplace identity used by factual events.
    pub const fn id(self) -> EntityId {
        match self {
            Self::Building(id) | Self::OffMap(id, _) => id,
        }
    }
    fn visit(self, h: &mut SipHasher24) {
        match self {
            Self::Building(id) => h.write_words(&[0, id.raw()]),
            Self::OffMap(id, exit) => h.write_words(&[1, id.raw(), u64::from(exit)]),
        }
    }
}
/// Exactly one location at each queried instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// Present at a building.
    AtBuilding(EntityId),
    /// Outside the bundled block, retaining workplace and exit identity.
    OffMap(EntityId, u8),
    /// A single journey, not presence at both endpoints. Times are exact absolute
    /// microseconds in i128 so intervals intersecting SimTime's endpoints remain
    /// representable without clamping or overflowing their scheduled endpoints.
    InTransit {
        /// Origin.
        from: Place,
        /// Destination.
        to: Place,
        /// Inclusive departure.
        departure: i128,
        /// Exclusive arrival.
        arrival: i128,
    },
}
impl From<Place> for Location {
    fn from(p: Place) -> Self {
        match p {
            Place::Building(id) => Self::AtBuilding(id),
            Place::OffMap(id, e) => Self::OffMap(id, e),
        }
    }
}
/// Activity and its unique location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    /// Current activity.
    pub activity: Activity,
    /// Unique current location.
    pub location: Location,
}
impl Answer {
    /// Destination of the activity or journey, for typed schedule facts.
    pub const fn destination(self) -> EntityId {
        match self.location {
            Location::AtBuilding(id) | Location::OffMap(id, _) => id,
            Location::InTransit { to, .. } => to.id(),
        }
    }
}
/// One half-open daily interval, ending at the following row or midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Minute after model midnight, including the fixed household shift.
    pub minute: u16,
    /// Activity of this interval.
    pub activity: Activity,
    /// Destination (stationary location for non-commute rows).
    pub destination: Place,
    /// Journey origin, only for commute rows.
    pub origin: Option<Place>,
    /// Arrival minute, only for commute rows.
    pub arrival_minute: Option<u16>,
}
/// Immutable schedule profile; all resident profiles are kept in stable-ID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// Resident identity.
    pub resident: EntityId,
    /// Household identity used by the jitter stream.
    pub household: EntityId,
    /// Baseline role.
    pub role: Role,
    /// Household minute shift, -15..=15.
    pub jitter: i16,
    weekday: Vec<Segment>,
    weekend: Vec<Segment>,
}
impl Schedule {
    /// Monday-Friday table, starting at zero.
    pub fn weekday(&self) -> &[Segment] {
        &self.weekday
    }
    /// Saturday-Sunday table, starting at zero.
    pub fn weekend(&self) -> &[Segment] {
        &self.weekend
    }
    fn table(&self, day: i128) -> &[Segment] {
        if day.rem_euclid(7) < 5 {
            &self.weekday
        } else {
            &self.weekend
        }
    }
    /// Pure total evaluation, including negative times and extreme valid inputs.
    pub fn at(&self, epoch: SimTime, time: SimTime) -> Answer {
        let delta =
            i128::from(time.as_micros_since_epoch()) - i128::from(epoch.as_micros_since_epoch());
        let day = delta.div_euclid(DAY);
        let start = i128::from(epoch.as_micros_since_epoch()) + day * DAY;
        let minute_us = delta.rem_euclid(DAY);
        let table = self.table(day);
        let row = table[table.partition_point(|r| i128::from(r.minute) * MINUTE <= minute_us) - 1];
        let location = match (row.origin, row.arrival_minute) {
            (Some(from), Some(arrival)) => Location::InTransit {
                from,
                to: row.destination,
                departure: start + i128::from(row.minute) * MINUTE,
                arrival: start + i128::from(arrival) * MINUTE,
            },
            _ => row.destination.into(),
        };
        Answer {
            activity: row.activity,
            location,
        }
    }
    /// First change strictly after time. Midnight sleep continues without a fact.
    /// None means every future boundary is beyond SimTime::MAX.
    pub fn next_change(&self, epoch: SimTime, time: SimTime) -> Option<SimTime> {
        let e = i128::from(epoch.as_micros_since_epoch());
        let t = i128::from(time.as_micros_since_epoch());
        let day = (t - e).div_euclid(DAY);
        let next = self
            .table(day)
            .iter()
            .skip(1)
            .map(|r| e + day * DAY + i128::from(r.minute) * MINUTE)
            .find(|b| *b > t)
            .unwrap_or_else(|| {
                e + (day + 1) * DAY + i128::from(self.table(day + 1)[1].minute) * MINUTE
            });
        i64::try_from(next)
            .ok()
            .map(SimTime::from_micros_since_epoch)
    }
}
/// Source population no longer provides a required registered destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidPopulation;
impl core::fmt::Display for InvalidPopulation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("schedule destination is absent")
    }
}
impl std::error::Error for InvalidPopulation {}
/// All immutable schedule profiles and their explicit model-week epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedules {
    epoch: SimTime,
    profiles: Vec<Schedule>,
}
impl Schedules {
    /// Builds deterministic role/household-derived profiles from a pinned baseline.
    /// # Errors
    /// Missing local shop or a resident's required workplace.
    pub fn new(population: &PopulatedSlice, epoch: SimTime) -> Result<Self, InvalidPopulation> {
        let shop = population
            .workplaces()
            .iter()
            .find_map(|(_, w)| match w.location {
                WorkplaceLocation::Building(id) => Some(Place::Building(id)),
                _ => None,
            })
            .ok_or(InvalidPopulation)?;
        let mut profiles = Vec::with_capacity(population.residents().len());
        for (resident, r) in population.residents().iter() {
            let duty = r
                .workplace
                .map(|id| {
                    population
                        .workplaces()
                        .get(id)
                        .map(|w| match w.location {
                            WorkplaceLocation::Building(b) => Place::Building(b),
                            WorkplaceLocation::Exit(e) => Place::OffMap(id, e),
                        })
                        .ok_or(InvalidPopulation)
                })
                .transpose()?;
            if matches!(r.role, Role::Student | Role::Employed) && duty.is_none() {
                return Err(InvalidPopulation);
            }
            let seed = population.baseline().structural.seed;
            let jitter = below(
                Stream::new(seed, JITTER, &[r.household.raw()]).draw(0),
                NonZeroU64::new(31).unwrap(),
            ) as i16
                - 15;
            let home = Place::Building(r.home);
            profiles.push(Schedule {
                resident,
                household: r.household,
                role: r.role,
                jitter,
                weekday: table(r.role, false, jitter, home, duty.unwrap_or(home), shop),
                weekend: table(r.role, true, jitter, home, duty.unwrap_or(home), shop),
            });
        }
        Ok(Self { epoch, profiles })
    }
    /// Explicit model Monday, not an inferred civil time-zone label.
    pub const fn epoch(&self) -> SimTime {
        self.epoch
    }
    /// Profiles in canonical resident-ID order.
    pub fn profiles(&self) -> &[Schedule] {
        &self.profiles
    }
    /// Immutable profile lookup.
    pub fn get(&self, resident: EntityId) -> Option<&Schedule> {
        self.profiles
            .binary_search_by_key(&resident, |p| p.resident)
            .ok()
            .map(|i| &self.profiles[i])
    }
    /// Earliest boundary strictly after time, a rebuildable derived index value.
    pub fn next_change(&self, time: SimTime) -> Option<SimTime> {
        self.profiles
            .iter()
            .filter_map(|p| p.next_change(self.epoch, time))
            .min()
    }
    /// Plans sorted factual activity changes at this exact boundary without mutation.
    /// # Errors
    /// Fallible temporary capacity reservation or bounded participant construction.
    pub fn facts_at(&self, time: SimTime) -> Result<Vec<Candidate>, EventError> {
        let mut out = Vec::new();
        out.try_reserve(self.profiles.len())
            .map_err(|_| EventError::Resource)?;
        let Some(before_us) = time.as_micros_since_epoch().checked_sub(1) else {
            return Ok(out);
        };
        let before = SimTime::from_micros_since_epoch(before_us);
        for profile in &self.profiles {
            let old = profile.at(self.epoch, before);
            let new = profile.at(self.epoch, time);
            if old.activity == new.activity {
                continue;
            }
            let destination = new.destination();
            let mut fact = Candidate::new(
                time,
                60,
                out.len() as u64,
                Payload::ScheduleChanged {
                    resident: profile.resident,
                    destination,
                    previous: old.activity,
                    next: new.activity,
                },
                Origin::System(60),
            );
            fact.lod = 2;
            fact.actors.push(Actor {
                id: profile.resident,
                role: ActorRole::Subject,
            })?;
            fact.actors.push(Actor {
                id: destination,
                role: ActorRole::Object,
            })?;
            out.push(fact);
        }
        Ok(out)
    }
    /// Canonical domain hash: explicit profile inputs/tables and current answers in ID order.
    /// Capacities and the optional boundary cache are excluded.
    pub fn state_hash(&self, time: SimTime) -> u64 {
        let mut h = SipHasher24::new(0, Purpose::new("ws-life/state-hash").value());
        h.write_words(&[
            u64::from(RULES_VERSION),
            self.epoch.as_micros_since_epoch() as u64,
            self.profiles.len() as u64,
        ]);
        for p in &self.profiles {
            h.write_words(&[
                p.resident.raw(),
                p.household.raw(),
                p.role as u64,
                p.jitter as u64,
            ]);
            for table in [&p.weekday, &p.weekend] {
                h.write_u64(table.len() as u64);
                for r in table {
                    h.write_words(&[u64::from(r.minute), r.activity as u64]);
                    r.destination.visit(&mut h);
                    if let Some(from) = r.origin {
                        h.write_u64(1);
                        from.visit(&mut h);
                        h.write_u64(u64::from(r.arrival_minute.unwrap()));
                    } else {
                        h.write_u64(0);
                    }
                }
            }
            let answer = p.at(self.epoch, time);
            h.write_u64(answer.activity as u64);
            match answer.location {
                Location::AtBuilding(id) => h.write_words(&[0, id.raw()]),
                Location::OffMap(id, e) => h.write_words(&[1, id.raw(), u64::from(e)]),
                Location::InTransit {
                    from,
                    to,
                    departure,
                    arrival,
                } => {
                    h.write_u64(2);
                    from.visit(&mut h);
                    to.visit(&mut h);
                    h.write_words(&[
                        departure as u64,
                        (departure >> 64) as u64,
                        arrival as u64,
                        (arrival >> 64) as u64,
                    ]);
                }
            }
        }
        h.finish()
    }
}
fn table(
    role: Role,
    weekend: bool,
    jitter: i16,
    home: Place,
    duty: Place,
    shop: Place,
) -> Vec<Segment> {
    let shifted = |m: i16| (m + jitter) as u16;
    let at = |m, activity, destination| Segment {
        minute: if m == 0 { 0 } else { shifted(m) },
        activity,
        destination,
        origin: None,
        arrival_minute: None,
    };
    let travel = |m, end, from, to| Segment {
        minute: shifted(m),
        activity: Activity::Commute,
        destination: to,
        origin: Some(from),
        arrival_minute: Some(shifted(end)),
    };
    let mut rows = vec![at(0, Activity::Sleep, home)];
    if !weekend && matches!(role, Role::Student | Role::Employed) {
        let end = if role == Role::Student { 900 } else { 1020 };
        rows.extend([
            at(390, Activity::Home, home),
            travel(450, 480, home, duty),
            at(
                480,
                if role == Role::Student {
                    Activity::School
                } else {
                    Activity::Work
                },
                duty,
            ),
            travel(end, end + 30, duty, home),
            at(end + 30, Activity::Home, home),
            travel(1080, 1090, home, shop),
            at(1090, Activity::Errands, shop),
            travel(1120, 1130, shop, home),
            at(1130, Activity::Home, home),
        ]);
    } else {
        rows.extend([
            at(480, Activity::Home, home),
            travel(660, 670, home, shop),
            at(670, Activity::Errands, shop),
            travel(700, 710, shop, home),
            at(710, Activity::Home, home),
        ]);
    }
    rows.push(at(1320, Activity::Sleep, home));
    rows
}
