//! The canonical record codec (ADR 0020).
//!
//! Little-endian, fixed-width fields, no padding and no platform-dependent
//! sizes; `f32` as its raw bits. Each record kind is written and read by one
//! function pair, and every enum is a one-byte tag that the decoder checks.
//! A tag or value the decoder does not know is an error, never a guess.

use core::num::NonZeroU32;

use ws_ecs::EntityId;
use ws_events::{
    Actor, Candidate, Cause, Confidence, Event, EventId, Location, Origin as EventOrigin, Payload,
    Related, Relation, Role, Small, Target,
};
use ws_math::{CellId, CoordError, SurfacePos};
use ws_sim::commands::{
    Action, Command, Effect, Intervention, InterventionTarget, IssuedBy, Outcome, Rejection, Timing,
};
use ws_sim::{Clock, LodError, LodViolation, SimTime, TimeOverflow, TimeScale};
use ws_time::{Rate, SimDuration};
use ws_world::{
    Activity, Background, Building, Lod, Origin, Quantity, Resident, ResidentAddress,
    ScheduleProblem, Walk,
};
use ws_world::{Illness, IllnessKind};

/// Why bytes are not a valid record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecError(pub &'static str);

type Result<T> = core::result::Result<T, CodecError>;

pub(crate) struct Writer(pub Vec<u8>);

impl Writer {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn id(&mut self, v: EntityId) {
        self.u64(v.raw());
    }
    pub fn time(&mut self, v: SimTime) {
        self.i64(v.as_micros_since_epoch());
    }
    pub fn pos(&mut self, p: SurfacePos) {
        self.u64(p.cell().raw());
        for c in p.offset() {
            self.f32(c);
        }
    }
}

pub(crate) struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(CodecError("truncated"))?;
        self.0 = rest;
        Ok(*head)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    /// A boolean stored as exactly 0 or 1.
    pub fn flag(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CodecError("flag is not 0 or 1")),
        }
    }
    pub fn id(&mut self) -> Result<EntityId> {
        EntityId::from_raw(self.u64()?).map_err(|_| CodecError("invalid entity ID"))
    }
    pub fn cell(&mut self) -> Result<CellId> {
        CellId::from_raw(self.u64()?).map_err(|_| CodecError("invalid cell ID"))
    }
    pub fn time(&mut self) -> Result<SimTime> {
        Ok(SimTime::from_micros_since_epoch(self.i64()?))
    }
    pub fn pos(&mut self) -> Result<SurfacePos> {
        let cell = self.cell()?;
        let offset = [self.f32()?, self.f32()?, self.f32()?];
        SurfacePos::new(cell, offset).map_err(|_| CodecError("invalid surface position"))
    }
    /// Fails unless every byte was consumed.
    pub fn finish(self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(CodecError("trailing bytes"))
        }
    }
}

fn lod_from(v: u8) -> Result<Lod> {
    Ok(match v {
        0 => Lod::L0,
        1 => Lod::L1,
        2 => Lod::L2,
        3 => Lod::L3,
        4 => Lod::L4,
        5 => Lod::L5,
        6 => Lod::L6,
        7 => Lod::L7,
        _ => return Err(CodecError("unknown level")),
    })
}

// ---- buildings -----------------------------------------------------------

pub(crate) fn put_building(b: &Building, level: Lod) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(b.index);
    w.pos(b.pos);
    w.f32(b.half_extent_m);
    w.f32(b.height_m);
    w.u8(level.as_u8());
    w.0
}

pub(crate) fn get_building(bytes: &[u8]) -> Result<(Building, Lod)> {
    let mut r = Reader::new(bytes);
    let b = Building {
        index: r.u32()?,
        pos: r.pos()?,
        half_extent_m: r.f32()?,
        height_m: r.f32()?,
    };
    let level = lod_from(r.u8()?)?;
    r.finish()?;
    Ok((b, level))
}

// ---- residents -----------------------------------------------------------

pub(crate) fn put_resident(r: &Resident) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(r.address.building_index);
    w.u32(r.address.slot);
    w.id(r.home);
    w.0
}

pub(crate) fn get_resident(bytes: &[u8]) -> Result<Resident> {
    let mut r = Reader::new(bytes);
    let resident = Resident {
        address: ResidentAddress {
            building_index: r.u32()?,
            slot: r.u32()?,
        },
        home: r.id()?,
    };
    r.finish()?;
    Ok(resident)
}

// ---- L1 and L2 state -----------------------------------------------------

pub(crate) fn put_l1(pos: SurfacePos, walk: &Walk) -> Vec<u8> {
    let mut w = Writer::new();
    w.pos(pos);
    for c in walk.velocity {
        w.f32(c);
    }
    w.id(walk.target);
    w.u8(u8::from(walk.arrived));
    w.u32(walk.wait_steps);
    w.u32(walk.targets_drawn);
    w.0
}

