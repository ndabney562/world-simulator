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
    Activity, Background, BaselineAddress, Building, Lod, Origin, Quantity, Resident,
    ResidentAddress, ScheduleProblem, Tombstone, Walk,
};
use ws_world::{Illness, IllnessKind};

use crate::header::BaselineRecord;

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
    pub fn i128(&mut self, v: i128) {
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
    pub fn i128(&mut self) -> Result<i128> {
        Ok(i128::from_le_bytes(self.take()?))
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

#[cfg(test)]
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

// ---- format 3 baseline, overrides, notables, tombstones, spawned ---------

/// Component tag for `Building` in `structural_overrides` (ADR 0025 §2).
pub const COMPONENT_BUILDING: u8 = 1;
/// Component tag for `Resident` in `structural_overrides` (ADR 0025 §2).
pub const COMPONENT_RESIDENT: u8 = 2;

/// Complete canonical entity record stored in `notables` or `spawned`
/// (ADR 0025 §2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EntityRecord {
    Building(Building),
    Resident(Resident),
}

pub(crate) fn put_building_component(b: &Building) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(b.index);
    w.pos(b.pos);
    w.f32(b.half_extent_m);
    w.f32(b.height_m);
    w.0
}

pub(crate) fn get_building_component(bytes: &[u8]) -> Result<Building> {
    let mut r = Reader::new(bytes);
    let b = Building {
        index: r.u32()?,
        pos: r.pos()?,
        half_extent_m: r.f32()?,
        height_m: r.f32()?,
    };
    r.finish()?;
    Ok(b)
}

pub(crate) fn put_node_lod(level: Lod) -> Vec<u8> {
    vec![level.as_u8()]
}

pub(crate) fn get_node_lod(bytes: &[u8]) -> Result<Lod> {
    let mut r = Reader::new(bytes);
    let level = lod_from(r.u8()?)?;
    r.finish()?;
    Ok(level)
}

pub(crate) fn put_entity_record(rec: &EntityRecord) -> Vec<u8> {
    let mut w = Writer::new();
    match rec {
        EntityRecord::Building(b) => {
            w.u8(1);
            w.u32(b.index);
            w.pos(b.pos);
            w.f32(b.half_extent_m);
            w.f32(b.height_m);
        }
        EntityRecord::Resident(r) => {
            w.u8(2);
            w.u32(r.address.building_index);
            w.u32(r.address.slot);
            w.id(r.home);
        }
    }
    w.0
}

pub(crate) fn get_entity_record(bytes: &[u8]) -> Result<EntityRecord> {
    let mut r = Reader::new(bytes);
    let rec = match r.u8()? {
        1 => EntityRecord::Building(Building {
            index: r.u32()?,
            pos: r.pos()?,
            half_extent_m: r.f32()?,
            height_m: r.f32()?,
        }),
        2 => EntityRecord::Resident(Resident {
            address: ResidentAddress {
                building_index: r.u32()?,
                slot: r.u32()?,
            },
            home: r.id()?,
        }),
        _ => return Err(CodecError("unknown entity record tag")),
    };
    r.finish()?;
    Ok(rec)
}

pub(crate) fn put_tombstone(t: &Tombstone) -> Vec<u8> {
    let mut w = Writer::new();
    match t.address {
        BaselineAddress::Building(index) => {
            w.u8(1);
            w.u32(index);
        }
        BaselineAddress::Resident(addr) => {
            w.u8(2);
            w.u32(addr.building_index);
            w.u32(addr.slot);
        }
    }
    w.u64(t.deleted_tick);
    w.u64(t.root_event);
    w.0
}

pub(crate) fn get_tombstone(bytes: &[u8]) -> Result<Tombstone> {
    let mut r = Reader::new(bytes);
    let address = match r.u8()? {
        1 => BaselineAddress::Building(r.u32()?),
        2 => BaselineAddress::Resident(ResidentAddress {
            building_index: r.u32()?,
            slot: r.u32()?,
        }),
        _ => return Err(CodecError("unknown tombstone address tag")),
    };
    let deleted_tick = r.u64()?;
    let root_event = r.u64()?;
    r.finish()?;
    Ok(Tombstone {
        address,
        deleted_tick,
        root_event,
    })
}

pub(crate) fn put_baseline(b: &BaselineRecord) -> Vec<u8> {
    let mut w = Writer::new();
    w.u64(b.seed);
    w.u32(b.dataset_id);
    w.u64(b.dataset_version);
    w.0.extend_from_slice(&b.dataset_sha256);
    w.u32(b.building_generator_version);
    w.u32(b.resident_generator_version);
    w.u32(b.rules_version);
    w.u32(b.identity_policy_version);
    w.u32(b.population_min);
    w.u32(b.population_max);
    w.u64(b.walk_speed_bits);
    w.u32(b.walk_wait_steps);
    w.i64(b.start_time_us);
    w.u32(b.building_count);
    w.u32(b.resident_count);
    w.u64(b.structural_digest);
    w.u64(b.baseline_digest);
    w.u32(u32::try_from(b.collision_overrides.len()).expect("collision_overrides fits in u32"));
    w.0.extend_from_slice(&b.collision_overrides);
    w.0
}

pub(crate) fn get_baseline(bytes: &[u8]) -> Result<BaselineRecord> {
    let mut r = Reader::new(bytes);
    let seed = r.u64()?;
    let dataset_id = r.u32()?;
    let dataset_version = r.u64()?;
    let dataset_sha256 = r.take::<32>()?;
    let building_generator_version = r.u32()?;
    let resident_generator_version = r.u32()?;
    let rules_version = r.u32()?;
    let identity_policy_version = r.u32()?;
    let population_min = r.u32()?;
    let population_max = r.u32()?;
    let walk_speed_bits = r.u64()?;
    let walk_wait_steps = r.u32()?;
    let start_time_us = r.i64()?;
    let building_count = r.u32()?;
    let resident_count = r.u32()?;
    let structural_digest = r.u64()?;
    let baseline_digest = r.u64()?;
    let override_len = r.u32()? as usize;
    if override_len > 65_536 || override_len > r.0.len() {
        return Err(CodecError("invalid collision override length"));
    }
    let collision_overrides = r.0[..override_len].to_vec();
    r.0 = &r.0[override_len..];
    r.finish()?;
    Ok(BaselineRecord {
        seed,
        dataset_id,
        dataset_version,
        dataset_sha256,
        building_generator_version,
        resident_generator_version,
        rules_version,
        identity_policy_version,
        population_min,
        population_max,
        walk_speed_bits,
        walk_wait_steps,
        start_time_us,
        building_count,
        resident_count,
        structural_digest,
        baseline_digest,
        collision_overrides,
    })
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

/// The clock and counters in format 3 (`clock` schema 2, ADR 0025 §2–3).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ClockRowV3 {
    pub clock: Clock,
    pub next_sequence: u64,
    pub boundary: u64,
    pub next_spawned_id: u64,
    pub next_event_id: u64,
}

