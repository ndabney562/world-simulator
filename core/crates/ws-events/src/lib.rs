//! Versioned meaningful history, deterministic commit and read-only queries (ADR 0019).
//! No event or index feeds back into domain simulation. IDs/order are Tier 1 for
//! a given candidate stream; the stream's evolving quantities are Tier 2.
use ws_ecs::EntityId;
use ws_math::{CellId, SurfacePos};
use ws_rng::SipHasher24;
use ws_time::SimTime;

/// Small bounded inline list; no per-record heap allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Small<T: Copy, const N: usize> {
    values: [Option<T>; N],
    len: usize,
}
impl<T: Copy, const N: usize> Default for Small<T, N> {
    fn default() -> Self {
        Self {
            values: [None; N],
            len: 0,
        }
    }
}
impl<T: Copy, const N: usize> Small<T, N> {
    /// Appends, rejecting capacity overflow rather than dropping data.
    pub fn push(&mut self, value: T) -> Result<(), EventError> {
        if self.len == N {
            return Err(EventError::Limit);
        }
        self.values[self.len] = Some(value);
        self.len += 1;
        Ok(())
    }
    /// Values in canonical producer order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.values[..self.len].iter().flatten()
    }
    /// Number of values.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
/// Bounded factual actor-event projection; does not discard retained history.
#[derive(Debug, PartialEq)]
pub struct RecentEvents<'a> {
    /// At most sixteen events in descending occurrence-time/EventId order.
    pub records: Vec<&'a Event>,
    /// A seventeenth matching retained event exists.
    pub more: bool,
}
/// Stable nonzero world-scoped event identity, never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventId(pub u64);
/// Actor role in an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    /// Principal entity.
    Subject = 0,
    /// Other participant or destination.
    Object = 1,
}
/// Entity and its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    /// Stable entity ID.
    pub id: EntityId,
    /// Semantic role.
    pub role: Role,
}
/// Honest confidence class of a causal claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Confidence {
    /// Proven producer link.
    Direct = 0,
    /// Approximate contribution.
    Contributing = 1,
    /// Aggregate association.
    Statistical = 2,
    /// Counterfactual comparison.
    Counterfactual = 3,
}
/// Causal relation, distinct from temporal adjacency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Relation {
    /// Direct cause.
    CausedBy = 0,
    /// Precondition.
    EnabledBy = 1,
    /// Immediate trigger.
    TriggeredBy = 2,
    /// Partial contribution.
    ContributedTo = 3,
}
/// A committed cause or an earlier candidate in this transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// An existing event.
    Id(EventId),
    /// Zero-based candidate index, strictly preceding the source.
    Local(u32),
}
/// Candidate-local references are resolved before publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cause {
    /// Target identity or local reference.
    pub target: Target,
    /// Relation type.
    pub relation: Relation,
    /// Strength of claim.
    pub confidence: Confidence,
}
/// Semantic location; world-scoped events use None.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    /// Spatial cell.
    pub cell: CellId,
    /// Optional precise position in that cell.
    pub precise: Option<SurfacePos>,
    /// Semantic node ancestry, coarse to fine.
    pub path: Small<EntityId, 8>,
}
impl Location {
    /// Toy building location.
    pub fn building(id: EntityId, p: SurfacePos) -> Self {
        let mut path = Small::default();
        path.push(id).expect("one path element");
        Self {
            cell: p.cell(),
            precise: Some(p),
            path,
        }
    }
}
/// Origin of a factual record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Declared producer system.
    System(u16),
    /// Applied journal command sequence.
    Command(u64),
}
/// Stable schedule activity vocabulary (ADR 0027); integer Tier 1 tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Activity {
    /// Sleeping at home.
    Sleep = 0,
    /// A coarse journey between two explicit endpoints.
    Commute = 1,
    /// Employment at a local or off-map workplace.
    Work = 2,
    /// Education off-map.
    School = 3,
    /// Shopping at the synthetic shop.
    Errands = 4,
    /// Awake at home.
    Home = 5,
}
impl Activity {
    /// Decodes only the six assigned activity tags.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Sleep),
            1 => Some(Self::Commute),
            2 => Some(Self::Work),
            3 => Some(Self::School),
            4 => Some(Self::Errands),
            5 => Some(Self::Home),
            _ => None,
        }
    }
}
/// Initial typed event subset; numbers and fields are explicitly serialized by users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// Configuration/version summary at creation.
    WorldCreated {
        /// World seed.
        seed: u64,
        /// Dataset version.
        dataset: u64,
        /// Generator version.
        generator: u32,
        /// Rules version.
        rules: u32,
        /// Minimum residents per building.
        min: u32,
        /// Maximum residents per building.
        max: u32,
    },
    /// Applied command, including idempotent requests and NoOp.
    CommandApplied {
        /// Journal sequence.
        sequence: u64,
        /// Stable payload opcode.
        kind: u32,
        /// New scale tag or number of changed buildings.
        effect: u32,
        /// Total demoted.
        demoted: u32,
        /// Total promoted.
        promoted: u32,
    },
    /// One building actually changed level.
    LodChanged {
        /// Building.
        building: EntityId,
        /// Prior level.
        from: u8,
        /// Committed level.
        to: u8,
        /// Residents demoted here.
        demoted: u32,
        /// Residents promoted here.
        promoted: u32,
    },
    /// Actual arrival from a successful domain plan.
    Arrived {
        /// Resident.
        resident: EntityId,
        /// Destination.
        destination: EntityId,
        /// Stable leg counter.
        leg: u32,
    },
    /// An intervention command took effect (root of its consequences,
    /// ADR 0021 §5).
    InterventionApplied {
        /// Journal sequence of the command.
        sequence: u64,
        /// Effect tag in the effect vocabulary (0: afflict).
        effect: u8,
        /// Illness kind tag.
        illness: u8,
        /// Severity.
        severity: u8,
        /// Residents newly affected.
        affected: u32,
    },
    /// A resident fell ill.
    LifeIllness {
        /// The resident.
        resident: EntityId,
        /// Illness kind tag.
        illness: u8,
        /// Severity.
        severity: u8,
    },
    /// A real schedule boundary, not a fabricated physical arrival.
    ScheduleChanged {
        /// Resident whose activity changed.
        resident: EntityId,
        /// New activity's destination building or off-map workplace.
        destination: EntityId,
        /// Activity immediately before the boundary.
        previous: Activity,
        /// Activity owning the boundary and following interval.
        next: Activity,
    },
    /// Version-pinned real-slice creation, without invented past facts.
    SliceCreated {
        /// World seed.
        seed: u64,
        /// Registered source version.
        dataset: u64,
        /// Structural generator version.
        structural: u32,
        /// Population generator version.
        population: u32,
        /// Schedule rules version.
        schedules: u32,
        /// Monday midnight in the model week, in absolute simulation microseconds.
        schedule_epoch: i64,
    },
    /// Real-slice node-mode or automatic resident-membership change (ADR0029).
    SliceLodChanged {
        /// Governing featured building.
        building: EntityId,
        /// Previous node mode,1 through3.
        from: u8,
        /// New node mode,1 through3.
        to: u8,
        /// Residents actually changed to L2 under this governing node.
        demoted: u32,
        /// Residents actually changed to L1 under this governing node.
        promoted: u32,
    },
    /// Actual duty interruption or missed scheduled duty caused by an episode.
    MissedDuty {
        /// Affected resident.
        resident: EntityId,
        /// Registered duty destination.
        destination: EntityId,
        /// Work or School only.
        activity: Activity,
    },
    /// Actual L1 same-building social check-in.
    NeighbourCheckIn {
        /// Visiting resident.
        visitor: EntityId,
        /// Ill resident.
        patient: EntityId,
        /// Shared home building.
        home: EntityId,
        /// Intended visit duration; later interventions may interrupt.
        intended_duration_us: u64,
    },
    /// Finite episode recovery.
    Recovered {
        /// Recovered resident.
        resident: EntityId,
        /// Authoritative restored health.
        health: u32,
    },
    /// Real-slice intervention root, including accepted duplicate no-ops.
    SliceIllnessApplied {
        /// Target resident.
        resident: EntityId,
        /// Normalized operation-record index.
        sequence: u64,
        /// Severity 1 through 10.
        severity: u32,
        /// Requested duration, integer microseconds.
        duration_us: u64,
        /// Zero for duplicate, one for a new episode.
        affected: u32,
    },
    /// Real-slice bless root, including accepted no-ops (ADR 0038).
    BlessApplied {
        /// Target resident.
        resident: EntityId,
        /// Normalized operation-record index.
        sequence: u64,
        /// Zero for a no-op, one when an episode or need deficit was cured.
        affected: u32,
    },
    /// A bless relieved a healthy resident's need deficits.
    NeedsRelieved {
        /// Relieved resident.
        resident: EntityId,
    },
    /// Real-slice storm root (ADR 0038).
    StormApplied {
        /// Normalized operation-record index.
        sequence: u64,
        /// Requested duration, integer microseconds.
        duration_us: u64,
    },
    /// A departure the storm deferred.
    StayedInside {
        /// Resident that stayed.
        resident: EntityId,
        /// Building the resident stayed in.
        building: EntityId,
        /// Activity of the deferred row.
        activity: Activity,
    },
    /// Storm summary at its end.
    StormKeptInside {
        /// Number of `StayedInside` events recorded during the storm.
        count: u32,
    },
    /// The storm ended.
    StormEnded {
        /// Operation-record index of the storm root.
        sequence: u64,
    },
    /// Real-slice gathering root (ADR 0038).
    GatheringCalled {
        /// Target featured building.
        building: EntityId,
        /// Normalized operation-record index.
        sequence: u64,
        /// Requested duration, integer microseconds.
        duration_us: u64,
        /// Residents summoned, at most seven.
        attendees: u32,
    },
    /// An attendee arrived at the gathering.
    JoinedGathering {
        /// Attending resident.
        resident: EntityId,
        /// Gathering building.
        building: EntityId,
    },
    /// The gathering ended.
    GatheringEnded {
        /// Gathering building.
        building: EntityId,
        /// Residents that recorded `JoinedGathering`.
        attendees: u32,
    },
    /// A daily goal was completed (ADR 0039).
    GoalCompleted {
        /// Day the goal belongs to.
        day: i64,
        /// Goal slot within the day.
        slot: u8,
        /// Goal kind code.
        kind: u8,
        /// Goal target code.
        target: u8,
    },
    /// An ill resident arrived at its home.
    ReturnedHome {
        /// The resident.
        resident: EntityId,
        /// Its home building.
        home: EntityId,
    },
}
impl Payload {
    /// Hierarchical event taxonomy name.
    pub fn name(self) -> &'static str {
        match self {
            Self::MissedDuty { .. } => "life.missed_duty",
            Self::NeighbourCheckIn { .. } => "social.neighbour_check_in",
            Self::Recovered { .. } => "life.recovered",
            Self::SliceIllnessApplied { .. } => "intervention.slice_illness_applied",
            Self::BlessApplied { .. } => "intervention.bless_applied",
            Self::NeedsRelieved { .. } => "life.needs_relieved",
            Self::StormApplied { .. } => "intervention.storm_applied",
            Self::StayedInside { .. } => "life.stayed_inside",
            Self::StormKeptInside { .. } => "life.storm_kept_inside",
            Self::StormEnded { .. } => "life.storm_ended",
            Self::GatheringCalled { .. } => "intervention.gathering_called",
            Self::JoinedGathering { .. } => "social.joined_gathering",
            Self::GatheringEnded { .. } => "social.gathering_ended",
            Self::GoalCompleted { .. } => "god.goal_completed",
            Self::WorldCreated { .. } => "sim.world_created",
            Self::CommandApplied { .. } => "sim.command_applied",
            Self::LodChanged { .. } => "sim.lod_changed",
            Self::Arrived { .. } => "agent.arrived",
            Self::InterventionApplied { .. } => "intervention.applied",
            Self::LifeIllness { .. } => "life.illness",
            Self::ReturnedHome { .. } => "agent.returned_home",
            Self::ScheduleChanged { .. } => "agent.schedule_changed",
            Self::SliceCreated { .. } => "sim.slice_created",
            Self::SliceLodChanged { .. } => "sim.slice_lod_changed",
        }
    }
    /// Stable initial payload tag (schema version is separate).
    pub fn tag(self) -> u32 {
        match self {
            Self::MissedDuty { .. } => 10,
            Self::NeighbourCheckIn { .. } => 11,
            Self::Recovered { .. } => 12,
            Self::SliceIllnessApplied { .. } => 13,
            Self::BlessApplied { .. } => 14,
            Self::NeedsRelieved { .. } => 15,
            Self::StormApplied { .. } => 16,
            Self::StayedInside { .. } => 17,
            Self::StormKeptInside { .. } => 18,
            Self::StormEnded { .. } => 19,
            Self::GatheringCalled { .. } => 20,
            Self::JoinedGathering { .. } => 21,
            Self::GatheringEnded { .. } => 22,
            Self::GoalCompleted { .. } => 23,
            Self::WorldCreated { .. } => 0,
            Self::CommandApplied { .. } => 1,
            Self::LodChanged { .. } => 2,
            Self::Arrived { .. } => 3,
            Self::InterventionApplied { .. } => 4,
            Self::LifeIllness { .. } => 5,
            Self::ReturnedHome { .. } => 6,
            Self::ScheduleChanged { .. } => 7,
            Self::SliceCreated { .. } => 8,
            Self::SliceLodChanged { .. } => 9,
        }
    }
    /// Initial quantized importance, without floating relevance calculation.
    pub fn importance(self) -> u16 {
        match self {
            Self::MissedDuty { .. }
            | Self::NeighbourCheckIn { .. }
            | Self::Recovered { .. }
            | Self::StayedInside { .. }
            | Self::JoinedGathering { .. }
            | Self::NeedsRelieved { .. }
            | Self::StormKeptInside { .. }
            | Self::StormEnded { .. }
            | Self::GatheringEnded { .. }
            | Self::GoalCompleted { .. } => 8192,
            Self::SliceIllnessApplied { .. }
            | Self::BlessApplied { .. }
            | Self::StormApplied { .. }
            | Self::GatheringCalled { .. } => 32768,
            Self::WorldCreated { .. } => 65535,
            Self::CommandApplied { .. } => 0,
            Self::LodChanged { .. } => 16384,
            Self::Arrived { .. } => 4096,
            Self::InterventionApplied { .. } => 32768,
            Self::LifeIllness { .. } => 24576,
            Self::ReturnedHome { .. } => 8192,
            Self::ScheduleChanged { .. } => 4096,
            Self::SliceCreated { .. } => 65535,
            Self::SliceLodChanged { .. } => 16384,
        }
    }
    /// True for the real-slice intervention roots (tags 13, 14, 16 and 20).
    pub fn is_slice_root(&self) -> bool {
        matches!(
            self,
            Self::SliceIllnessApplied { .. }
                | Self::BlessApplied { .. }
                | Self::StormApplied { .. }
                | Self::GatheringCalled { .. }
        )
    }
}
/// Non-causal related event link; TP-008 producers leave this empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Related {
    /// Existing event.
    pub id: EventId,
    /// Versioned relation tag: 0 part-of, 1 follows-up, 2 same-episode.
    pub kind: u8,
}
/// Inline producer record; ID and ordinal are assigned only at commit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// Integer occurrence time.
    pub time: SimTime,
    /// Stable producer system order.
    pub system: u16,
    /// Canonical sequence within this producer/transaction.
    pub sequence: u64,
    /// Typed version-1 payload/effect summary.
    pub payload: Payload,
    /// Producing system or command sequence.
    pub origin: Origin,
    /// Bounded actors and roles.
    pub actors: Small<Actor, 8>,
    /// Optional location.
    pub location: Option<Location>,
    /// Bounded causal links, including earlier local candidates.
    pub causes: Small<Cause, 8>,
    /// Bounded non-causal links.
    pub related: Small<Related, 8>,
    /// Producing LOD; zero for world-scoped control events.
    pub lod: u8,
}
impl Candidate {
    /// Starts a record with empty bounded actor/link lists.
    pub fn new(
        time: SimTime,
        system: u16,
        sequence: u64,
        payload: Payload,
        origin: Origin,
    ) -> Self {
        Self {
            time,
            system,
            sequence,
            payload,
            origin,
            actors: Small::default(),
            location: None,
            causes: Small::default(),
            related: Small::default(),
            lod: 0,
        }
    }
}
/// A committed immutable record; local causal references have become IDs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// Record/payload schema version.
    pub schema_version: u32,
    /// Unique ID.
    pub id: EventId,
    /// Completed tick at publication.
    pub tick: u64,
    /// Monotonic transaction commit ordinal.
    pub ordinal: u64,
    /// Original producer record, with resolved links.
    pub record: Candidate,
    /// Quantized [0,65535] importance.
    pub importance: u16,
    /// False for the initial individual-only subset.
    pub aggregate: bool,
}
impl Event {
    /// Canonical field-wise digest; no struct padding, addresses or platform hashing.
    pub fn digest(&self) -> u64 {
        let mut h = SipHasher24::new(0, 0x5753_4556_454e_5431);
        self.visit_words(|v| h.write_words(v));
        h.finish()
    }
    /// Visits explicit canonical u64 fields, for independent replay observations.
    pub fn visit_words(&self, mut sink: impl FnMut(&[u64])) {
        fn one(sink: &mut impl FnMut(&[u64]), v: u64) {
            sink(&[v]);
        }

        sink(&[
            self.schema_version as u64,
            self.id.0,
            self.tick,
            self.ordinal,
            self.record.time.as_micros_since_epoch() as u64,
            self.record.system as u64,
            self.record.sequence,
            self.record.payload.tag() as u64,
            self.importance as u64,
            u64::from(self.aggregate),
            self.record.lod as u64,
        ]);
        match self.record.origin {
            Origin::System(n) => sink(&[0, n as u64]),
            Origin::Command(n) => sink(&[1, n]),
        }
        match self.record.payload {
            Payload::MissedDuty {
                resident,
                destination,
                activity,
            } => sink(&[resident.raw(), destination.raw(), activity as u64]),
            Payload::NeighbourCheckIn {
                visitor,
                patient,
                home,
                intended_duration_us,
            } => sink(&[
                visitor.raw(),
                patient.raw(),
                home.raw(),
                intended_duration_us,
            ]),
            Payload::Recovered { resident, health } => sink(&[resident.raw(), u64::from(health)]),
            Payload::SliceIllnessApplied {
                resident,
                sequence,
                severity,
                duration_us,
                affected,
            } => sink(&[
                resident.raw(),
                sequence,
                u64::from(severity),
                duration_us,
                u64::from(affected),
            ]),
            Payload::BlessApplied {
                resident,
                sequence,
                affected,
            } => sink(&[resident.raw(), sequence, u64::from(affected)]),
            Payload::NeedsRelieved { resident } => sink(&[resident.raw()]),
            Payload::StormApplied {
                sequence,
                duration_us,
            } => sink(&[sequence, duration_us]),
            Payload::StayedInside {
                resident,
                building,
                activity,
            } => sink(&[resident.raw(), building.raw(), activity as u64]),
            Payload::StormKeptInside { count } => sink(&[u64::from(count)]),
            Payload::StormEnded { sequence } => sink(&[sequence]),
            Payload::GatheringCalled {
                building,
                sequence,
                duration_us,
                attendees,
            } => sink(&[building.raw(), sequence, duration_us, u64::from(attendees)]),
            Payload::JoinedGathering { resident, building } => {
                sink(&[resident.raw(), building.raw()])
            }
            Payload::GatheringEnded {
                building,
                attendees,
            } => sink(&[building.raw(), u64::from(attendees)]),
            Payload::GoalCompleted {
                day,
                slot,
                kind,
                target,
            } => sink(&[
                day as u64,
                u64::from(slot),
                u64::from(kind),
                u64::from(target),
            ]),
            Payload::WorldCreated {
                seed,
                dataset,
                generator,
                rules,
                min,
                max,
            } => sink(&[
                seed,
                dataset,
                generator as u64,
                rules as u64,
                min as u64,
                max as u64,
            ]),
            Payload::CommandApplied {
                sequence,
                kind,
                effect,
                demoted,
                promoted,
            } => sink(&[
                sequence,
                kind as u64,
                effect as u64,
                demoted as u64,
                promoted as u64,
            ]),
            Payload::LodChanged {
                building,
                from,
                to,
                demoted,
                promoted,
            }
            | Payload::SliceLodChanged {
                building,
                from,
                to,
                demoted,
                promoted,
            } => sink(&[
                building.raw(),
                from as u64,
                to as u64,
                demoted as u64,
                promoted as u64,
            ]),
            Payload::Arrived {
                resident,
                destination,
                leg,
            } => sink(&[resident.raw(), destination.raw(), leg as u64]),
            Payload::InterventionApplied {
                sequence,
                effect,
                illness,
                severity,
                affected,
            } => sink(&[
                sequence,
                effect as u64,
                illness as u64,
                severity as u64,
                affected as u64,
            ]),
            Payload::LifeIllness {
                resident,
                illness,
                severity,
            } => sink(&[resident.raw(), illness as u64, severity as u64]),
            Payload::ReturnedHome { resident, home } => sink(&[resident.raw(), home.raw()]),
            Payload::ScheduleChanged {
                resident,
                destination,
                previous,
                next,
            } => sink(&[
                resident.raw(),
                destination.raw(),
                previous as u64,
                next as u64,
            ]),
            Payload::SliceCreated {
                seed,
                dataset,
                structural,
                population,
                schedules,
                schedule_epoch,
            } => sink(&[
                seed,
                dataset,
                structural as u64,
                population as u64,
                schedules as u64,
                schedule_epoch as u64,
            ]),
        }
        one(&mut sink, self.record.actors.len() as u64);
        for a in self.record.actors.iter() {
            sink(&[a.id.raw(), a.role as u64]);
        }
        if let Some(l) = self.record.location {
            sink(&[1, l.cell.raw()]);
            if let Some(p) = l.precise {
                one(&mut sink, 1);
                for v in p.offset() {
                    one(&mut sink, v.to_bits() as u64);
                }
            } else {
                one(&mut sink, 0);
            }
            one(&mut sink, l.path.len() as u64);
            for id in l.path.iter() {
                one(&mut sink, id.raw());
            }
        } else {
            one(&mut sink, 0);
        }
        one(&mut sink, self.record.causes.len() as u64);
        for c in self.record.causes.iter() {
            let Target::Id(id) = c.target else {
                unreachable!("committed reference")
            };
            sink(&[id.0, c.relation as u64, c.confidence as u64]);
        }
        one(&mut sink, self.record.related.len() as u64);
        for r in self.record.related.iter() {
            sink(&[r.id.0, r.kind as u64]);
        }
    }
}
/// Validation or recoverable reservation failure; no record is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    /// Bounded record/counter exhausted.
    Limit,
    /// Noncanonical ordering or unsupported record.
    Order,
    /// Forward, dangling, cyclic or temporally impossible reference.
    Cause,
    /// Actor/location cannot resolve in the registered world.
    UnknownActor,
    /// Fallible buffer reservation failed.
    Resource,
}
impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "event error: {self:?}")
    }
}
impl std::error::Error for EventError {}
/// Append-only arena and rebuildable ordered indices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EventStore {
    events: Vec<Event>,
    time: Vec<(SimTime, EventId)>,
    // One append-only ID list per sorted registered actor; never shift history.
    actors: Vec<Vec<EventId>>,
    // Rebuildable occurrence-time/ID order for bounded recent actor queries.
    actor_time: Vec<Vec<EventId>>,
    known: Vec<EntityId>,
    ordinal: u64,
    digest: u64,
}
impl EventStore {
    /// Registers all live toy actors; future deletion/legends are out of scope.
    pub fn new(mut known: Vec<EntityId>) -> Self {
        known.sort_unstable();
        known.dedup();
        Self {
            actors: vec![Vec::new(); known.len()],
            actor_time: vec![Vec::new(); known.len()],
            known,
            ..Self::default()
        }
    }
    /// Registers a newly spawned actor in canonical ID order if not already known.
    pub fn register_actor(&mut self, id: EntityId) {
        if let Err(at) = self.known.binary_search(&id) {
            self.known.insert(at, id);
            self.actors.insert(at, Vec::new());
            self.actor_time.insert(at, Vec::new());
        }
    }
    /// Immutable arena in commit/ID order.
    pub fn records(&self) -> &[Event] {
        &self.events
    }
    /// Constant-time lookup, retaining stable IDs.
    pub fn get(&self, id: EventId) -> Option<&Event> {
        usize::try_from(id.0.checked_sub(1)?)
            .ok()
            .and_then(|i| self.events.get(i))
    }
    /// Independent history digest, excluding derived index capacity/order.
    pub fn digest(&self) -> u64 {
        self.digest
    }
    /// Next event ID and next event-bearing commit ordinal.
    pub fn counters(&self) -> Result<(u64, u64), EventError> {
        Ok((
            (self.events.len() as u64)
                .checked_add(1)
                .ok_or(EventError::Limit)?,
            self.ordinal.checked_add(1).ok_or(EventError::Limit)?,
        ))
    }
    /// Allocated arena/index bytes, excluding registered actor IDs.
    pub fn capacity_bytes(&self) -> usize {
        self.actor_time.capacity() * size_of::<Vec<EventId>>()
            + self
                .actor_time
                .iter()
                .map(|v| v.capacity() * size_of::<EventId>())
                .sum::<usize>()
            + self.events.capacity() * size_of::<Event>()
            + self.time.capacity() * size_of::<(SimTime, EventId)>()
            + self.actors.capacity() * size_of::<Vec<EventId>>()
            + self
                .actors
                .iter()
                .map(|ids| ids.capacity() * size_of::<EventId>())
                .sum::<usize>()
    }
    /// Validates and reserves a complete transaction before any publication.
    pub fn prepare(
        &mut self,
        tick: u64,
        candidates: &[Candidate],
    ) -> Result<PreparedEvents<'_>, EventError> {
        if candidates.is_empty() {
            return Ok(PreparedEvents {
                store: self,
                records: Vec::new(),
            });
        }
        if self.events.last().is_some_and(|e| e.tick > tick) {
            return Err(EventError::Order);
        }
        let ordinal = self.ordinal.checked_add(1).ok_or(EventError::Limit)?;
        let first = (self.events.len() as u64)
            .checked_add(1)
            .ok_or(EventError::Limit)?;
        first
            .checked_add(candidates.len() as u64)
            .ok_or(EventError::Limit)?;
        let mut records = Vec::new();
        records
            .try_reserve(candidates.len())
            .map_err(|_| EventError::Resource)?;
        for (i, c) in candidates.iter().enumerate() {
            if i > 0
                && (c.system, c.sequence) <= (candidates[i - 1].system, candidates[i - 1].sequence)
            {
                return Err(EventError::Order);
            }
            let known = |id: EntityId| self.known.binary_search(&id).is_ok();
            match c.payload {
                Payload::SliceIllnessApplied {
                    resident,
                    sequence,
                    severity,
                    duration_us,
                    affected,
                } => {
                    if !known(resident) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.system != 10
                        || c.lod != 0
                        || c.origin != Origin::Command(sequence)
                        || !(1..=10).contains(&severity)
                        || !(100_000..=86_400_000_000).contains(&duration_us)
                        || duration_us % 100_000 != 0
                        || affected > 1
                        || !c.actors.iter().copied().eq([Actor {
                            id: resident,
                            role: Role::Subject,
                        }])
                        || !c.causes.is_empty()
                        || !c.related.is_empty()
                        || c.location.is_some()
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::MissedDuty {
                    resident,
                    destination,
                    activity,
                } => {
                    if !known(resident) || !known(destination) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.system != 80
                        || !matches!(c.lod, 1 | 2)
                        || c.origin != Origin::System(80)
                        || !matches!(activity, Activity::Work | Activity::School)
                        || !c.actors.iter().copied().eq([
                            Actor {
                                id: resident,
                                role: Role::Subject,
                            },
                            Actor {
                                id: destination,
                                role: Role::Object,
                            },
                        ])
                        || !slice_cause(c, Relation::CausedBy, Confidence::Direct)
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::NeighbourCheckIn {
                    visitor,
                    patient,
                    home,
                    intended_duration_us,
                } => {
                    if !known(visitor) || !known(patient) || !known(home) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.system != 81
                        || c.lod != 1
                        || c.origin != Origin::System(81)
                        || visitor == patient
                        || intended_duration_us != 300_000_000
                        || !c.actors.iter().copied().eq([
                            Actor {
                                id: visitor,
                                role: Role::Subject,
                            },
                            Actor {
                                id: patient,
                                role: Role::Object,
                            },
                            Actor {
                                id: home,
                                role: Role::Object,
                            },
                        ])
                        || !slice_cause(c, Relation::ContributedTo, Confidence::Contributing)
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::Recovered { resident, health } => {
                    if !known(resident) {
                        return Err(EventError::UnknownActor);
                    }
                    // System 82 is the natural episode end (one cause, ADR 0034);
                    // system 84 is a bless cure: CausedBy/Direct the bless root,
                    // then ContributedTo/Direct the episode's illness (ADR 0038 §2).
                    let causes_ok = match c.system {
                        82 => slice_cause(c, Relation::CausedBy, Confidence::Direct),
                        84 => {
                            let mut links = c.causes.iter();
                            c.causes.len() == 2
                                && links.next().is_some_and(|l| {
                                    l.relation == Relation::CausedBy
                                        && l.confidence == Confidence::Direct
                                })
                                && links.next().is_some_and(|l| {
                                    l.relation == Relation::ContributedTo
                                        && l.confidence == Confidence::Direct
                                })
                                && c.related.is_empty()
                                && c.location.is_none()
                        }
                        _ => false,
                    };
                    if !matches!(c.lod, 1 | 2)
                        || c.origin != Origin::System(c.system)
                        || health != 100
                        || !c.actors.iter().copied().eq([Actor {
                            id: resident,
                            role: Role::Subject,
                        }])
                        || !causes_ok
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::LodChanged {
                    building, from, to, ..
                } => {
                    if !known(building) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.system != 20
                        || !(1..=2).contains(&from)
                        || !(1..=2).contains(&to)
                        || from == to
                        || c.lod != to
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::Arrived {
                    resident,
                    destination,
                    ..
                } => {
                    if !known(resident) || !known(destination) {
                        return Err(EventError::UnknownActor);
                    }
                    if !matches!((c.system, c.lod), (30, 1) | (40, 2)) {
                        return Err(EventError::Order);
                    }
                }
                Payload::InterventionApplied { .. } => {
                    if c.system != 10 || c.lod != 0 {
                        return Err(EventError::Order);
                    }
                }
                Payload::LifeIllness { resident, .. } => {
                    if !known(resident) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.system != 12 || c.lod != 0 {
                        return Err(EventError::Order);
                    }
                }
                Payload::ReturnedHome { resident, home } => {
                    if !known(resident) || !known(home) {
                        return Err(EventError::UnknownActor);
                    }
                    if !matches!((c.system, c.lod), (50, 1 | 2)) {
                        return Err(EventError::Order);
                    }
                }
                Payload::ScheduleChanged {
                    resident,
                    destination,
                    previous,
                    next,
                } => {
                    if !known(resident) || !known(destination) {
                        return Err(EventError::UnknownActor);
                    }
                    let expected = [
                        Actor {
                            id: resident,
                            role: Role::Subject,
                        },
                        Actor {
                            id: destination,
                            role: Role::Object,
                        },
                    ];
                    if c.system != 60
                        || c.lod != 2
                        || c.origin != Origin::System(60)
                        || previous == next
                        || !c.actors.iter().copied().eq(expected)
                        || !c.causes.is_empty()
                        || !c.related.is_empty()
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::SliceCreated {
                    structural,
                    population,
                    schedules,
                    ..
                } => {
                    if c.system != 0
                        || c.lod != 0
                        || c.origin != Origin::System(0)
                        || structural == 0
                        || population == 0
                        || schedules == 0
                        || !c.actors.is_empty()
                        || !c.causes.is_empty()
                        || !c.related.is_empty()
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::SliceLodChanged {
                    building,
                    from,
                    to,
                    demoted,
                    promoted,
                } => {
                    if !known(building) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.system != 70
                        || c.origin != Origin::System(70)
                        || !(1..=3).contains(&from)
                        || !(1..=3).contains(&to)
                        || (from == to && demoted == 0 && promoted == 0)
                        || c.lod != to
                        || !c.actors.iter().copied().eq([Actor {
                            id: building,
                            role: Role::Subject,
                        }])
                        || c.location.is_some()
                        || !c.causes.is_empty()
                        || !c.related.is_empty()
                    {
                        return Err(EventError::Order);
                    }
                }
                Payload::BlessApplied {
                    resident: id,
                    sequence,
                    ..
                } => {
                    if !known(id) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.origin != Origin::Command(sequence) || !c.causes.is_empty() {
                        return Err(EventError::Order);
                    }
                }
                Payload::StormApplied { sequence, .. } => {
                    if c.origin != Origin::Command(sequence) || !c.causes.is_empty() {
                        return Err(EventError::Order);
                    }
                }
                Payload::GatheringCalled {
                    building, sequence, ..
                } => {
                    if !known(building) {
                        return Err(EventError::UnknownActor);
                    }
                    if c.origin != Origin::Command(sequence) || !c.causes.is_empty() {
                        return Err(EventError::Order);
                    }
                }
                Payload::NeedsRelieved { resident: id }
                | Payload::JoinedGathering { resident: id, .. }
                | Payload::StayedInside { resident: id, .. } => {
                    if !known(id) {
                        return Err(EventError::UnknownActor);
                    }
                    let other = match c.payload {
                        Payload::JoinedGathering { building, .. }
                        | Payload::StayedInside { building, .. } => Some(building),
                        _ => None,
                    };
                    if other.is_some_and(|b| !known(b)) {
                        return Err(EventError::UnknownActor);
                    }
                }
                Payload::GatheringEnded { building, .. } => {
                    if !known(building) {
                        return Err(EventError::UnknownActor);
                    }
                }
                Payload::StormKeptInside { .. }
                | Payload::StormEnded { .. }
                | Payload::GoalCompleted { .. } => {}
                Payload::WorldCreated { .. } => {
                    if c.system != 0 || c.lod != 0 {
                        return Err(EventError::Order);
                    }
                }
                Payload::CommandApplied { .. } => {
                    if c.system != 10 || c.lod != 0 {
                        return Err(EventError::Order);
                    }
                }
            }
            for a in c.actors.iter() {
                if self.known.binary_search(&a.id).is_err() {
                    return Err(EventError::UnknownActor);
                }
            }
            if let Some(l) = c.location
                && (l.precise.is_some_and(|p| p.cell() != l.cell)
                    || l.path
                        .iter()
                        .any(|id| self.known.binary_search(id).is_err()))
            {
                return Err(EventError::UnknownActor);
            }
            let mut record = *c;
            record.causes = Small::default();
            for link in c.causes.iter() {
                let target = match link.target {
                    Target::Id(id) => self
                        .get(id)
                        .or_else(|| records.iter().find(|e: &&Event| e.id == id))
                        .ok_or(EventError::Cause)?,
                    Target::Local(index) if (index as usize) < i => &records[index as usize],
                    Target::Local(_) => return Err(EventError::Cause),
                };
                if target.record.time > c.time {
                    return Err(EventError::Cause);
                }
                record.causes.push(Cause {
                    target: Target::Id(target.id),
                    ..*link
                })?;
            }
            for related in c.related.iter() {
                if related.kind > 2 || self.get(related.id).is_none() {
                    return Err(EventError::Cause);
                }
            }
            records.push(Event {
                schema_version: 1,
                id: EventId(first + i as u64),
                tick,
                ordinal,
                record,
                importance: c.payload.importance(),
                aggregate: false,
            });
        }
        self.events
            .try_reserve(records.len())
            .map_err(|_| EventError::Resource)?;
        self.time
            .try_reserve(records.len())
            .map_err(|_| EventError::Resource)?;
        // Reserve only touched actors, once per transaction, without scanning
        // the whole registered population. The temporary list scales with this
        // batch, not total history. Duplicate roles count once per event.
        let count = records.iter().try_fold(0usize, |n, e| {
            n.checked_add(e.record.actors.len())
                .ok_or(EventError::Limit)
        })?;
        let mut touched = Vec::new();
        touched
            .try_reserve(count)
            .map_err(|_| EventError::Resource)?;
        for e in &records {
            for (i, actor) in e.record.actors.iter().enumerate() {
                if !e.record.actors.iter().take(i).any(|a| a.id == actor.id) {
                    touched.push(
                        self.known
                            .binary_search(&actor.id)
                            .expect("validated actor"),
                    );
                }
            }
        }
        touched.sort_unstable();
        let mut at = 0;
        while at < touched.len() {
            let actor = touched[at];
            let end = at + touched[at..].partition_point(|i| *i == actor);
            self.actors[actor]
                .try_reserve(end - at)
                .map_err(|_| EventError::Resource)?;
            self.actor_time[actor]
                .try_reserve(end - at)
                .map_err(|_| EventError::Resource)?;
            at = end;
        }
        Ok(PreparedEvents {
            store: self,
            records,
        })
    }
    /// Actor/time query in occurrence-time/ID order, deduplicating multiple roles.
    pub fn query(&self, actor: Option<EntityId>, from: SimTime, to: SimTime) -> Vec<&Event> {
        if from > to {
            return Vec::new();
        }
        if let Some(actor) = actor {
            let Ok(index) = self.known.binary_search(&actor) else {
                return Vec::new();
            };
            let mut result: Vec<_> = self.actors[index]
                .iter()
                .filter_map(|id| self.get(*id))
                .filter(|e| e.record.time >= from && e.record.time <= to)
                .collect();
            result.sort_unstable_by_key(|e| (e.record.time, e.id));
            result.dedup_by_key(|e| e.id);
            result
        } else {
            let start = self.time.partition_point(|(t, _)| *t < from);
            let end = self.time.partition_point(|(t, _)| *t <= to);
            self.time[start..end]
                .iter()
                .filter_map(|(_, id)| self.get(*id))
                .collect()
        }
    }
    /// Newest sixteen distinct actor facts at or before an inclusive cutoff.
    /// Traverses at most seventeen results after a logarithmic cutoff lookup.
    /// # Errors
    /// Bounded result-storage reservation failure.
    pub fn recent(&self, actor: EntityId, to: SimTime) -> Result<RecentEvents<'_>, EventError> {
        let mut records = Vec::new();
        records
            .try_reserve_exact(16)
            .map_err(|_| EventError::Resource)?;
        let mut more = false;
        if let Ok(index) = self.known.binary_search(&actor) {
            let ids = &self.actor_time[index];
            let end =
                ids.partition_point(|id| self.get(*id).expect("indexed event").record.time <= to);
            for id in ids[..end].iter().rev().take(17) {
                if records.len() == 16 {
                    more = true;
                    break;
                }
                records.push(self.get(*id).expect("indexed event"));
            }
        }
        Ok(RecentEvents { records, more })
    }
    /// Deterministic bounded backward causal traversal, with explicit truncation.
    pub fn backward(&self, root: EventId, max_depth: usize, max_results: usize) -> Traversal {
        let mut todo = std::collections::VecDeque::from([(root, 0)]);
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Traversal {
            ids: Vec::new(),
            truncated: false,
        };
        while let Some((id, depth)) = todo.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            let Some(event) = self.get(id) else {
                continue;
            };
            if out.ids.len() == max_results {
                out.truncated = true;
                break;
            }
            out.ids.push(id);
            let mut next: Vec<_> = event
                .record
                .causes
                .iter()
                .filter_map(|c| {
                    if let Target::Id(id) = c.target {
                        Some(id)
                    } else {
                        None
                    }
                })
                .collect();
            next.sort_unstable();
            next.dedup();
            if depth == max_depth {
                out.truncated |= next.iter().any(|id| !seen.contains(id));
            } else {
                for id in next {
                    todo.push_back((id, depth + 1));
                }
            }
        }
        out
    }
    /// The root intervention behind `event`: follows `CausedBy` links (and,
    /// where an event has none, its `ContributedTo` links) back to an event
    /// with no such cause, and returns it when that event is an applied
    /// intervention. `None` if the event is unknown or its history does not
    /// end at an intervention (ADR 0021 §6). Ties go to the smallest ID.
    pub fn root_intervention(&self, event: EventId) -> Option<&Event> {
        let mut at = self.get(event)?;
        // Causal links point strictly backwards in ID order, so this ends.
        loop {
            let next = at
                .record
                .causes
                .iter()
                .filter(|c| c.relation == Relation::CausedBy)
                .chain(
                    at.record
                        .causes
                        .iter()
                        .filter(|c| c.relation == Relation::ContributedTo),
                )
                .find_map(|c| match c.target {
                    Target::Id(id) => Some(id),
                    Target::Local(_) => None,
                });
            match next {
                Some(id) => at = self.get(id)?,
                None => break,
            }
        }
        (matches!(at.record.payload, Payload::InterventionApplied { .. })
            || at.record.payload.is_slice_root())
        .then_some(at)
    }
    /// Reconstructs counters/indices from committed records (not a save codec).
    pub fn from_records(known: Vec<EntityId>, records: &[Event]) -> Result<Self, EventError> {
        let mut store = Self::new(known);
        let mut at = 0;
        while at < records.len() {
            let ordinal = records[at].ordinal;
            if ordinal != store.ordinal + 1 {
                return Err(EventError::Order);
            }
            let end = at + records[at..].partition_point(|e| e.ordinal == ordinal);
            let candidates: Vec<_> = records[at..end].iter().map(|e| e.record).collect();
            let prepared = store.prepare(records[at].tick, &candidates)?;
            if prepared.records.as_slice() != &records[at..end] {
                return Err(EventError::Order);
            }
            prepared.commit();
            at = end;
        }
        Ok(store)
    }
}
fn slice_cause(c: &Candidate, relation: Relation, confidence: Confidence) -> bool {
    c.causes.len() == 1
        && c.causes
            .iter()
            .all(|v| v.relation == relation && v.confidence == confidence)
        && c.related.is_empty()
        && c.location.is_none()
}
/// Query result; IDs are visited deterministically, never silently complete on cutoff.
#[derive(Debug, PartialEq, Eq)]
pub struct Traversal {
    /// Visited IDs including the root when found.
    pub ids: Vec<EventId>,
    /// More reachable causes were omitted.
    pub truncated: bool,
}
/// Reserved, validated publication; dropping it changes no logical history.
pub struct PreparedEvents<'a> {
    store: &'a mut EventStore,
    records: Vec<Event>,
}
impl PreparedEvents<'_> {
    /// Number of validated records.
    pub fn len(&self) -> usize {
        self.records.len()
    }
    /// Whether this transaction emits nothing.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// Publishes using reserved storage; no fallible operation remains.
    pub fn commit(self) {
        for e in self.records {
            let time = (e.record.time, e.id);
            let i = self.store.time.partition_point(|v| *v <= time);
            self.store.time.insert(i, time);
            for (role_index, a) in e.record.actors.iter().enumerate() {
                if e.record
                    .actors
                    .iter()
                    .take(role_index)
                    .any(|prior| prior.id == a.id)
                {
                    continue;
                }
                let i = self
                    .store
                    .known
                    .binary_search(&a.id)
                    .expect("validated actor");
                let ids = &mut self.store.actors[i];
                if ids.last() != Some(&e.id) {
                    ids.push(e.id);
                }
                let arena = &self.store.events;
                let ordered = &mut self.store.actor_time[i];
                let key = (e.record.time, e.id);
                let position = if ordered.last().is_none_or(|id| {
                    let prior = &arena[(id.0 - 1) as usize];
                    (prior.record.time, prior.id) <= key
                }) {
                    ordered.len()
                } else {
                    ordered.partition_point(|id| {
                        let prior = &arena[(id.0 - 1) as usize];
                        (prior.record.time, prior.id) <= key
                    })
                };
                ordered.insert(position, e.id);
            }
            self.store.ordinal = e.ordinal;
            self.store.digest = ws_rng::hash(0, 0x4556_4c4f_4731, &[self.store.digest, e.digest()]);
            self.store.events.push(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_reserves_all_actor_lists_and_capacity_includes_spare_storage() {
        let ids: Vec<_> = (0..3).map(|i| EntityId::spawned(i).unwrap()).collect();
        let mut store = EventStore::new(ids.clone());
        let candidates: Vec<_> = (0..20)
            .map(|i| {
                let mut c = Candidate::new(
                    SimTime::EPOCH,
                    10,
                    i,
                    Payload::CommandApplied {
                        sequence: i,
                        kind: 0,
                        effect: 0,
                        demoted: 0,
                        promoted: 0,
                    },
                    Origin::Command(i),
                );
                for (id, role) in [
                    (ids[0], Role::Subject),
                    (ids[1], Role::Object),
                    (ids[0], Role::Object),
                ] {
                    c.actors.push(Actor { id, role }).unwrap();
                }
                c
            })
            .collect();
        let prepared = store.prepare(0, &candidates).unwrap();
        let reserved = prepared.store.capacity_bytes();
        assert!(prepared.store.actors[0].capacity() >= 20);
        assert!(prepared.store.actors[1].capacity() >= 20);
        assert_eq!(prepared.store.actors[2].capacity(), 0);
        prepared.commit();
        assert_eq!(
            store.capacity_bytes(),
            reserved,
            "commit must not grow any buffer"
        );
        for list in &store.actors[..2] {
            assert_eq!(list, &(1..=20).map(EventId).collect::<Vec<_>>());
        }
        assert_eq!(
            store.capacity_bytes(),
            store.actor_time.capacity() * size_of::<Vec<EventId>>()
                + store
                    .actor_time
                    .iter()
                    .map(|v| v.capacity() * size_of::<EventId>())
                    .sum::<usize>()
                + store.events.capacity() * size_of::<Event>()
                + store.time.capacity() * size_of::<(SimTime, EventId)>()
                + store.actors.capacity() * size_of::<Vec<EventId>>()
                + store
                    .actors
                    .iter()
                    .map(|v| v.capacity() * size_of::<EventId>())
                    .sum::<usize>()
        );
    }

    #[test]
    fn exhausted_commit_counter_reports_error_without_publication() {
        let mut store = EventStore {
            ordinal: u64::MAX,
            ..EventStore::default()
        };
        assert_eq!(store.counters(), Err(EventError::Limit));
        let c = Candidate::new(
            SimTime::EPOCH,
            10,
            0,
            Payload::CommandApplied {
                sequence: 1,
                kind: 0,
                effect: 0,
                demoted: 0,
                promoted: 0,
            },
            Origin::Command(1),
        );
        assert!(matches!(store.prepare(0, &[c]), Err(EventError::Limit)));
        assert!(store.records().is_empty());
    }
}