pub(crate) fn get_l1(bytes: &[u8]) -> Result<(SurfacePos, Walk)> {
    let mut r = Reader::new(bytes);
    let pos = r.pos()?;
    let walk = Walk {
        velocity: [r.f32()?, r.f32()?, r.f32()?],
        target: r.id()?,
        arrived: r.flag()?,
        wait_steps: r.u32()?,
        targets_drawn: r.u32()?,
    };
    r.finish()?;
    Ok((pos, walk))
}

pub(crate) fn put_l2(bg: &Background) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(bg.targets_drawn);
    match bg.activity {
        Activity::Waiting { at, depart } => {
            w.u8(0);
            w.id(at);
            w.time(depart);
        }
        Activity::Walking {
            origin,
            origin_time,
            dest,
            arrive,
        } => {
            w.u8(1);
            match origin {
                Origin::Building(b) => {
                    w.u8(0);
                    w.id(b);
                }
                Origin::Point { cell, offset } => {
                    w.u8(1);
                    w.u64(cell.raw());
                    for c in offset {
                        w.f32(c);
                    }
                }
            }
            w.time(origin_time);
            w.id(dest);
            w.time(arrive);
        }
    }
    w.0
}

pub(crate) fn get_l2(bytes: &[u8]) -> Result<Background> {
    let mut r = Reader::new(bytes);
    let targets_drawn = r.u32()?;
    let activity = match r.u8()? {
        0 => Activity::Waiting {
            at: r.id()?,
            depart: r.time()?,
        },
        1 => {
            let origin = match r.u8()? {
                0 => Origin::Building(r.id()?),
                1 => Origin::Point {
                    cell: r.cell()?,
                    offset: [r.f32()?, r.f32()?, r.f32()?],
                },
                _ => return Err(CodecError("unknown origin tag")),
            };
            Activity::Walking {
                origin,
                origin_time: r.time()?,
                dest: r.id()?,
                arrive: r.time()?,
            }
        }
        _ => return Err(CodecError("unknown activity tag")),
    };
    r.finish()?;
    Ok(Background {
        activity,
        targets_drawn,
    })
}

// ---- clock and counters --------------------------------------------------

/// The clock and the world's two counters.
pub(crate) struct ClockRow {
    pub clock: Clock,
    pub next_sequence: u64,
    pub boundary: u64,
}

pub(crate) fn put_clock(c: &Clock, next_sequence: u64, boundary: u64) -> Vec<u8> {
    let mut w = Writer::new();
    w.time(c.start());
    w.i64(c.base_step().step().as_micros());
    w.u64(c.steps());
    w.u32(c.scale().multiplier());
    w.u32(c.max_steps_per_call().get());
    w.u64(c.pending_micros());
    w.u64(next_sequence);
    w.u64(boundary);
    w.0
}

pub(crate) fn get_clock(bytes: &[u8]) -> Result<ClockRow> {
    let mut r = Reader::new(bytes);
    let start = r.time()?;
    let base_step = Rate::new(SimDuration::from_micros(r.i64()?))
        .map_err(|_| CodecError("base step is not positive"))?;
    let steps = r.u64()?;
    let scale = match r.u32()? {
        0 => TimeScale::Paused,
        1 => TimeScale::X1,
        10 => TimeScale::X10,
        100 => TimeScale::X100,
        _ => return Err(CodecError("unknown time scale")),
    };
    let max_steps = NonZeroU32::new(r.u32()?).ok_or(CodecError("zero step cap"))?;
    let pending_us = r.u64()?;
    let next_sequence = r.u64()?;
    let boundary = r.u64()?;
    r.finish()?;
    let clock = Clock::from_parts(start, base_step, steps, scale, max_steps, pending_us)
        .map_err(|_| CodecError("inconsistent clock"))?;
    Ok(ClockRow {
        clock,
        next_sequence,
        boundary,
    })
}

// ---- events --------------------------------------------------------------

fn put_payload(w: &mut Writer, p: Payload) {
    match p {
        Payload::WorldCreated {
            seed,
            dataset,
            generator,
            rules,
            min,
            max,
        } => {
            w.u8(0);
            w.u64(seed);
            w.u64(dataset);
            w.u32(generator);
            w.u32(rules);
            w.u32(min);
            w.u32(max);
        }
        Payload::CommandApplied {
            sequence,
            kind,
            effect,
            demoted,
            promoted,
        } => {
            w.u8(1);
            w.u64(sequence);
            w.u32(kind);
            w.u32(effect);
            w.u32(demoted);
            w.u32(promoted);
        }
        Payload::LodChanged {
            building,
            from,
            to,
            demoted,
            promoted,
        } => {
            w.u8(2);
            w.id(building);
            w.u8(from);
            w.u8(to);
            w.u32(demoted);
            w.u32(promoted);
        }
        Payload::Arrived {
            resident,
            destination,
            leg,
        } => {
            w.u8(3);
            w.id(resident);
            w.id(destination);
            w.u32(leg);
        }
        Payload::InterventionApplied {
            sequence,
            effect,
            illness,
            severity,
            affected,
        } => {
            w.u8(4);
            w.u64(sequence);
            w.u8(effect);
            w.u8(illness);
            w.u8(severity);
            w.u32(affected);
        }
        Payload::LifeIllness {
            resident,
            illness,
            severity,
        } => {
            w.u8(5);
            w.id(resident);
            w.u8(illness);
            w.u8(severity);
        }
        Payload::ReturnedHome { resident, home } => {
            w.u8(6);
            w.id(resident);
            w.id(home);
        }
    }
}