pub(crate) fn put_clock_v3(
    c: &Clock,
    next_sequence: u64,
    boundary: u64,
    next_spawned_id: u64,
    next_event_id: u64,
) -> Vec<u8> {
    let mut w = Writer(put_clock(c, next_sequence, boundary));
    w.u64(next_spawned_id);
    w.u64(next_event_id);
    w.0
}

pub(crate) fn get_clock_v3(bytes: &[u8]) -> Result<ClockRowV3> {
    if bytes.len() != 72 {
        return Err(CodecError(if bytes.len() < 72 {
            "truncated"
        } else {
            "trailing bytes"
        }));
    }
    let base = get_clock(&bytes[..56])?;
    let mut r = Reader::new(&bytes[56..]);
    let next_spawned_id = r.u64()?;
    let next_event_id = r.u64()?;
    r.finish()?;
    Ok(ClockRowV3 {
        clock: base.clock,
        next_sequence: base.next_sequence,
        boundary: base.boundary,
        next_spawned_id,
        next_event_id,
    })
}

// ---- events --------------------------------------------------------------

fn put_payload(w: &mut Writer, p: Payload) -> Result<()> {
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
        Payload::SliceCreated {
            seed,
            dataset,
            structural,
            population,
            schedules,
            schedule_epoch,
        } => {
            w.u8(7);
            w.u64(seed);
            w.u64(dataset);
            w.u32(structural);
            w.u32(population);
            w.u32(schedules);
            w.i64(schedule_epoch);
        }
        Payload::SliceLodChanged {
            building,
            from,
            to,
            demoted,
            promoted,
        } => {
            w.u8(8);
            w.id(building);
            w.u8(from);
            w.u8(to);
            w.u32(demoted);
            w.u32(promoted);
        }
        Payload::ScheduleChanged {
            resident,
            destination,
            previous,
            next,
        } => {
            w.u8(9);
            w.id(resident);
            w.id(destination);
            w.u8(previous as u8);
            w.u8(next as u8);
        }
        Payload::SliceIllnessApplied {
            resident,
            sequence,
            severity,
            duration_us,
            affected,
        } => {
            w.u8(10);
            w.id(resident);
            w.u64(sequence);
            w.u32(severity);
            w.u64(duration_us);
            w.u32(affected);
        }
        Payload::MissedDuty {
            resident,
            destination,
            activity,
        } => {
            w.u8(11);
            w.id(resident);
            w.id(destination);
            w.u8(activity as u8);
        }
        Payload::NeighbourCheckIn {
            visitor,
            patient,
            home,
            intended_duration_us,
        } => {
            w.u8(12);
            w.id(visitor);
            w.id(patient);
            w.id(home);
            w.u64(intended_duration_us);
        }
        Payload::Recovered { resident, health } => {
            w.u8(13);
            w.id(resident);
            w.u32(health);
        }
        Payload::BlessApplied {
            resident,
            sequence,
            affected,
        } => {
            w.u8(14);
            w.id(resident);
            w.u64(sequence);
            w.u32(affected);
        }
        Payload::NeedsRelieved { resident } => {
            w.u8(15);
            w.id(resident);
        }
        Payload::StormApplied {
            sequence,
            duration_us,
        } => {
            w.u8(16);
            w.u64(sequence);
            w.u64(duration_us);
        }
        Payload::StayedInside {
            resident,
            building,
            activity,
        } => {
            w.u8(17);
            w.id(resident);
            w.id(building);
            w.u8(activity as u8);
        }
        Payload::StormKeptInside { count } => {
            w.u8(18);
            w.u32(count);
        }
        Payload::StormEnded { sequence } => {
            w.u8(19);
            w.u64(sequence);
        }
        Payload::GatheringCalled {
            building,
            sequence,
            duration_us,
            attendees,
        } => {
            w.u8(20);
            w.id(building);
            w.u64(sequence);
            w.u64(duration_us);
            w.u32(attendees);
        }
        Payload::JoinedGathering { resident, building } => {
            w.u8(21);
            w.id(resident);
            w.id(building);
        }
        Payload::GatheringEnded {
            building,
            attendees,
        } => {
            w.u8(22);
            w.id(building);
            w.u32(attendees);
        }
        // Tag 23 is unsupported until save format 6.
        Payload::GoalCompleted { .. } => return Err(CodecError("unknown payload tag")),
    }
    Ok(())
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
        7 => Payload::SliceCreated {
            seed: r.u64()?,
            dataset: r.u64()?,
            structural: r.u32()?,
            population: r.u32()?,
            schedules: r.u32()?,
            schedule_epoch: r.i64()?,
        },
        8 => Payload::SliceLodChanged {
            building: r.id()?,
            from: r.u8()?,
            to: r.u8()?,
            demoted: r.u32()?,
            promoted: r.u32()?,
        },
        9 => Payload::ScheduleChanged {
            resident: r.id()?,
            destination: r.id()?,
            previous: ws_events::Activity::from_tag(r.u8()?)
                .ok_or(CodecError("invalid activity tag"))?,
            next: ws_events::Activity::from_tag(r.u8()?)
                .ok_or(CodecError("invalid activity tag"))?,
        },
        10 => Payload::SliceIllnessApplied {
            resident: r.id()?,
            sequence: r.u64()?,
            severity: r.u32()?,
            duration_us: r.u64()?,
            affected: r.u32()?,
        },
        11 => Payload::MissedDuty {
            resident: r.id()?,
            destination: r.id()?,
            activity: ws_events::Activity::from_tag(r.u8()?)
                .ok_or(CodecError("invalid activity tag"))?,
        },
        12 => Payload::NeighbourCheckIn {
            visitor: r.id()?,
            patient: r.id()?,
            home: r.id()?,
            intended_duration_us: r.u64()?,
        },
        13 => Payload::Recovered {
            resident: r.id()?,
            health: r.u32()?,
        },
        14 => Payload::BlessApplied {
            resident: r.id()?,
            sequence: r.u64()?,
            affected: r.u32()?,
        },
        15 => Payload::NeedsRelieved { resident: r.id()? },
        16 => Payload::StormApplied {
            sequence: r.u64()?,
            duration_us: r.u64()?,
        },
        17 => Payload::StayedInside {
            resident: r.id()?,
            building: r.id()?,
            activity: ws_events::Activity::from_tag(r.u8()?)
                .ok_or(CodecError("invalid activity tag"))?,
        },
        18 => Payload::StormKeptInside { count: r.u32()? },
        19 => Payload::StormEnded { sequence: r.u64()? },
        20 => Payload::GatheringCalled {
            building: r.id()?,
            sequence: r.u64()?,
            duration_us: r.u64()?,
            attendees: r.u32()?,
        },
        21 => Payload::JoinedGathering {
            resident: r.id()?,
            building: r.id()?,
        },
        22 => Payload::GatheringEnded {
            building: r.id()?,
            attendees: r.u32()?,
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
pub(crate) fn put_event(e: &Event) -> Result<Vec<u8>> {
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
    put_payload(&mut w, rec.payload)?;
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
    Ok(w.0)
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

// ---- slice codecs (ADR 0036) ---------------------------------------------

pub(crate) fn put_slice_config(c: &ws_sim::slice::SliceConfig) -> Vec<u8> {
    let mut w = Writer::new();
    w.u64(c.seed);
    w.time(c.start);
    w.time(c.schedule_epoch);
    w.0
}

pub(crate) fn get_slice_config(bytes: &[u8]) -> Result<ws_sim::slice::SliceConfig> {
    let mut r = Reader::new(bytes);
    let seed = r.u64()?;
    let start = r.time()?;
    let schedule_epoch = r.time()?;
    r.finish()?;
    Ok(ws_sim::slice::SliceConfig {
        seed,
        start,
        schedule_epoch,
    })
}

pub(crate) fn put_slice_vital(v: &ws_world::slice::vitals::VitalState) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(v.sex() as u8);
    w.u8(v.health());
    w.u8(v.life() as u8);
    w.0
}

pub(crate) fn get_slice_vital(bytes: &[u8]) -> Result<ws_world::slice::vitals::VitalState> {
    let mut r = Reader::new(bytes);
    let sex =
        ws_world::slice::vitals::Sex::from_tag(r.u8()?).ok_or(CodecError("invalid sex tag"))?;
    let health = r.u8()?;
    let life =
        ws_world::slice::vitals::Life::from_tag(r.u8()?).ok_or(CodecError("invalid life tag"))?;
    r.finish()?;
    ws_world::slice::vitals::VitalState::checked(sex, health, life)
        .ok_or(CodecError("invalid vital state"))
}

pub(crate) fn put_slice_episode(e: &ws_sim::slice::illness::Episode) -> Vec<u8> {
    let mut w = Writer::new();
    w.id(e.resident);
    w.u64(e.root.0);
    w.u64(e.illness.0);
    w.u32(e.severity);
    w.time(e.start);
    w.time(e.end);
    match e.return_at {
        Some(t) => {
            w.u8(1);
            w.i128(t);
        }
        None => w.u8(0),
    }
    w.u8(u8::from(e.recovered));
    w.u8(u8::from(e.returned));
    w.u8(e.missed);
    w.u8(u8::from(e.visited));
    w.0
}

pub(crate) fn get_slice_episode(bytes: &[u8]) -> Result<ws_sim::slice::illness::Episode> {
    let mut r = Reader::new(bytes);
    let resident = r.id()?;
    let root = ws_events::EventId(r.u64()?);
    let illness = ws_events::EventId(r.u64()?);
    let severity = r.u32()?;
    let start = r.time()?;
    let end = r.time()?;
    let return_at = match r.u8()? {
        0 => None,
        1 => Some(r.i128()?),
        _ => return Err(CodecError("invalid return_at flag")),
    };
    let recovered = r.flag()?;
    let returned = r.flag()?;
    let missed = r.u8()?;
    let visited = r.flag()?;
    r.finish()?;
    Ok(ws_sim::slice::illness::Episode {
        resident,
        root,
        illness,
        severity,
        start,
        end,
        return_at,
        recovered,
        returned,
        missed,
        visited,
    })
}

fn put_place(w: &mut Writer, p: ws_life::Place) {
    match p {
        ws_life::Place::Building(id) => {
            w.u8(0);
            w.id(id);
        }
        ws_life::Place::OffMap(id, exit) => {
            w.u8(1);
            w.id(id);
            w.u8(exit);
        }
    }
}

fn get_place(r: &mut Reader) -> Result<ws_life::Place> {
    match r.u8()? {
        0 => Ok(ws_life::Place::Building(r.id()?)),
        1 => Ok(ws_life::Place::OffMap(r.id()?, r.u8()?)),
        _ => Err(CodecError("invalid place tag")),
    }
}

fn put_journey(w: &mut Writer, j: ws_life::behavior::Journey) {
    put_place(w, j.from);
    put_place(w, j.to);
    w.i128(j.departure);
    w.i128(j.arrival);
}

fn get_journey(r: &mut Reader) -> Result<ws_life::behavior::Journey> {
    let from = get_place(r)?;
    let to = get_place(r)?;
    let departure = r.i128()?;
    let arrival = r.i128()?;
    Ok(ws_life::behavior::Journey {
        from,
        to,
        departure,
        arrival,
    })
}

fn put_routine(w: &mut Writer, r: ws_life::behavior::RoutineOverride) {
    w.u32(r.kind);
    w.u64(r.root);
    w.id(r.home);
    match r.original {
        Some(j) => {
            w.u8(1);
            put_journey(w, j);
        }
        None => w.u8(0),
    }
    match r.homeward {
        Some(j) => {
            w.u8(1);
            put_journey(w, j);
        }
        None => w.u8(0),
    }
    w.time(r.started);
    w.time(r.until);
    match r.rejoin {
        Some(t) => {
            w.u8(1);
            w.time(t);
        }
        None => w.u8(0),
    }
}

/// Override kinds 1 (illness) and 2 (visit) in format 4; format 5 adds 3
/// (gathering, ADR 0038 §6).
fn get_routine(r: &mut Reader, format: u32) -> Result<ws_life::behavior::RoutineOverride> {
    let kind = r.u32()?;
    let last = if format >= 5 { 3 } else { 2 };
    if !(1..=last).contains(&kind) {
        return Err(CodecError("invalid routine kind"));
    }
    let root = r.u64()?;
    let home = r.id()?;
    let original = match r.u8()? {
        0 => None,
        1 => Some(get_journey(r)?),
        _ => return Err(CodecError("invalid original journey flag")),
    };
    let homeward = match r.u8()? {
        0 => None,
        1 => Some(get_journey(r)?),
        _ => return Err(CodecError("invalid homeward journey flag")),
    };
    let started = r.time()?;
    let until = r.time()?;
    let rejoin = match r.u8()? {
        0 => None,
        1 => Some(r.time()?),
        _ => return Err(CodecError("invalid rejoin flag")),
    };
    Ok(ws_life::behavior::RoutineOverride {
        kind,
        root,
        home,
        original,
        homeward,
        started,
        until,
        rejoin,
    })
}

fn put_opt_journey(w: &mut Writer, j: Option<ws_life::behavior::Journey>) {
    match j {
        Some(j) => {
            w.u8(1);
            put_journey(w, j);
        }
        None => w.u8(0),
    }
}

fn get_opt_journey(r: &mut Reader) -> Result<Option<ws_life::behavior::Journey>> {
    Ok(if r.flag()? {
        Some(get_journey(r)?)
    } else {
        None
    })
}

fn put_opt_time(w: &mut Writer, t: Option<SimTime>) {
    match t {
        Some(t) => {
            w.u8(1);
            w.time(t);
        }
        None => w.u8(0),
    }
}

fn get_opt_time(r: &mut Reader) -> Result<Option<SimTime>> {
    Ok(if r.flag()? { Some(r.time()?) } else { None })
}

/// The format-5 storm hold block (plan decision D1): every field, with the
/// phase stored explicitly.
fn put_storm_hold(w: &mut Writer, h: ws_life::behavior::StormHold) {
    w.u64(h.root);
    w.id(h.building);
    w.u8(h.activity as u8);
    put_opt_journey(w, h.resume);
    put_opt_time(w, h.rejoin);
    w.u8(h.phase as u8);
}

fn get_storm_hold(r: &mut Reader) -> Result<ws_life::behavior::StormHold> {
    let root = r.u64()?;
    if root == 0 {
        return Err(CodecError("storm hold without a root"));
    }
    let building = r.id()?;
    let activity =
        ws_events::Activity::from_tag(r.u8()?).ok_or(CodecError("invalid activity tag"))?;
    let resume = get_opt_journey(r)?;
    let rejoin = get_opt_time(r)?;
    let phase = match r.u8()? {
        0 => ws_life::behavior::HoldPhase::Deferring,
        1 => ws_life::behavior::HoldPhase::Resuming,
        _ => return Err(CodecError("invalid storm hold phase")),
    };
    Ok(ws_life::behavior::StormHold {
        root,
        building,
        activity,
        resume,
        rejoin,
        phase,
    })
}

/// A `slice_behavior` row. Format 5 appends the storm hold block; format 4
/// has none, and its writer refuses a resident with a hold.
pub(crate) fn put_slice_resident_state(
    st: &ws_life::behavior::ResidentStateSnapshot,
    format: u32,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.id(st.id);
    w.u8(st.detail as u8);
    for v in st.needs.values() {
        w.u32(v);
    }
    w.time(st.anchor);
    w.u8(st.action as u8);
    w.u64(st.decisions);
    match st.routine {
        Some(r) => {
            w.u8(1);
            put_routine(&mut w, r);
        }
        None => w.u8(0),
    }
    if format >= 5 {
        match st.storm_hold {
            Some(h) => {
                w.u8(1);
                put_storm_hold(&mut w, h);
            }
            None => w.u8(0),
        }
    }
    w.0
}

pub(crate) fn get_slice_resident_state(
    bytes: &[u8],
    format: u32,
) -> Result<ws_life::behavior::ResidentStateSnapshot> {
    let mut r = Reader::new(bytes);
    let id = r.id()?;
    let detail =
        ws_life::behavior::Detail::from_tag(r.u8()?).ok_or(CodecError("invalid detail tag"))?;
    let needs = ws_life::behavior::Needs::checked([r.u32()?, r.u32()?, r.u32()?, r.u32()?])
        .ok_or(CodecError("needs exceed MAX_DEFICIT"))?;
    let anchor = r.time()?;
    let action =
        ws_life::behavior::Action::from_tag(r.u8()?).ok_or(CodecError("invalid action tag"))?;
    let decisions = r.u64()?;
    let routine = match r.u8()? {
        0 => None,
        1 => Some(get_routine(&mut r, format)?),
        _ => return Err(CodecError("invalid routine flag")),
    };
    let storm_hold = if format >= 5 && r.flag()? {
        Some(get_storm_hold(&mut r)?)
    } else {
        None
    };
    r.finish()?;
    Ok(ws_life::behavior::ResidentStateSnapshot {
        id,
        detail,
        needs,
        anchor,
        action,
        decisions,
        routine,
        storm_hold,
    })
}

/// The format-5 `slice_storm` row: the storm episode and the `ws-life`
/// window, each optional (the window outlives the episode until every hold
/// has rejoined).
pub(crate) fn put_slice_storm(
    storm: Option<ws_sim::slice::Storm>,
    window: Option<ws_life::behavior::StormWindow>,
) -> Vec<u8> {
    let mut w = Writer::new();
    match storm {
        Some(s) => {
            w.u8(1);
            w.u64(s.root.0);
            w.time(s.start);
            w.time(s.end);
            w.u32(s.kept);
        }
        None => w.u8(0),
    }
    match window {
        Some(x) => {
            w.u8(1);
            w.time(x.start);
            w.time(x.end);
            w.u64(x.root);
        }
        None => w.u8(0),
    }
    w.0
}

pub(crate) type DecodedStorm = (
    Option<ws_sim::slice::Storm>,
    Option<ws_life::behavior::StormWindow>,
);

pub(crate) fn get_slice_storm(bytes: &[u8]) -> Result<DecodedStorm> {
    let mut r = Reader::new(bytes);
    let storm = if r.flag()? {
        Some(ws_sim::slice::Storm {
            root: EventId(r.u64()?),
            start: r.time()?,
            end: r.time()?,
            kept: r.u32()?,
        })
    } else {
        None
    };
    let window = if r.flag()? {
        Some(ws_life::behavior::StormWindow {
            start: r.time()?,
            end: r.time()?,
            root: r.u64()?,
        })
    } else {
        None
    };
    r.finish()?;
    if storm.is_none() && window.is_none() {
        return Err(CodecError("empty storm row"));
    }
    Ok((storm, window))
}

/// The format-5 `slice_gathering` row: the episode, its `joined` count, then
/// up to seven attendees of resident ID and phase.
pub(crate) fn put_slice_gathering(g: &ws_sim::slice::Gathering) -> Vec<u8> {
    let mut w = Writer::new();
    w.id(g.building);
    w.u64(g.root.0);
    w.time(g.start);
    w.time(g.end);
    w.u32(g.joined);
    w.u8(g.attendees().len() as u8);
    for (id, phase) in g.attendees() {
        w.id(*id);
        w.u8(*phase as u8);
    }
    w.0
}

pub(crate) fn get_slice_gathering(bytes: &[u8]) -> Result<ws_sim::slice::Gathering> {
    let mut r = Reader::new(bytes);
    let building = r.id()?;
    let root = EventId(r.u64()?);
    let start = r.time()?;
    let end = r.time()?;
    let joined = r.u32()?;
    let count = usize::from(r.u8()?);
    if count > ws_sim::slice::MAX_ATTENDEES {
        return Err(CodecError("too many attendees"));
    }
    let mut attendees = [(building, ws_sim::slice::Phase::Outbound); ws_sim::slice::MAX_ATTENDEES];
    for slot in &mut attendees[..count] {
        let id = r.id()?;
        let phase =
            ws_sim::slice::Phase::from_tag(r.u8()?).ok_or(CodecError("invalid attendee phase"))?;
        *slot = (id, phase);
    }
    r.finish()?;
    ws_sim::slice::Gathering::restore(building, root, start, end, joined, &attendees[..count])
        .ok_or(CodecError("too many attendees"))
}

pub(crate) fn put_slice_lod(
    camera: Option<ws_life::lod::CameraInterest>,
    nodes: &[(EntityId, ws_life::lod::Mode, SimTime)],
) -> Vec<u8> {
    let mut w = Writer::new();
    match camera {
        Some(c) => {
            w.u8(1);
            w.i64(c.center.east);
            w.i64(c.center.north);
            w.u64(c.radius_mm);
        }
        None => w.u8(0),
    }
    w.u32(nodes.len() as u32);
    for &(id, mode, changed_at) in nodes {
        w.id(id);
        w.u8(mode as u8);
        w.time(changed_at);
    }
    w.0
}

pub(crate) type DecodedLod = (
    Option<ws_life::lod::CameraInterest>,
    Vec<(EntityId, ws_life::lod::Mode, SimTime)>,
);

pub(crate) fn get_slice_lod(bytes: &[u8]) -> Result<DecodedLod> {
    let mut r = Reader::new(bytes);
    let camera = match r.u8()? {
        0 => None,
        1 => {
            let east = r.i64()?;
            let north = r.i64()?;
            let radius_mm = r.u64()?;
            Some(ws_life::lod::CameraInterest {
                center: ws_life::navigation::Point { east, north },
                radius_mm,
            })
        }
        _ => return Err(CodecError("invalid camera flag")),
    };
    let count = r.u32()? as usize;
    if count > 16 {
        return Err(CodecError("too many nodes"));
    }
    let mut nodes = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.id()?;
        let mode = ws_life::lod::Mode::from_tag(r.u8()?).ok_or(CodecError("invalid mode tag"))?;
        let changed_at = r.time()?;
        nodes.push((id, mode, changed_at));
    }
    r.finish()?;
    Ok((camera, nodes))
}

