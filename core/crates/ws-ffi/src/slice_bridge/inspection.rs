//! ADR0032 bounded little-endian inspection projection.
use super::*;
use ws_sim::slice::inspect::{Entity, Inspection};
use ws_sim::ws_events::Payload;

pub(super) const MAX: usize = 8192;
struct Writer<'a> {
    bytes: &'a mut [u8; MAX],
    at: usize,
}
impl Writer<'_> {
    fn raw(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        let end = self
            .at
            .checked_add(bytes.len())
            .filter(|end| *end <= MAX)
            .ok_or("inspection exceeds cap")?;
        self.bytes[self.at..end].copy_from_slice(bytes);
        self.at = end;
        Ok(())
    }
    fn u32(&mut self, n: u32) -> Result<(), &'static str> {
        self.raw(&n.to_le_bytes())
    }
    fn u64(&mut self, n: u64) -> Result<(), &'static str> {
        self.raw(&n.to_le_bytes())
    }
    fn text(&mut self, s: &str) -> Result<(), &'static str> {
        if s.len() > 128 || s.as_bytes().contains(&0) {
            return Err("invalid inspection text");
        }
        self.u32(s.len() as u32)?;
        self.raw(s.as_bytes())
    }
}

pub(super) fn event_words(payload: Payload) -> Result<[u64; 5], &'static str> {
    Ok(match payload {
        Payload::LifeIllness {
            resident,
            illness,
            severity,
        } => [
            resident.raw(),
            u64::from(illness),
            u64::from(severity),
            0,
            0,
        ],
        Payload::ReturnedHome { resident, home } => [resident.raw(), home.raw(), 0, 0, 0],
        Payload::MissedDuty {
            resident,
            destination,
            activity,
        } => [resident.raw(), destination.raw(), activity as u64, 0, 0],
        Payload::NeighbourCheckIn {
            visitor,
            patient,
            home,
            intended_duration_us,
        } => [
            visitor.raw(),
            patient.raw(),
            home.raw(),
            intended_duration_us,
            0,
        ],
        Payload::Recovered { resident, health } => [resident.raw(), u64::from(health), 0, 0, 0],
        Payload::SliceIllnessApplied {
            resident,
            sequence,
            severity,
            duration_us,
            affected,
        } => [
            resident.raw(),
            sequence,
            u64::from(severity),
            duration_us,
            u64::from(affected),
        ],
        Payload::ScheduleChanged {
            resident,
            destination,
            previous,
            next,
        } => [
            resident.raw(),
            destination.raw(),
            previous as u64,
            next as u64,
            0,
        ],
        Payload::SliceLodChanged {
            building,
            from,
            to,
            demoted,
            promoted,
        } => [
            building.raw(),
            u64::from(from),
            u64::from(to),
            u64::from(demoted),
            u64::from(promoted),
        ],
        Payload::BlessApplied {
            resident,
            sequence,
            affected,
        } => [resident.raw(), sequence, u64::from(affected), 0, 0],
        Payload::NeedsRelieved { resident } => [resident.raw(), 0, 0, 0, 0],
        Payload::StormApplied {
            sequence,
            duration_us,
        } => [sequence, duration_us, 0, 0, 0],
        Payload::StayedInside {
            resident,
            building,
            activity,
        } => [resident.raw(), building.raw(), activity as u64, 0, 0],
        Payload::StormKeptInside { count } => [u64::from(count), 0, 0, 0, 0],
        Payload::StormEnded { sequence } => [sequence, 0, 0, 0, 0],
        Payload::GatheringCalled {
            building,
            sequence,
            duration_us,
            attendees,
        } => [
            building.raw(),
            sequence,
            duration_us,
            u64::from(attendees),
            0,
        ],
        Payload::JoinedGathering { resident, building } => {
            [resident.raw(), building.raw(), 0, 0, 0]
        }
        Payload::GatheringEnded {
            building,
            attendees,
        } => [building.raw(), u64::from(attendees), 0, 0, 0],
        _ => return Err("unsupported inspection event vocabulary"),
    })
}