fn get_payload(r: &mut Reader) -> Result<Payload> {
    Ok(match r.u8()? {
        0 => Payload::WorldCreated {
            seed: r.u64()?,
            dataset: r.u64()?,
            generator: r.u32()?,
            rules: r.u32()?,
            min: r.u32()?,
            max: r.u32()?,
        },
        1 => Payload::CommandApplied {
            sequence: r.u64()?,
            kind: r.u32()?,
            effect: r.u32()?,
            demoted: r.u32()?,
            promoted: r.u32()?,
        },
        2 => Payload::LodChanged {
            building: r.id()?,
            from: r.u8()?,
            to: r.u8()?,
            demoted: r.u32()?,
            promoted: r.u32()?,
        },
        3 => Payload::Arrived {
            resident: r.id()?,
            destination: r.id()?,
            leg: r.u32()?,
        },
        4 => Payload::InterventionApplied {
            sequence: r.u64()?,
            effect: r.u8()?,
            illness: r.u8()?,
            severity: r.u8()?,
            affected: r.u32()?,
        },
        5 => Payload::LifeIllness {
            resident: r.id()?,
            illness: r.u8()?,
            severity: r.u8()?,
        },
        6 => Payload::ReturnedHome {
            resident: r.id()?,
            home: r.id()?,
        },
        _ => return Err(CodecError("unknown payload tag")),
    })
}

/// The links of one event, stored in `event_links` rather than in the
/// event row, so history queries can range-scan them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Link {
    /// 0 for a cause, 1 for a non-causal related link.
    pub kind: u8,
    pub target: EventId,
    /// The relation for a cause; the related-kind tag for a related link.
    pub relation: u8,
    /// The confidence for a cause; zero for a related link.
    pub confidence: u8,
}

/// An event row without its links.
pub(crate) fn put_event(e: &Event) -> Vec<u8> {
    let rec = &e.record;
    let mut w = Writer::new();
    w.u32(e.schema_version);
    w.u64(e.id.0);
    w.u64(e.tick);
    w.u64(e.ordinal);
    w.time(rec.time);
    w.u16(rec.system);
    w.u64(rec.sequence);
    match rec.origin {
        EventOrigin::System(n) => {
            w.u8(0);
            w.u64(u64::from(n));
        }
        EventOrigin::Command(n) => {
            w.u8(1);
            w.u64(n);
        }
    }
    put_payload(&mut w, rec.payload);
    w.u8(rec.actors.len() as u8);
    for a in rec.actors.iter() {
        w.id(a.id);
        w.u8(a.role as u8);
    }
    match rec.location {
        None => w.u8(0),
        Some(l) => {
            w.u8(1);
            w.u64(l.cell.raw());
            match l.precise {
                None => w.u8(0),
                Some(p) => {
                    w.u8(1);
                    w.pos(p);
                }
            }
            w.u8(l.path.len() as u8);
            for id in l.path.iter() {
                w.id(*id);
            }
        }
    }
    w.u8(rec.lod);
    w.u16(e.importance);
    w.u8(u8::from(e.aggregate));
    w.0
}

/// The links of a committed event, or `None` if a cause is still a local
/// candidate reference (which a committed event never has).
pub(crate) fn event_links(e: &Event) -> Option<Vec<Link>> {
    let mut links = Vec::new();
    for c in e.record.causes.iter() {
        let Target::Id(id) = c.target else {
            return None;
        };
        links.push(Link {
            kind: 0,
            target: id,
            relation: c.relation as u8,
            confidence: c.confidence as u8,
        });
    }
    for r in e.record.related.iter() {
        links.push(Link {
            kind: 1,
            target: r.id,
            relation: r.kind,
            confidence: 0,
        });
    }
    Some(links)
}