pub(crate) fn put_operation_record(rec: &ws_sim::slice::OperationRecord) -> Vec<u8> {
    let mut w = Writer::new();
    match rec.operation {
        ws_sim::slice::Operation::Steps(n) => {
            w.u8(0);
            w.u64(n);
        }
        ws_sim::slice::Operation::SetScale(scale) => {
            w.u8(1);
            w.u32(scale.multiplier());
        }
        ws_sim::slice::Operation::HostElapsed(us) => {
            w.u8(2);
            w.u64(us);
        }
        ws_sim::slice::Operation::SetDetail { resident, level } => {
            w.u8(3);
            w.id(resident);
            w.u8(level as u8);
        }
        ws_sim::slice::Operation::CameraInterest(c) => {
            w.u8(4);
            w.i64(c.center.east);
            w.i64(c.center.north);
            w.u64(c.radius_mm);
        }
        ws_sim::slice::Operation::Afflict {
            resident,
            severity,
            duration_us,
        } => {
            w.u8(5);
            w.id(resident);
            w.u32(severity);
            w.u64(duration_us);
        }
        ws_sim::slice::Operation::Bless { resident } => {
            w.u8(6);
            w.id(resident);
        }
        ws_sim::slice::Operation::Storm { duration_us } => {
            w.u8(7);
            w.u64(duration_us);
        }
        ws_sim::slice::Operation::Gather {
            building,
            duration_us,
        } => {
            w.u8(8);
            w.id(building);
            w.u64(duration_us);
        }
    }
    w.u64(rec.report.committed);
    match rec.report.error {
        Some(e) => {
            w.u8(1);
            w.u64(error_tag_sim(e));
        }
        None => w.u8(0),
    }
    match rec.stop {
        Some(s) => {
            w.u8(1);
            w.u64(s.tick);
            w.u64(s.state);
            w.u64(s.events);
            w.u64(s.pending_micros);
        }
        None => w.u8(0),
    }
    w.0
}