fn endpoint(p: ws_sim::slice::inspect::Place) -> (u64, u32) {
    use ws_sim::slice::inspect::Place;
    match p {
        Place::Building(id) => (id.raw(), u32::MAX),
        Place::OffMap(id, exit) => (id.raw(), u32::from(exit)),
    }
}
/// Schema3 routine-override coherence, checked before anything is published.
/// Kinds: 0 none, 1 illness home hold, 2 visit, 3 gathering (ADR0038 §4), 4
/// storm hold (ADR0038 D1 as amended). An override needs a root; an
/// overridden resident is either commuting or at home with the Home activity,
/// except that kinds 3 and 4 alone may stand away from home: kind3 at the
/// active gathering's building, Home activity, Socialize; kind4 exactly where
/// its hold places it (`storm_hold`).
fn check_override(
    home: ws_sim::EntityId,
    (location, activity, action): (ws_sim::slice::inspect::Location, u32, u32),
    illness: &ws_sim::slice::illness::State,
    gathering: Option<ws_sim::EntityId>,
    (time, featured): (i128, &[ws_sim::EntityId]),
) -> Result<(), &'static str> {
    use ws_sim::slice::inspect::Location;
    let kind = illness.override_kind;
    if kind > 4
        || (kind == 0) != (illness.override_root == 0)
        || (kind == 4) != illness.hold.is_some()
    {
        return Err("inspection override kind/root incoherent");
    }
    if let Some(h) = illness.hold {
        return storm_hold(
            h,
            illness.override_root,
            (location, activity, action),
            (time, featured),
        );
    }
    if kind == 0 || activity == 1 {
        return Ok(());
    }
    match location {
        Location::AtBuilding(at) if activity == 5 && at == home => Ok(()),
        Location::AtBuilding(at)
            if kind == 3 && activity == 5 && action == 3 && gathering == Some(at) =>
        {
            Ok(())
        }
        _ => Err("inspection override away from home"),
    }
}
/// Kind4: a deferring hold stands at its featured building with the kept
/// activity; a resuming hold is on its walk out of that building (Commute,
/// Travel, the walk's exact endpoints and instants) until arrival, then at the
/// walk's destination (or the building, without a walk) with the kept activity
/// until the rejoin.
fn storm_hold(
    h: ws_sim::slice::inspect::StormHold,
    root: u64,
    (location, activity, action): (ws_sim::slice::inspect::Location, u32, u32),
    (time, featured): (i128, &[ws_sim::EntityId]),
) -> Result<(), &'static str> {
    use ws_sim::slice::inspect::{HoldPhase, Location, Place};
    let here = Place::Building(h.building);
    let kept = h.activity as u32;
    // `choose`: Sleep sleeps, duties for work/school/errands, a home action at home.
    let fits = match activity {
        0 => action == 6,
        2..=4 => action == 4,
        5 => action <= 4,
        _ => false,
    };
    let ok = root == h.root
        && featured.contains(&h.building)
        && kept != 1
        && match (h.phase, h.resume) {
            (HoldPhase::Deferring, Some(row)) => {
                row.from == here
                    && location == Location::AtBuilding(h.building)
                    && activity == kept
                    && fits
            }
            (HoldPhase::Deferring, None) => false,
            (HoldPhase::Resuming, Some(j)) if time < j.arrival => {
                j.from == here
                    && j.departure <= time
                    && activity == 1
                    && action == 5
                    && location
                        == Location::InTransit {
                            from: here,
                            to: j.to,
                            departure: j.departure,
                            arrival: j.arrival,
                        }
            }
            (HoldPhase::Resuming, Some(j)) => {
                j.from == here && location == j.to.into() && activity == kept && fits
            }
            (HoldPhase::Resuming, None) => {
                location == Location::AtBuilding(h.building) && activity == kept && fits
            }
        };
    if ok {
        Ok(())
    } else {
        Err("inspection storm hold incoherent")
    }
}
pub(super) fn encode(
    record: &Inspection<'_>,
    gathering: Option<ws_sim::EntityId>,
    featured: &[ws_sim::EntityId],
    bytes: &mut [u8; MAX],
) -> Result<usize, &'static str> {
    use ws_sim::slice::inspect::Location;
    let mut w = Writer { bytes, at: 96 };
    w.u32(2)?;
    w.u32(0)?;
    let kind = match &record.entity {
        Entity::Resident {
            identity: r,
            vitals,
            ..
        } => {
            w.u64(r.home.raw())?;
            w.u64(r.household.raw())?;
            w.u64(r.workplace.map_or(0, |id| id.raw()))?;
            w.u32(u32::from(r.age))?;
            w.u32(r.role as u32)?;
            w.u32(vitals.sex() as u32)?;
            w.text(r.given_name)?;
            w.text(r.family_name)?;
            1
        }
        Entity::Building { identity: b, .. } => {
            w.u64(u64::from(b.bin))?;
            w.u64(b.bbl)?;
            w.u32(b.height_mm)?;
            w.u32(b.residential_units)?;
            w.text(b.address)?;
            w.text(b.building_class)?;
            w.text(b.featured.ok_or("context is not inspectable")?)?;
            2
        }
    };
    let identity_bytes = w.at - 96;
    if identity_bytes > 1024 {
        return Err("inspection identity exceeds cap");
    }
    u32_at(w.bytes, 100, identity_bytes as u32);
    let state_at = w.at;
    let mut schedules = 0;
    match &record.entity {
        Entity::Resident {
            identity,
            state,
            schedule,
            vitals,
        } => {
            let (tag, from, to) = match state.logical.location {
                Location::AtBuilding(id) => (0, (id.raw(), u32::MAX), (id.raw(), u32::MAX)),
                Location::OffMap(id, exit) => {
                    (1, (id.raw(), u32::from(exit)), (id.raw(), u32::from(exit)))
                }
                Location::InTransit { from, to, .. } => (2, endpoint(from), endpoint(to)),
            };
            for value in [
                state.detail as u32,
                state.logical.activity as u32,
                state.action as u32,
                tag,
            ] {
                w.u32(value)?;
            }
            for value in state.needs.values() {
                w.u32(value)?;
            }
            w.u64(from.0)?;
            w.u64(to.0)?;
            w.u32(from.1)?;
            w.u32(to.1)?;
            w.u32(u32::from(vitals.health()))?;
            w.u32(vitals.life() as u32)?;
            w.u64(record.schedule_epoch.as_micros_since_epoch() as u64)?;
            w.u32(i32::from(schedule.jitter) as u32)?;
            w.u32(1)?;
            let illness = record.illness.ok_or("missing illness state")?;
            check_override(
                identity.home,
                (
                    state.logical.location,
                    state.logical.activity as u32,
                    state.action as u32,
                ),
                &illness,
                gathering,
                (i128::from(record.time.as_micros_since_epoch()), featured),
            )?;
            w.u32(illness.override_kind)?;
            w.u32(u32::from(illness.active))?;
            w.u64(illness.override_root)?;
            w.u64(illness.own_root)?;
            w.u64(illness.end.map_or(0, |t| t.as_micros_since_epoch() as u64))?;
        }
        Entity::Building { summary: b, .. } => {
            w.u64(b.building.raw())?;
            w.u32(b.mode as u32)?;
            w.u32(b.occupancy)?;
            w.u32(b.governed)?;
            for value in b.activities {
                w.u32(value)?;
            }
            w.u32(0)?;
        }
    }
    let state_bytes = w.at - state_at;
    if let Entity::Resident { schedule, .. } = &record.entity {
        for (table, rows) in [(0, schedule.weekday()), (1, schedule.weekend())] {
            for row in rows {
                schedules += 1;
                if schedules > 32 {
                    return Err("inspection schedule exceeds cap");
                }
                let destination = endpoint(row.destination);
                let (origin_kind, origin) = match row.origin {
                    None => (0, (0, u32::MAX)),
                    Some(p) => {
                        let point = endpoint(p);
                        (if point.1 == u32::MAX { 1 } else { 2 }, point)
                    }
                };
                for value in [
                    table,
                    u32::from(row.minute),
                    row.activity as u32,
                    u32::from(destination.1 != u32::MAX),
                    origin_kind,
                    row.arrival_minute.map_or(u32::MAX, u32::from),
                    destination.1,
                    origin.1,
                ] {
                    w.u32(value)?;
                }
                w.u64(destination.0)?;
                w.u64(origin.0)?;
            }
        }
    }
    if record.events.records.len() > 16 {
        return Err("inspection events exceed cap");
    }
    for e in &record.events.records {
        w.u64(e.id.0)?;
        w.u64(e.record.time.as_micros_since_epoch() as u64)?;
        w.u64(e.tick)?;
        w.u64(e.ordinal)?;
        w.u32(e.record.payload.tag())?;
        w.u32(e.schema_version)?;
        let words = event_words(e.record.payload)?;
        for value in words {
            w.u64(value)?;
        }
    }
    let len = w.at;
    u32_at(w.bytes, 0, ABI_VERSION);
    u32_at(w.bytes, 4, 96);
    u64_at(w.bytes, 8, record.tick);
    u64_at(w.bytes, 16, record.time.as_micros_since_epoch() as u64);
    u64_at(w.bytes, 24, record.state_hash);
    u64_at(w.bytes, 32, record.dataset);
    u64_at(w.bytes, 40, record.id.raw());
    for (at, value) in [
        (48, 3),
        (52, kind),
        (56, len as u32),
        (60, u32::from(record.events.more)),
        (64, identity_bytes as u32),
        (68, state_bytes as u32),
        (72, schedules),
        (76, record.events.records.len() as u32),
        (80, 48),
        (84, 80),
        (88, record.scale),
    ] {
        u32_at(w.bytes, at, value);
    }
    Ok(len)
}
/// One pure bounded entity inspection, schema3. Short output reports its exact
/// required length; other failures zero written and leave data untouched.
/// # Safety
/// Handle is live/exclusive; buffer is writable for capacity bytes; written is
/// aligned writable usize storage. All outputs are disjoint from each other,
/// the handle and input. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_inspect(
    handle: *mut WsSliceHandle,
    entity_id: u64,
    buffer: *mut u8,
    capacity: usize,
    written: *mut usize,
) -> WsStatus {
    if written.is_null() || !written.is_aligned() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: caller guarantees a writable aligned disjoint control output.
    unsafe {
        written.write(0);
    }
    // SAFETY: caller supplies exclusively serialized handle and output buffers.
    unsafe {
        operate_slice(handle, |h| {
            let id = ws_sim::EntityId::from_raw(entity_id)
                .map_err(|_| (INVALID_ARGUMENT, "invalid inspection identity"))?;
            if h.world.population().residents().get(id).is_none()
                && h.world.population().world().buildings().get(id).is_none()
            {
                return Err((
                    INVALID_ARGUMENT,
                    "inspection target is not a retained resident or featured building",
                ));
            }
            let record = h
                .world
                .inspect(id)
                .map_err(|_| (STEP_FAILED, "inspection query resource or domain failure"))?;
            let mut scratch = [0u8; MAX];
            let len = encode(
                &record,
                h.world.gathering().map(|g| g.building),
                h.world.population().world().buildings().ids(),
                &mut scratch,
            )
            .map_err(|message| (STEP_FAILED, message))?;
            #[cfg(test)]
            if h.panic_before_output {
                panic!("inspection output fault");
            }
            fill_bytes(&scratch[..len], buffer, capacity, written)
        })
    }
}