pub(crate) fn get_event(bytes: &[u8], links: &[Link]) -> Result<Event> {
    let mut r = Reader::new(bytes);
    let schema_version = r.u32()?;
    let id = EventId(r.u64()?);
    let tick = r.u64()?;
    let ordinal = r.u64()?;
    let time = r.time()?;
    let system = r.u16()?;
    let sequence = r.u64()?;
    let origin = match r.u8()? {
        0 => EventOrigin::System(
            u16::try_from(r.u64()?).map_err(|_| CodecError("system origin out of range"))?,
        ),
        1 => EventOrigin::Command(r.u64()?),
        _ => return Err(CodecError("unknown event origin tag")),
    };
    let payload = get_payload(&mut r)?;
    let mut actors: Small<Actor, 8> = Small::default();
    for _ in 0..r.u8()? {
        let id = r.id()?;
        let role = match r.u8()? {
            0 => Role::Subject,
            1 => Role::Object,
            _ => return Err(CodecError("unknown role")),
        };
        actors
            .push(Actor { id, role })
            .map_err(|_| CodecError("too many actors"))?;
    }
    let location = match r.u8()? {
        0 => None,
        1 => {
            let cell = r.cell()?;
            let precise = match r.u8()? {
                0 => None,
                1 => Some(r.pos()?),
                _ => return Err(CodecError("bad location flag")),
            };
            let mut path: Small<EntityId, 8> = Small::default();
            for _ in 0..r.u8()? {
                path.push(r.id()?)
                    .map_err(|_| CodecError("path too long"))?;
            }
            Some(Location {
                cell,
                precise,
                path,
            })
        }
        _ => return Err(CodecError("bad location flag")),
    };
    let lod = r.u8()?;
    let importance = r.u16()?;
    let aggregate = r.flag()?;
    r.finish()?;
    let mut causes: Small<Cause, 8> = Small::default();
    let mut related: Small<Related, 8> = Small::default();
    for l in links {
        match l.kind {
            0 => {
                let relation = match l.relation {
                    0 => Relation::CausedBy,
                    1 => Relation::EnabledBy,
                    2 => Relation::TriggeredBy,
                    3 => Relation::ContributedTo,
                    _ => return Err(CodecError("unknown relation")),
                };
                let confidence = match l.confidence {
                    0 => Confidence::Direct,
                    1 => Confidence::Contributing,
                    2 => Confidence::Statistical,
                    3 => Confidence::Counterfactual,
                    _ => return Err(CodecError("unknown confidence")),
                };
                causes
                    .push(Cause {
                        target: Target::Id(l.target),
                        relation,
                        confidence,
                    })
                    .map_err(|_| CodecError("too many causes"))?;
            }
            1 => related
                .push(Related {
                    id: l.target,
                    kind: l.relation,
                })
                .map_err(|_| CodecError("too many related links"))?,
            _ => return Err(CodecError("unknown link kind")),
        }
    }
    Ok(Event {
        schema_version,
        id,
        tick,
        ordinal,
        record: Candidate {
            time,
            system,
            sequence,
            payload,
            origin,
            actors,
            location,
            causes,
            related,
            lod,
        },
        importance,
        aggregate,
    })
}

// ---- journal -------------------------------------------------------------

fn put_coord(w: &mut Writer, e: CoordError) {
    w.u8(match e {
        CoordError::NonFinite => 0,
        CoordError::LatitudeOutOfRange => 1,
        CoordError::LongitudeOutOfRange => 2,
        CoordError::NearEarthCentre => 3,
        CoordError::InvalidCellId => 4,
        CoordError::InvalidLevel => 5,
        CoordError::NotSectorCell => 6,
        CoordError::FrameMismatch => 7,
        CoordError::BoundaryUnresolved => 8,
    });
}

fn get_coord(r: &mut Reader) -> Result<CoordError> {
    Ok(match r.u8()? {
        0 => CoordError::NonFinite,
        1 => CoordError::LatitudeOutOfRange,
        2 => CoordError::LongitudeOutOfRange,
        3 => CoordError::NearEarthCentre,
        4 => CoordError::InvalidCellId,
        5 => CoordError::InvalidLevel,
        6 => CoordError::NotSectorCell,
        7 => CoordError::FrameMismatch,
        8 => CoordError::BoundaryUnresolved,
        _ => return Err(CodecError("unknown coordinate error")),
    })
}

fn put_problem(w: &mut Writer, p: ScheduleProblem) {
    w.u8(match p {
        ScheduleProblem::OffGrid => 0,
        ScheduleProblem::EventNotInFuture => 1,
        ScheduleProblem::OriginInFuture => 2,
        ScheduleProblem::EmptyLeg => 3,
        ScheduleProblem::WaitTooLong => 4,
        ScheduleProblem::WalkingWithoutDraw => 5,
    });
}

fn get_problem(r: &mut Reader) -> Result<ScheduleProblem> {
    Ok(match r.u8()? {
        0 => ScheduleProblem::OffGrid,
        1 => ScheduleProblem::EventNotInFuture,
        2 => ScheduleProblem::OriginInFuture,
        3 => ScheduleProblem::EmptyLeg,
        4 => ScheduleProblem::WaitTooLong,
        5 => ScheduleProblem::WalkingWithoutDraw,
        _ => return Err(CodecError("unknown schedule problem")),
    })
}