fn error_tag_sim(error: ws_sim::slice::SliceError) -> u64 {
    match error {
        ws_sim::slice::SliceError::Rejected(e) => 100 + e as u64,
        ws_sim::slice::SliceError::Population(_) => 1,
        ws_sim::slice::SliceError::Destination(_) => 2,
        ws_sim::slice::SliceError::Grid => 3,
        ws_sim::slice::SliceError::TimeOverflow => 4,
        ws_sim::slice::SliceError::Resource => 5,
        ws_sim::slice::SliceError::Lod(e) => match e {
            ws_life::lod::LodError::Input => 40,
            ws_life::lod::LodError::CameraOwned => 41,
            ws_life::lod::LodError::Population => 42,
            ws_life::lod::LodError::Resource => 43,
            ws_life::lod::LodError::Invariant => 44,
            ws_life::lod::LodError::Behavior(b) => error_tag_behavior(b),
        },
        ws_sim::slice::SliceError::Behavior(b) => error_tag_behavior(b),
        ws_sim::slice::SliceError::History(_) => 6,
    }
}

fn error_tag_behavior(b: ws_life::behavior::BehaviorError) -> u64 {
    20 + match b {
        ws_life::behavior::BehaviorError::Navigation(_) => 0,
        ws_life::behavior::BehaviorError::InfeasibleRoute => 1,
        ws_life::behavior::BehaviorError::Grid => 2,
        ws_life::behavior::BehaviorError::Time => 3,
        ws_life::behavior::BehaviorError::Counter => 4,
        ws_life::behavior::BehaviorError::UnknownResident => 5,
        ws_life::behavior::BehaviorError::Resource => 6,
    }
}