/// Consequence header subject (offset48): the resident for illness and bless,
/// the building for a gathering, and 0 only for a storm root (ADR0038 §5).
pub(super) fn header_subject(g: &ws_sim::slice::illness::Group) -> Result<u64, &'static str> {
    match (g.tag, g.subject) {
        (16, None) => Ok(0),
        (13 | 14 | 20, Some(s)) => Ok(s.raw()),
        _ => Err("consequence root subject incoherent"),
    }
}

fn consequence_error(error: ws_sim::slice::SliceError) -> (WsStatus, &'static str) {
    match error {
        ws_sim::slice::SliceError::Rejected(ws_sim::slice::illness::Rejection::Target) => {
            (INVALID_ARGUMENT, "unknown consequence root")
        }
        _ => (STEP_FAILED, "consequence query resource or domain failure"),
    }
}

/// Pure bounded real-slice produced consequences with explicit causal confidence.
/// # Safety
/// Live exclusive handle; buffer and aligned written storage writable/disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ws_slice_consequences(
    handle: *mut WsSliceHandle,
    root: u64,
    buffer: *mut u8,
    capacity: usize,
    written: *mut usize,
) -> WsStatus {
    if written.is_null() || !written.is_aligned() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: caller guarantees writable aligned disjoint control output.
    unsafe {
        written.write(0);
    }
    // SAFETY: caller guarantees exclusive live handle and disjoint output storage.
    unsafe {
        operate_slice(handle, |h| {
            let (g, events) = h
                .world
                .consequences(ws_sim::ws_events::EventId(root))
                .map_err(consequence_error)?;
            let subject = header_subject(g).map_err(|m| (STEP_FAILED, m))?;
            let mut scratch = [0u8; MAX];
            let mut w = Writer {
                bytes: &mut scratch,
                at: 96,
            };
            for e in &events {
                w.u64(e.id.0).map_err(|m| (STEP_FAILED, m))?;
                w.u64(e.record.time.as_micros_since_epoch() as u64)
                    .map_err(|m| (STEP_FAILED, m))?;
                w.u64(e.tick).map_err(|m| (STEP_FAILED, m))?;
                w.u64(e.ordinal).map_err(|m| (STEP_FAILED, m))?;
                w.u32(e.record.payload.tag())
                    .map_err(|m| (STEP_FAILED, m))?;
                w.u32(e.schema_version).map_err(|m| (STEP_FAILED, m))?;
                for value in event_words(e.record.payload).map_err(|m| (STEP_FAILED, m))? {
                    w.u64(value).map_err(|m| (STEP_FAILED, m))?;
                }
                if e.record.causes.len() > 2 {
                    return Err((STEP_FAILED, "consequence cause cap"));
                }
                w.u32(e.record.causes.len() as u32)
                    .map_err(|m| (STEP_FAILED, m))?;
                w.u32(0).map_err(|m| (STEP_FAILED, m))?;
                let mut causes = e.record.causes.iter();
                for _ in 0..2 {
                    let (id, relation, confidence) = if let Some(c) = causes.next() {
                        let ws_sim::ws_events::Target::Id(id) = c.target else {
                            return Err((STEP_FAILED, "unresolved cause"));
                        };
                        (id.0, c.relation as u32, c.confidence as u32)
                    } else {
                        (0, 0, 0)
                    };
                    w.u64(id).map_err(|m| (STEP_FAILED, m))?;
                    w.u32(relation).map_err(|m| (STEP_FAILED, m))?;
                    w.u32(confidence).map_err(|m| (STEP_FAILED, m))?;
                }
                w.u64(0).map_err(|m| (STEP_FAILED, m))?;
            }
            let len = w.at;
            for (at, v) in [
                (0, ABI_VERSION),
                (4, 96),
                (56, events.len() as u32),
                (60, events.len() as u32),
                (64, 128),
                (68, 0),
                (72, 1),
                (76, g.severity),
            ] {
                u32_at(w.bytes, at, v);
            }
            for (at, v) in [
                (8, h.world.clock().steps()),
                (16, h.world.clock().now().as_micros_since_epoch() as u64),
                (24, h.world.state_hash()),
                (32, ws_world_dataset()),
                (40, g.root.0),
                (48, subject),
                (80, g.applied.as_micros_since_epoch() as u64),
                (88, g.end.map_or(0, |t| t.as_micros_since_epoch() as u64)),
            ] {
                u64_at(w.bytes, at, v);
            }
            #[cfg(test)]
            if h.panic_before_output {
                panic!("consequence output fault");
            }
            fill_bytes(&scratch[..len], buffer, capacity, written)
        })
    }
}
fn ws_world_dataset() -> u64 {
    ws_sim::slice::inspect::DATASET_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use ws_sim::ws_events::{Actor, Candidate, EventStore, Origin, Role};
    #[test]
    fn consequence_query_resource_and_invariant_failures_are_not_misreported_as_bad_input() {
        use ws_sim::slice::{SliceError, illness::Rejection};
        use ws_sim::ws_events::EventError;
        assert_eq!(
            consequence_error(SliceError::Rejected(Rejection::Target)).0,
            INVALID_ARGUMENT
        );
        for error in [
            EventError::Resource,
            EventError::Cause,
            EventError::Limit,
            EventError::Order,
        ] {
            assert_eq!(consequence_error(SliceError::History(error)).0, STEP_FAILED);
        }
    }
    #[test]
    fn event_words_truncation_unknown_vocabulary_and_text_caps_are_explicit() {
        let world = SliceSimulation::new(SliceConfig {
            seed: 71449,
            start: SimTime::EPOCH,
            schedule_epoch: SimTime::EPOCH,
        })
        .unwrap();
        let resident = world.population().residents().ids()[0];
        let building = world.population().world().buildings().ids()[0];
        let mut store = EventStore::new(vec![resident, building]);
        for sequence in 0..17 {
            let mut c = Candidate::new(
                SimTime::EPOCH,
                60,
                sequence,
                Payload::ScheduleChanged {
                    resident,
                    destination: building,
                    previous: ws_sim::ws_events::Activity::Home,
                    next: ws_sim::ws_events::Activity::Work,
                },
                Origin::System(60),
            );
            c.lod = 2;
            c.actors
                .push(Actor {
                    id: resident,
                    role: Role::Subject,
                })
                .unwrap();
            c.actors
                .push(Actor {
                    id: building,
                    role: Role::Object,
                })
                .unwrap();
            store.prepare(0, &[c]).unwrap().commit();
        }
        let mut record = world.inspect(resident).unwrap();
        record.events = store.recent(resident, SimTime::EPOCH).unwrap();
        let mut bytes = [0; MAX];
        let len = encode(
            &record,
            None,
            world.population().world().buildings().ids(),
            &mut bytes,
        )
        .unwrap();
        assert_eq!(u32::from_le_bytes(bytes[60..64].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(bytes[76..80].try_into().unwrap()), 16);
        let at = len - 16 * 80;
        assert_eq!(
            u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()),
            17
        );
        assert_eq!(
            u32::from_le_bytes(bytes[at + 32..at + 36].try_into().unwrap()),
            7
        );
        assert_eq!(
            u64::from_le_bytes(bytes[at + 40..at + 48].try_into().unwrap()),
            resident.raw()
        );
        assert_eq!(
            u64::from_le_bytes(bytes[at + 48..at + 56].try_into().unwrap()),
            building.raw()
        );
        assert_eq!(
            u64::from_le_bytes(bytes[at + 56..at + 64].try_into().unwrap()),
            5
        );
        assert_eq!(
            u64::from_le_bytes(bytes[at + 64..at + 72].try_into().unwrap()),
            2
        );
        let mut c = Candidate::new(
            SimTime::EPOCH,
            70,
            0,
            Payload::SliceLodChanged {
                building,
                from: 2,
                to: 1,
                demoted: 0,
                promoted: 3,
            },
            Origin::System(70),
        );
        c.lod = 1;
        c.actors
            .push(Actor {
                id: building,
                role: Role::Subject,
            })
            .unwrap();
        store.prepare(0, &[c]).unwrap().commit();
        let mut record = world.inspect(building).unwrap();
        record.events = store.recent(building, SimTime::EPOCH).unwrap();
        bytes.fill(0);
        let len = encode(
            &record,
            None,
            world.population().world().buildings().ids(),
            &mut bytes,
        )
        .unwrap();
        let at = len - 16 * 80;
        assert_eq!(
            u32::from_le_bytes(bytes[at + 32..at + 36].try_into().unwrap()),
            9
        );
        assert_eq!(
            u64::from_le_bytes(bytes[at + 40..at + 48].try_into().unwrap()),
            building.raw()
        );
        assert_eq!(
            u64::from_le_bytes(bytes[at + 72..at + 80].try_into().unwrap()),
            3
        );
        let mut future = EventStore::new(vec![resident]);
        let mut c = Candidate::new(
            SimTime::EPOCH,
            10,
            0,
            Payload::CommandApplied {
                sequence: 0,
                kind: 0,
                effect: 0,
                demoted: 0,
                promoted: 0,
            },
            Origin::Command(0),
        );
        c.actors
            .push(Actor {
                id: resident,
                role: Role::Subject,
            })
            .unwrap();
        future.prepare(0, &[c]).unwrap().commit();
        let mut record = world.inspect(resident).unwrap();
        record.events = future.recent(resident, SimTime::EPOCH).unwrap();
        assert_eq!(
            encode(
                &record,
                None,
                world.population().world().buildings().ids(),
                &mut bytes
            ),
            Err("unsupported inspection event vocabulary")
        );
        let mut writer = Writer {
            bytes: &mut bytes,
            at: 0,
        };
        assert!(writer.text(&"x".repeat(129)).is_err());
        assert!(writer.text("bad\0text").is_err());
    }
}