fn put_violation(w: &mut Writer, v: LodViolation) {
    use LodViolation as V;
    match v {
        V::Duplicated(id) => {
            w.u8(0);
            w.id(id);
        }
        V::Lost(id) => {
            w.u8(1);
            w.id(id);
        }
        V::Orphan(id) => {
            w.u8(2);
            w.id(id);
        }
        V::PartialL1(id) => {
            w.u8(3);
            w.id(id);
        }
        V::IdentityMismatch(id) => {
            w.u8(4);
            w.id(id);
        }
        V::IdentityChanged(id) => {
            w.u8(5);
            w.id(id);
        }
        V::UnknownBuilding { id, building } => {
            w.u8(6);
            w.id(id);
            w.id(building);
        }
        V::MissingNodeLod(id) => {
            w.u8(7);
            w.id(id);
        }
        V::InvalidNodeLod(id) => {
            w.u8(8);
            w.id(id);
        }
        V::ResidencyMismatch {
            id,
            governing,
            level,
        } => {
            w.u8(9);
            w.id(id);
            w.id(governing);
            w.u8(level.as_u8());
        }
        V::QueueMismatch(id) => {
            w.u8(10);
            w.id(id);
        }
        V::UnreconciledMismatch(id) => {
            w.u8(11);
            w.id(id);
        }
        V::BadSchedule { id, problem } => {
            w.u8(12);
            w.id(id);
            put_problem(w, problem);
        }
        V::NotAtCentre(id) => {
            w.u8(13);
            w.id(id);
        }
        V::NotInOwnCell(id) => {
            w.u8(14);
            w.id(id);
        }
        V::CountMismatch {
            quantity,
            before,
            after,
        } => {
            w.u8(15);
            match quantity {
                Quantity::Population => w.u8(0),
                Quantity::Home(id) => {
                    w.u8(1);
                    w.id(id);
                }
                Quantity::Governing(id) => {
                    w.u8(2);
                    w.id(id);
                }
            }
            w.u64(before);
            w.u64(after);
        }
        V::CounterChanged(id) => {
            w.u8(16);
            w.id(id);
        }
    }
}

fn get_violation(r: &mut Reader) -> Result<LodViolation> {
    use LodViolation as V;
    Ok(match r.u8()? {
        0 => V::Duplicated(r.id()?),
        1 => V::Lost(r.id()?),
        2 => V::Orphan(r.id()?),
        3 => V::PartialL1(r.id()?),
        4 => V::IdentityMismatch(r.id()?),
        5 => V::IdentityChanged(r.id()?),
        6 => V::UnknownBuilding {
            id: r.id()?,
            building: r.id()?,
        },
        7 => V::MissingNodeLod(r.id()?),
        8 => V::InvalidNodeLod(r.id()?),
        9 => V::ResidencyMismatch {
            id: r.id()?,
            governing: r.id()?,
            level: lod_from(r.u8()?)?,
        },
        10 => V::QueueMismatch(r.id()?),
        11 => V::UnreconciledMismatch(r.id()?),
        12 => V::BadSchedule {
            id: r.id()?,
            problem: get_problem(r)?,
        },
        13 => V::NotAtCentre(r.id()?),
        14 => V::NotInOwnCell(r.id()?),
        15 => {
            let quantity = match r.u8()? {
                0 => Quantity::Population,
                1 => Quantity::Home(r.id()?),
                2 => Quantity::Governing(r.id()?),
                _ => return Err(CodecError("unknown quantity")),
            };
            V::CountMismatch {
                quantity,
                before: r.u64()?,
                after: r.u64()?,
            }
        }
        16 => V::CounterChanged(r.id()?),
        _ => return Err(CodecError("unknown violation tag")),
    })
}

fn put_lod_error(w: &mut Writer, e: LodError) {
    match e {
        LodError::UnknownNode(id) => {
            w.u8(0);
            w.id(id);
        }
        LodError::DuplicateNode(id) => {
            w.u8(1);
            w.id(id);
        }
        LodError::UnsupportedLevel(l) => {
            w.u8(2);
            w.u8(l.as_u8());
        }
        LodError::OffGrid(t) => {
            w.u8(3);
            w.time(t);
        }
        LodError::Coord(c) => {
            w.u8(4);
            put_coord(w, c);
        }
        LodError::Time(TimeOverflow) => w.u8(5),
        LodError::CounterExhausted(id) => {
            w.u8(6);
            w.id(id);
        }
        LodError::Invariant(v) => {
            w.u8(7);
            put_violation(w, v);
        }
    }
}