fn error_from_tag(tag: u64) -> Result<ws_sim::slice::SliceError> {
    if (100..=106).contains(&tag) {
        let r = match tag - 100 {
            0 => ws_sim::slice::illness::Rejection::Target,
            1 => ws_sim::slice::illness::Rejection::Dead,
            2 => ws_sim::slice::illness::Rejection::Severity,
            3 => ws_sim::slice::illness::Rejection::Duration,
            4 => ws_sim::slice::illness::Rejection::Deadline,
            5 => ws_sim::slice::illness::Rejection::Building,
            6 => ws_sim::slice::illness::Rejection::Busy,
            _ => return Err(CodecError("invalid rejection tag")),
        };
        return Ok(ws_sim::slice::SliceError::Rejected(r));
    }
    match tag {
        3 => Ok(ws_sim::slice::SliceError::Grid),
        4 => Ok(ws_sim::slice::SliceError::TimeOverflow),
        5 => Ok(ws_sim::slice::SliceError::Resource),
        6 => Ok(ws_sim::slice::SliceError::History(
            ws_events::EventError::Resource,
        )),
        40 => Ok(ws_sim::slice::SliceError::Lod(
            ws_life::lod::LodError::Input,
        )),
        41 => Ok(ws_sim::slice::SliceError::Lod(
            ws_life::lod::LodError::CameraOwned,
        )),
        42 => Ok(ws_sim::slice::SliceError::Lod(
            ws_life::lod::LodError::Population,
        )),
        43 => Ok(ws_sim::slice::SliceError::Lod(
            ws_life::lod::LodError::Resource,
        )),
        44 => Ok(ws_sim::slice::SliceError::Lod(
            ws_life::lod::LodError::Invariant,
        )),
        20 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::Navigation(ws_life::navigation::NavError::Geometry),
        )),
        21 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::InfeasibleRoute,
        )),
        22 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::Grid,
        )),
        23 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::Time,
        )),
        24 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::Counter,
        )),
        25 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::UnknownResident,
        )),
        26 => Ok(ws_sim::slice::SliceError::Behavior(
            ws_life::behavior::BehaviorError::Resource,
        )),
        _ => Err(CodecError("invalid error tag")),
    }
}