fn get_lod_error(r: &mut Reader) -> Result<LodError> {
    Ok(match r.u8()? {
        0 => LodError::UnknownNode(r.id()?),
        1 => LodError::DuplicateNode(r.id()?),
        2 => LodError::UnsupportedLevel(lod_from(r.u8()?)?),
        3 => LodError::OffGrid(r.time()?),
        4 => LodError::Coord(get_coord(r)?),
        5 => LodError::Time(TimeOverflow),
        6 => LodError::CounterExhausted(r.id()?),
        7 => LodError::Invariant(get_violation(r)?),
        _ => return Err(CodecError("unknown level error tag")),
    })
}

pub(crate) fn put_outcome(o: &Outcome) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(o.schema_version);
    w.u64(o.sequence);
    w.time(o.time);
    w.u32(o.effect);
    w.u32(o.demoted);
    w.u32(o.promoted);
    w.u64(o.tick);
    w.u64(o.boundary);
    w.u32(o.command.schema_version);
    w.u32(o.command.issued_by as u32);
    match &o.command.action {
        Action::NoOp => w.u8(0),
        Action::SetTimeScale(s) => {
            w.u8(1);
            w.u32(s.multiplier());
        }
        Action::SetNodeLod(v) => {
            w.u8(2);
            w.u32(v.len() as u32);
            for (id, level) in v {
                w.id(*id);
                w.u8(level.as_u8());
            }
        }
        Action::Intervene(i) => {
            w.u8(3);
            put_intervention(&mut w, i);
        }
    }
    match o.result {
        Ok(()) => w.u8(0),
        Err(Rejection::Schema) => w.u8(1),
        Err(Rejection::TooManyNodes) => w.u8(2),
        Err(Rejection::Lod(e)) => {
            w.u8(3);
            put_lod_error(&mut w, e);
        }
        Err(Rejection::UnknownTarget(id)) => {
            w.u8(4);
            w.id(id);
        }
        Err(Rejection::UnsupportedLevel(id)) => {
            w.u8(5);
            w.id(id);
        }
        Err(Rejection::SeverityOutOfRange(s)) => {
            w.u8(6);
            w.u8(s);
        }
    }
    w.0
}

fn put_intervention(w: &mut Writer, i: &Intervention) {
    match i.target {
        InterventionTarget::Entity(id) => {
            w.u8(0);
            w.id(id);
        }
        InterventionTarget::ResidentsOf(id) => {
            w.u8(1);
            w.id(id);
        }
    }
    let Effect::Afflict { illness, severity } = i.effect;
    w.u8(0);
    w.u8(illness.tag());
    w.u8(severity);
    match i.timing {
        Timing::Now => w.u8(0),
        Timing::At(t) => {
            w.u8(1);
            w.time(t);
        }
    }
}

fn get_intervention(r: &mut Reader) -> Result<Intervention> {
    let target = match r.u8()? {
        0 => InterventionTarget::Entity(r.id()?),
        1 => InterventionTarget::ResidentsOf(r.id()?),
        _ => return Err(CodecError("unknown intervention target")),
    };
    if r.u8()? != 0 {
        return Err(CodecError("unknown effect tag"));
    }
    let illness = IllnessKind::from_tag(r.u8()?).ok_or(CodecError("unknown illness kind"))?;
    let severity = r.u8()?;
    let timing = match r.u8()? {
        0 => Timing::Now,
        1 => Timing::At(r.time()?),
        _ => return Err(CodecError("unknown timing")),
    };
    Ok(Intervention {
        target,
        effect: Effect::Afflict { illness, severity },
        timing,
    })
}

pub(crate) fn put_illness(i: &Illness) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(i.kind.tag());
    w.u8(i.severity);
    w.u64(i.root_event);
    w.0
}

pub(crate) fn get_illness(bytes: &[u8]) -> Result<Illness> {
    let mut r = Reader::new(bytes);
    let kind = IllnessKind::from_tag(r.u8()?).ok_or(CodecError("unknown illness kind"))?;
    let severity = r.u8()?;
    let root_event = r.u64()?;
    r.finish()?;
    Ok(Illness {
        kind,
        severity,
        root_event,
    })
}

pub(crate) fn put_scheduled(i: &Intervention) -> Vec<u8> {
    let mut w = Writer::new();
    put_intervention(&mut w, i);
    w.0
}

pub(crate) fn get_scheduled(bytes: &[u8]) -> Result<Intervention> {
    let mut r = Reader::new(bytes);
    let i = get_intervention(&mut r)?;
    r.finish()?;
    Ok(i)
}

pub(crate) fn get_outcome(bytes: &[u8]) -> Result<Outcome> {
    let mut r = Reader::new(bytes);
    let schema_version = r.u32()?;
    let sequence = r.u64()?;
    let time = r.time()?;
    let effect = r.u32()?;
    let demoted = r.u32()?;
    let promoted = r.u32()?;
    let tick = r.u64()?;
    let boundary = r.u64()?;
    let command_schema = r.u32()?;
    let issued_by = match r.u32()? {
        0 => IssuedBy::Player,
        1 => IssuedBy::Script,
        2 => IssuedBy::Test,
        _ => return Err(CodecError("unknown issuer")),
    };
    let action = match r.u8()? {
        0 => Action::NoOp,
        1 => Action::SetTimeScale(match r.u32()? {
            0 => TimeScale::Paused,
            1 => TimeScale::X1,
            10 => TimeScale::X10,
            100 => TimeScale::X100,
            _ => return Err(CodecError("unknown time scale")),
        }),
        2 => {
            let n = r.u32()? as usize;
            // The longest valid request has 1,024 entries; refuse to
            // allocate for a count a corrupt row could inflate.
            if n > 1024 {
                return Err(CodecError("too many nodes in a command"));
            }
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push((r.id()?, lod_from(r.u8()?)?));
            }
            Action::SetNodeLod(v)
        }
        3 => Action::Intervene(get_intervention(&mut r)?),
        _ => return Err(CodecError("unknown action tag")),
    };
    let result = match r.u8()? {
        0 => Ok(()),
        1 => Err(Rejection::Schema),
        2 => Err(Rejection::TooManyNodes),
        3 => Err(Rejection::Lod(get_lod_error(&mut r)?)),
        4 => Err(Rejection::UnknownTarget(r.id()?)),
        5 => Err(Rejection::UnsupportedLevel(r.id()?)),
        6 => Err(Rejection::SeverityOutOfRange(r.u8()?)),
        _ => return Err(CodecError("unknown result tag")),
    };
    r.finish()?;
    Ok(Outcome {
        schema_version,
        sequence,
        time,
        effect,
        demoted,
        promoted,
        tick,
        boundary,
        command: Command {
            schema_version: command_schema,
            issued_by,
            action,
        },
        result,
    })
}

#[cfg(test)]
mod tests {
    use ws_math::SECTOR_LEVEL;
    use ws_math::reference::BAY_RIDGE;

    use super::*;

    fn id(n: u64) -> EntityId {
        EntityId::procedural(n)
    }

    fn outcome(result: core::result::Result<(), Rejection>, action: Action) -> Outcome {
        Outcome {
            schema_version: 1,
            sequence: 9,
            time: SimTime::from_micros_since_epoch(-123_456),
            effect: 3,
            demoted: 4,
            promoted: 5,
            tick: 77,
            boundary: 2,
            command: Command {
                schema_version: 1,
                issued_by: IssuedBy::Script,
                action,
            },
            result,
        }
    }

    #[test]
    fn every_rejection_round_trips() {
        let a = id(1);
        let violations = [
            LodViolation::Duplicated(a),
            LodViolation::Lost(a),
            LodViolation::Orphan(a),
            LodViolation::PartialL1(a),
            LodViolation::IdentityMismatch(a),
            LodViolation::IdentityChanged(a),
            LodViolation::UnknownBuilding {
                id: a,
                building: id(2),
            },
            LodViolation::MissingNodeLod(a),
            LodViolation::InvalidNodeLod(a),
            LodViolation::ResidencyMismatch {
                id: a,
                governing: id(2),
                level: Lod::L2,
            },
            LodViolation::QueueMismatch(a),
            LodViolation::UnreconciledMismatch(a),
            LodViolation::BadSchedule {
                id: a,
                problem: ScheduleProblem::EmptyLeg,
            },
            LodViolation::NotAtCentre(a),
            LodViolation::NotInOwnCell(a),
            LodViolation::CountMismatch {
                quantity: Quantity::Population,
                before: 1,
                after: 2,
            },
            LodViolation::CountMismatch {
                quantity: Quantity::Home(a),
                before: 3,
                after: 4,
            },
            LodViolation::CountMismatch {
                quantity: Quantity::Governing(a),
                before: 5,
                after: 6,
            },
            LodViolation::CounterChanged(a),
        ];
        let problems = [
            ScheduleProblem::OffGrid,
            ScheduleProblem::EventNotInFuture,
            ScheduleProblem::OriginInFuture,
            ScheduleProblem::EmptyLeg,
            ScheduleProblem::WaitTooLong,
            ScheduleProblem::WalkingWithoutDraw,
        ];
        let coords = [
            CoordError::NonFinite,
            CoordError::LatitudeOutOfRange,
            CoordError::LongitudeOutOfRange,
            CoordError::NearEarthCentre,
            CoordError::InvalidCellId,
            CoordError::InvalidLevel,
            CoordError::NotSectorCell,
            CoordError::FrameMismatch,
            CoordError::BoundaryUnresolved,
        ];
        let mut errors = vec![
            LodError::UnknownNode(a),
            LodError::DuplicateNode(a),
            LodError::UnsupportedLevel(Lod::L5),
            LodError::OffGrid(SimTime::from_micros_since_epoch(7)),
            LodError::Time(TimeOverflow),
            LodError::CounterExhausted(a),
        ];
        errors.extend(coords.map(LodError::Coord));
        errors.extend(violations.map(LodError::Invariant));
        errors.extend(
            problems
                .map(|problem| LodError::Invariant(LodViolation::BadSchedule { id: a, problem })),
        );
        let mut results = vec![
            Ok(()),
            Err(Rejection::Schema),
            Err(Rejection::TooManyNodes),
            Err(Rejection::UnknownTarget(a)),
            Err(Rejection::UnsupportedLevel(a)),
            Err(Rejection::SeverityOutOfRange(0)),
            Err(Rejection::SeverityOutOfRange(255)),
        ];
        results.extend(errors.into_iter().map(|e| Err(Rejection::Lod(e))));
        for result in results {
            let o = outcome(result, Action::NoOp);
            assert_eq!(get_outcome(&put_outcome(&o)), Ok(o));
        }
    }