pub(crate) fn get_operation_record(bytes: &[u8]) -> Result<ws_sim::slice::OperationRecord> {
    let mut r = Reader::new(bytes);
    let operation = match r.u8()? {
        0 => ws_sim::slice::Operation::Steps(r.u64()?),
        1 => {
            let mult = r.u32()?;
            let scale = match mult {
                0 => ws_sim::TimeScale::Paused,
                1 => ws_sim::TimeScale::X1,
                10 => ws_sim::TimeScale::X10,
                100 => ws_sim::TimeScale::X100,
                _ => return Err(CodecError("invalid timescale")),
            };
            ws_sim::slice::Operation::SetScale(scale)
        }
        2 => ws_sim::slice::Operation::HostElapsed(r.u64()?),
        3 => {
            let resident = r.id()?;
            let level = ws_life::behavior::Detail::from_tag(r.u8()?)
                .ok_or(CodecError("invalid detail tag"))?;
            ws_sim::slice::Operation::SetDetail { resident, level }
        }
        4 => {
            let east = r.i64()?;
            let north = r.i64()?;
            let radius_mm = r.u64()?;
            ws_sim::slice::Operation::CameraInterest(ws_life::lod::CameraInterest {
                center: ws_life::navigation::Point { east, north },
                radius_mm,
            })
        }
        5 => {
            let resident = r.id()?;
            let severity = r.u32()?;
            let duration_us = r.u64()?;
            ws_sim::slice::Operation::Afflict {
                resident,
                severity,
                duration_us,
            }
        }
        6 => ws_sim::slice::Operation::Bless { resident: r.id()? },
        7 => ws_sim::slice::Operation::Storm {
            duration_us: r.u64()?,
        },
        8 => {
            let building = r.id()?;
            let duration_us = r.u64()?;
            ws_sim::slice::Operation::Gather {
                building,
                duration_us,
            }
        }
        _ => return Err(CodecError("unknown operation tag")),
    };
    let committed = r.u64()?;
    let error = match r.u8()? {
        0 => None,
        1 => Some(error_from_tag(r.u64()?)?),
        _ => return Err(CodecError("invalid error flag")),
    };
    let report = ws_sim::slice::Report { committed, error };
    let stop = match r.u8()? {
        0 => None,
        1 => {
            let tick = r.u64()?;
            let state = r.u64()?;
            let events = r.u64()?;
            let pending_micros = r.u64()?;
            Some(ws_sim::slice::Checkpoint {
                tick,
                state,
                events,
                pending_micros,
            })
        }
        _ => return Err(CodecError("invalid stop flag")),
    };
    r.finish()?;
    Ok(ws_sim::slice::OperationRecord {
        operation,
        report,
        stop,
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
            put_payload(&mut w, p).unwrap();
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

    #[test]
    fn format_3_records_round_trip() {
        let cell = CellId::from_geodetic_at_level(&BAY_RIDGE, SECTOR_LEVEL).unwrap();
        let pos = SurfacePos::new(cell, [4.0, 5.0, 6.0]).unwrap();
        let b = Building {
            index: 2,
            pos,
            half_extent_m: 12.0,
            height_m: 18.0,
        };
        assert_eq!(get_building(&put_building(&b, Lod::L2)), Ok((b, Lod::L2)));
        assert_eq!(get_building_component(&put_building_component(&b)), Ok(b));
        assert_eq!(get_node_lod(&put_node_lod(Lod::L1)), Ok(Lod::L1));
        assert_eq!(get_node_lod(&put_node_lod(Lod::L2)), Ok(Lod::L2));

        let r = Resident {
            address: ResidentAddress {
                building_index: 2,
                slot: 4,
            },
            home: id(10),
        };
        for rec in [EntityRecord::Building(b), EntityRecord::Resident(r)] {
            assert_eq!(get_entity_record(&put_entity_record(&rec)), Ok(rec));
        }
        assert!(get_entity_record(&[9]).is_err());

        for t in [
            Tombstone {
                address: BaselineAddress::Building(3),
                deleted_tick: 40,
                root_event: 5,
            },
            Tombstone {
                address: BaselineAddress::Resident(ResidentAddress {
                    building_index: 1,
                    slot: 7,
                }),
                deleted_tick: 80,
                root_event: 0,
            },
        ] {
            assert_eq!(get_tombstone(&put_tombstone(&t)), Ok(t));
        }
        assert!(get_tombstone(&[9]).is_err());

        let mut base = BaselineRecord {
            seed: 2026,
            dataset_id: 1,
            dataset_version: 1,
            dataset_sha256: [0x5a; 32],
            building_generator_version: 1,
            resident_generator_version: 1,
            rules_version: 1,
            identity_policy_version: 1,
            population_min: 4,
            population_max: 12,
            walk_speed_bits: 1.4_f64.to_bits(),
            walk_wait_steps: 50,
            start_time_us: 0,
            building_count: 5,
            resident_count: 42,
            structural_digest: 0x1234_5678,
            baseline_digest: 0,
            collision_overrides: Vec::new(),
        };
        base.baseline_digest = base.compute_digest();
        assert_eq!(get_baseline(&put_baseline(&base)), Ok(base));

        let clock = Clock::from_parts(
            SimTime::EPOCH,
            Rate::new(SimDuration::from_micros(100_000)).unwrap(),
            5,
            TimeScale::X10,
            NonZeroU32::new(16).unwrap(),
            0,
        )
        .unwrap();
        let row = ClockRowV3 {
            clock,
            next_sequence: 3,
            boundary: 2,
            next_spawned_id: 7,
            next_event_id: 11,
        };
        assert_eq!(
            get_clock_v3(&put_clock_v3(&row.clock, 3, 2, 7, 11)),
            Ok(row)
        );
    }
}

#[cfg(test)]
mod slice_vocabulary_tests {
    use super::*;
    #[test]
    fn slice_payload_codec_round_trips_every_slice_tag() {
        for tag in [23, 24, 255] {
            assert!(get_payload(&mut Reader::new(&[tag])).is_err());
        }
        let resident = EntityId::procedural(1);
        let destination = EntityId::procedural(2);
        for payload in [
            Payload::MissedDuty {
                resident,
                destination,
                activity: ws_events::Activity::Work,
            },
            Payload::NeighbourCheckIn {
                visitor: resident,
                patient: destination,
                home: destination,
                intended_duration_us: 300_000_000,
            },
            Payload::Recovered {
                resident,
                health: 100,
            },
            Payload::SliceIllnessApplied {
                resident,
                sequence: 0,
                severity: 5,
                duration_us: 100_000,
                affected: 1,
            },
            Payload::ScheduleChanged {
                resident,
                destination,
                previous: ws_events::Activity::Home,
                next: ws_events::Activity::Commute,
            },
            Payload::SliceCreated {
                seed: 42,
                dataset: 1,
                structural: 1,
                population: 1,
                schedules: 1,
                schedule_epoch: 0,
            },
            Payload::SliceLodChanged {
                building: destination,
                from: 1,
                to: 3,
                demoted: 4,
                promoted: 0,
            },
            Payload::BlessApplied {
                resident,
                sequence: 6,
                affected: 1,
            },
            Payload::NeedsRelieved { resident },
            Payload::StormApplied {
                sequence: 7,
                duration_us: 3_600_000_000,
            },
            Payload::StayedInside {
                resident,
                building: destination,
                activity: ws_events::Activity::School,
            },
            Payload::StormKeptInside { count: 9 },
            Payload::StormEnded { sequence: 7 },
            Payload::GatheringCalled {
                building: destination,
                sequence: 8,
                duration_us: 600_000_000,
                attendees: 7,
            },
            Payload::JoinedGathering {
                resident,
                building: destination,
            },
            Payload::GatheringEnded {
                building: destination,
                attendees: 3,
            },
        ] {
            let mut writer = Writer::new();
            put_payload(&mut writer, payload).unwrap();
            let mut reader = Reader::new(&writer.0);
            let decoded = get_payload(&mut reader).unwrap();
            reader.finish().unwrap();
            assert_eq!(decoded, payload);
        }
    }
}

#[cfg(test)]
mod slice_operation_tests {
    use super::*;
    use ws_sim::slice::{
        Checkpoint, Operation, OperationRecord, Report, SliceError, illness::Rejection,
    };
    #[test]
    fn power_operations_and_rejections_round_trip() {
        let id = EntityId::procedural(7);
        let stop = Some(Checkpoint {
            tick: 3,
            state: 4,
            events: 5,
            pending_micros: 6,
        });
        let operations = [
            Operation::Bless { resident: id },
            Operation::Storm {
                duration_us: 3_600_000_000,
            },
            Operation::Gather {
                building: id,
                duration_us: 600_000_000,
            },
        ];
        let rejections = [
            Rejection::Target,
            Rejection::Dead,
            Rejection::Severity,
            Rejection::Duration,
            Rejection::Deadline,
            Rejection::Building,
            Rejection::Busy,
        ];
        // Existing rejection codes keep their numbers; the new ones append.
        assert_eq!(Rejection::Deadline as u8, 4);
        assert_eq!((Rejection::Building as u8, Rejection::Busy as u8), (5, 6));
        for operation in operations {
            for error in rejections
                .iter()
                .map(|r| Some(SliceError::Rejected(*r)))
                .chain([None])
            {
                let rec = OperationRecord {
                    operation,
                    report: Report {
                        committed: 0,
                        error,
                    },
                    stop: error.and(stop),
                };
                let bytes = put_operation_record(&rec);
                assert_eq!(get_operation_record(&bytes).unwrap(), rec);
            }
        }
        assert_eq!(
            put_operation_record(&OperationRecord {
                operation: operations[0],
                report: Report {
                    committed: 0,
                    error: None
                },
                stop: None,
            })[0],
            6
        );
        let mut bytes = put_operation_record(&OperationRecord {
            operation: operations[0],
            report: Report {
                committed: 0,
                error: Some(SliceError::Rejected(Rejection::Busy)),
            },
            stop,
        });
        // Tag 107 is beyond the rejection table.
        let at = 1 + 8 + 8 + 1;
        bytes[at..at + 8].copy_from_slice(&107u64.to_le_bytes());
        assert!(get_operation_record(&bytes).is_err());
    }
}

#[cfg(test)]
mod format5_tests {
    use super::*;
    use ws_life::behavior::{
        Action, Detail, HoldPhase, Journey, Needs, ResidentStateSnapshot, RoutineOverride,
        StormHold, StormWindow,
    };
    use ws_sim::slice::{Gathering, Phase, Storm};

    fn t(us: i64) -> SimTime {
        SimTime::from_micros_since_epoch(us)
    }

    fn state(
        routine: Option<RoutineOverride>,
        storm_hold: Option<StormHold>,
    ) -> ResidentStateSnapshot {
        ResidentStateSnapshot {
            id: EntityId::procedural(3),
            detail: Detail::L1,
            needs: Needs::checked([1, 2, 3, 4]).unwrap(),
            anchor: t(600_000_000),
            action: Action::from_tag(0).unwrap(),
            decisions: 9,
            routine,
            storm_hold,
        }
    }

    fn routine(kind: u32) -> RoutineOverride {
        RoutineOverride {
            kind,
            root: 12,
            home: EntityId::procedural(4),
            original: None,
            homeward: None,
            started: t(100_000),
            until: t(200_000),
            rejoin: Some(t(300_000)),
        }
    }

    #[test]
    fn storm_hold_block_is_format5_only_and_round_trips_every_field() {
        let walk = Journey {
            from: ws_life::Place::Building(EntityId::procedural(5)),
            to: ws_life::Place::OffMap(EntityId::procedural(6), 2),
            departure: 7_200_000_000,
            arrival: 7_260_000_000,
        };
        for hold in [
            StormHold {
                root: 41,
                building: EntityId::procedural(5),
                activity: ws_events::Activity::Work,
                resume: Some(walk),
                rejoin: None,
                phase: HoldPhase::Deferring,
            },
            StormHold {
                root: 41,
                building: EntityId::procedural(5),
                activity: ws_events::Activity::Home,
                resume: None,
                rejoin: Some(t(9_000_000)),
                phase: HoldPhase::Resuming,
            },
        ] {
            let st = state(None, Some(hold));
            let bytes = put_slice_resident_state(&st, 5);
            assert_eq!(get_slice_resident_state(&bytes, 5), Ok(st));
            // The phase is the last byte; an unknown phase is an error.
            let mut bad = bytes.clone();
            *bad.last_mut().unwrap() = 2;
            assert!(get_slice_resident_state(&bad, 5).is_err());
            // Format 4 never reads the block, so its bytes are trailing.
            assert!(get_slice_resident_state(&bytes, 4).is_err());
        }
        // Without a hold, format 4 bytes are unchanged and format 5 adds one flag.
        let st = state(Some(routine(1)), None);
        let v4 = put_slice_resident_state(&st, 4);
        let v5 = put_slice_resident_state(&st, 5);
        assert_eq!(&v5[..v4.len()], &v4[..]);
        assert_eq!(&v5[v4.len()..], &[0]);
        assert_eq!(get_slice_resident_state(&v4, 4), Ok(st));
        assert_eq!(get_slice_resident_state(&v5, 5), Ok(st));
    }

    #[test]
    fn routine_kind_range_widens_to_three_in_format5() {
        for (kind, v4_ok, v5_ok) in [
            (0, false, false),
            (1, true, true),
            (2, true, true),
            (3, false, true),
            (4, false, false),
        ] {
            let st = state(Some(routine(kind)), None);
            assert_eq!(
                get_slice_resident_state(&put_slice_resident_state(&st, 4), 4).is_ok(),
                v4_ok,
                "kind {kind} in format 4"
            );
            assert_eq!(
                get_slice_resident_state(&put_slice_resident_state(&st, 5), 5).is_ok(),
                v5_ok,
                "kind {kind} in format 5"
            );
        }
    }

    #[test]
    fn storm_and_gathering_rows_round_trip_and_reject_malformed() {
        let storm = Storm {
            root: EventId(41),
            start: t(1_000_000),
            end: t(3_601_000_000),
            kept: 5,
        };
        let window = StormWindow {
            start: t(1_000_000),
            end: t(3_601_000_000),
            root: 41,
        };
        for row in [
            (Some(storm), Some(window)),
            (None, Some(window)),
            (Some(storm), None),
        ] {
            assert_eq!(get_slice_storm(&put_slice_storm(row.0, row.1)), Ok(row));
        }
        assert!(get_slice_storm(&put_slice_storm(None, None)).is_err());
        let mut trailing = put_slice_storm(Some(storm), Some(window));
        trailing.push(0);
        assert!(get_slice_storm(&trailing).is_err());

        let attendees: Vec<_> = (0..7)
            .map(|i| {
                (
                    EntityId::procedural(10 + i),
                    Phase::from_tag((i % 4) as u8).unwrap(),
                )
            })
            .collect();
        for n in [0, 3, 7] {
            let g = Gathering::restore(
                EntityId::procedural(2),
                EventId(50),
                t(10_000_000),
                t(1_810_000_000),
                2,
                &attendees[..n],
            )
            .unwrap();
            let bytes = put_slice_gathering(&g);
            let back = get_slice_gathering(&bytes).unwrap();
            assert_eq!(back, g);
            assert_eq!(back.attendees(), &attendees[..n]);
            assert_eq!(back.joined, 2);
        }
        let g = Gathering::restore(
            EntityId::procedural(2),
            EventId(50),
            t(10_000_000),
            t(1_810_000_000),
            1,
            &attendees[..1],
        )
        .unwrap();
        let bytes = put_slice_gathering(&g);
        // Count byte after building, root, start, end and joined.
        let at = 8 + 8 + 8 + 8 + 4;
        let mut many = bytes.clone();
        many[at] = 8;
        assert!(get_slice_gathering(&many).is_err());
        let mut phase = bytes.clone();
        *phase.last_mut().unwrap() = 4;
        assert!(get_slice_gathering(&phase).is_err());
        assert!(Phase::from_tag(4).is_none());
        let mut overfull = attendees.clone();
        overfull.push((EntityId::procedural(99), Phase::Outbound));
        assert!(
            Gathering::restore(
                EntityId::procedural(2),
                EventId(50),
                t(0),
                t(1),
                0,
                &overfull
            )
            .is_none()
        );
    }
}