    fn intervene(target: InterventionTarget, timing: Timing) -> Action {
        Action::Intervene(Intervention {
            target,
            effect: Effect::Afflict {
                illness: IllnessKind::Fever,
                severity: 10,
            },
            timing,
        })
    }

    #[test]
    fn illness_scheduled_and_event_payloads_round_trip() {
        let ill = Illness {
            kind: IllnessKind::Fever,
            severity: 3,
            root_event: 77,
        };
        assert_eq!(get_illness(&put_illness(&ill)), Ok(ill));
        let mut bad = put_illness(&ill);
        bad[0] = 9;
        assert!(get_illness(&bad).is_err());
        let Action::Intervene(i) = intervene(
            InterventionTarget::Entity(id(1)),
            Timing::At(SimTime::EPOCH),
        ) else {
            unreachable!()
        };
        assert_eq!(get_scheduled(&put_scheduled(&i)), Ok(i));
        for p in [
            Payload::InterventionApplied {
                sequence: 4,
                effect: 0,
                illness: 0,
                severity: 5,
                affected: 12,
            },
            Payload::LifeIllness {
                resident: id(1),
                illness: 0,
                severity: 5,
            },
            Payload::ReturnedHome {
                resident: id(1),
                home: id(2),
            },
        ] {
            let mut w = Writer::new();
            put_payload(&mut w, p);
            assert_eq!(get_payload(&mut Reader::new(&w.0)), Ok(p));
        }
    }

    #[test]
    fn every_action_round_trips() {
        for action in [
            Action::NoOp,
            Action::SetTimeScale(TimeScale::Paused),
            Action::SetTimeScale(TimeScale::X1),
            Action::SetTimeScale(TimeScale::X10),
            Action::SetTimeScale(TimeScale::X100),
            Action::SetNodeLod(vec![]),
            Action::SetNodeLod(vec![(id(1), Lod::L1), (id(2), Lod::L2), (id(3), Lod::L7)]),
            intervene(InterventionTarget::Entity(id(4)), Timing::Now),
            intervene(
                InterventionTarget::ResidentsOf(id(5)),
                Timing::At(SimTime::from_micros_since_epoch(-9)),
            ),
        ] {
            let o = outcome(Ok(()), action);
            assert_eq!(get_outcome(&put_outcome(&o)), Ok(o));
        }
    }

    #[test]
    fn trailing_bytes_truncation_and_unknown_tags_are_errors() {
        let bytes = put_outcome(&outcome(Ok(()), Action::NoOp));
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(get_outcome(&longer), Err(CodecError("trailing bytes")));
        for cut in 0..bytes.len() {
            assert!(get_outcome(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut bad_tag = bytes;
        let last = bad_tag.len() - 1;
        bad_tag[last] = 9; // the result tag
        assert_eq!(get_outcome(&bad_tag), Err(CodecError("unknown result tag")));
    }

    #[test]
    fn an_inflated_node_count_is_refused_before_allocating() {
        let mut bytes = put_outcome(&outcome(Ok(()), Action::SetNodeLod(vec![])));
        // The count follows the fixed 56-byte prefix and the action tag.
        let at = 4 + 8 + 8 + 4 + 4 + 4 + 8 + 8 + 4 + 4 + 1;
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            get_outcome(&bytes),
            Err(CodecError("too many nodes in a command"))
        );
    }

    #[test]
    fn flags_must_be_exactly_zero_or_one() {
        let walk = Walk {
            velocity: [0.0; 3],
            target: id(1),
            arrived: true,
            wait_steps: 1,
            targets_drawn: 2,
        };
        let cell = CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap();
        let pos = SurfacePos::new(cell, [1.0, 2.0, 3.0]).unwrap();
        let mut bytes = put_l1(pos, &walk);
        assert_eq!(get_l1(&bytes), Ok((pos, walk)));
        let flag = 8 + 12 + 12 + 8;
        bytes[flag] = 2;
        assert_eq!(get_l1(&bytes), Err(CodecError("flag is not 0 or 1")));
    }
}
