//! ADR0029 bounded real-slice camera policy. Integer Tier2 control/replay;
//! aggregates and recent observations are derived from retained individuals.
use crate::{
    Location, Place,
    behavior::{
        Behavior, BehaviorError, CommonSnapshot, Detail, FineLocation, Prepared as BehaviorPlan,
        Snapshot,
    },
    navigation::Point,
};
use ws_ecs::EntityId;
use ws_events::{Actor, Candidate, Origin, Payload, Role};
use ws_rng::{Purpose, SipHasher24};
use ws_time::SimTime;
use ws_world::slice::population::PopulatedSlice;

/// Frozen thresholds, cadence, membership and observation-window policy.
pub const VERSION: u32 = 1;
/// Exactly the four featured buildings, not surrounding visual massing.
pub const NODE_COUNT: usize = 4;
/// Inclusive model-time cache window, in microseconds.
pub const OBSERVATION_WINDOW_US: i128 = 120_000_000;
const UPDATE_US: i128 = 200_000;
const DWELL_US: i128 = 1_000_000;
const MARGIN_MM: i128 = 20_000;
const LIMIT_MM: u64 = 1_000_000_000;

/// One coarse host input for the complete slice, never a per-resident request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraInterest {
    /// ADR0028 approximate-plane integer millimetres, explicitly not ENU.
    pub center: Point,
    /// Zero means overview; a positive radius selects the finest entry band.
    pub radius_mm: u64,
}
impl CameraInterest {
    /// Rejects unsupported ranges without changing any controller state.
    /// # Errors
    /// Center or radius exceeds the versioned inclusive range.
    pub fn validate(self) -> Result<(), LodError> {
        if self.center.east.unsigned_abs() > LIMIT_MM
            || self.center.north.unsigned_abs() > LIMIT_MM
            || self.radius_mm > LIMIT_MM
        {
            return Err(LodError::Input);
        }
        Ok(())
    }
}
/// Building representation; resident membership remains exclusively L1 or L2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    /// Fine local residents may be exposed.
    L1 = 1,
    /// Retained individual schedules and needs.
    L2 = 2,
    /// Aggregate representation over retained coarse identities.
    L3 = 3,
}
impl Mode {
    /// Decode a mode tag (1 = L1, 2 = L2, 3 = L3).
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::L1),
            2 => Some(Self::L2),
            3 => Some(Self::L3),
            _ => None,
        }
    }
}
/// Typed validation/planning failure, before canonical publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodError {
    /// Unsupported camera coordinate or radius.
    Input,
    /// Manual resident detail is unavailable after the first camera input.
    CameraOwned,
    /// The baseline does not have the reviewed four-node complete population.
    Population,
    /// A common behavior plan or query failed.
    Behavior(BehaviorError),
    /// Temporary observation/view capacity could not be reserved.
    Resource,
    /// Planned identity, membership or aggregate partitions are inconsistent.
    Invariant,
}
impl From<BehaviorError> for LodError {
    fn from(value: BehaviorError) -> Self {
        Self::Behavior(value)
    }
}
impl core::fmt::Display for LodError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "slice LOD error: {self:?}")
    }
}
impl std::error::Error for LodError {}

/// Canonical selected mode and actual last-change boundary for one building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    /// Stable building identity.
    pub building: EntityId,
    /// Its immutable synthetic portal proxy.
    pub point: Point,
    /// Selected representation.
    pub mode: Mode,
    /// Creation or last actual mode change, never last input time.
    pub changed_at: SimTime,
}
/// One actual node-mode and/or resident-membership change in this transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    /// Governing building, in canonical order.
    pub building: EntityId,
    /// Previous node mode.
    pub from: Mode,
    /// Planned node mode.
    pub to: Mode,
    /// Residents whose detail changed to L2 under this governing building.
    pub demoted: u32,
    /// Residents whose detail changed to L1 under this governing building.
    pub promoted: u32,
}
impl Change {
    /// Fixed slice-only fact; no command/intervention cause is fabricated.
    pub fn fact(self, time: SimTime, sequence: u64) -> Candidate {
        let mut c = Candidate::new(
            time,
            70,
            sequence,
            Payload::SliceLodChanged {
                building: self.building,
                from: self.from as u8,
                to: self.to as u8,
                demoted: self.demoted,
                promoted: self.promoted,
            },
            Origin::System(70),
        );
        c.lod = self.to as u8;
        c.actors
            .push(Actor {
                id: self.building,
                role: Role::Subject,
            })
            .expect("one inline subject");
        c
    }
}
/// Exact fine variant at an automatic demotion, including off-map crossings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    /// Stable resident identity.
    pub resident: EntityId,
    /// Actual observation boundary, never a fabricated local timestamp.
    pub time: SimTime,
    /// The complete fine-location union at that boundary.
    pub fine: FineLocation,
}
/// Actual occupancy is separate from home or governing-node membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingSummary {
    /// Stable building identity.
    pub building: EntityId,
    /// Current selected representation.
    pub mode: Mode,
    /// Current logical AtBuilding occupants only.
    pub occupancy: u32,
    /// Occupants by Sleep/Commute/Work/School/Errands/Home tags.
    pub activities: [u32; 6],
    /// Policy ownership, including commuters/off-map people; not occupancy.
    pub governed: u32,
}
/// Pure complete count partition at one committed boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// All four rows in ascending building-ID order.
    pub buildings: [BuildingSummary; NODE_COUNT],
    /// Immutable total identity count.
    pub population: u32,
    /// Sum of actual building occupancy.
    pub local_stationary: u32,
    /// Logical journeys, including portal waits and external trip segments.
    pub in_transit: u32,
    /// Logical stationary off-map workplace/school occupants.
    pub off_map_stationary: u32,
    /// Sole L1 membership count.
    pub l1: u32,
    /// Sole L2 membership count.
    pub l2: u32,
}
/// Coarse host snapshot: all people and aggregates from the same boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Boundary represented by every row.
    pub time: SimTime,
    /// Exact derived counts and selected node modes.
    pub summary: Summary,
    /// Complete identity order, with fine views present only at L1.
    pub residents: Vec<Snapshot>,
}
/// Canonical policy input/node state plus bounded derived recent observations.
#[derive(Debug, Clone)]
pub struct Controller {
    epoch: SimTime,
    camera: Option<CameraInterest>,
    nodes: [Node; NODE_COUNT],
    homes: Vec<(EntityId, EntityId)>,
    observations: Vec<Observation>,
}
/// Prepared controller/cache transaction; dropping it publishes nothing.
pub struct Prepared<'a> {
    controller: &'a mut Controller,
    nodes: [Node; NODE_COUNT],
    observations: Option<Vec<Observation>>,
    changes: [Option<Change>; NODE_COUNT],
    summary: Summary,
}
impl Prepared<'_> {
    /// Actual changes in building-ID order, including membership-only facts.
    pub fn changes(&self) -> impl Iterator<Item = Change> + '_ {
        self.changes.iter().flatten().copied()
    }
    /// Independently checked post-tick count partition.
    pub const fn summary(&self) -> &Summary {
        &self.summary
    }
    /// No-fail publication after all behavior/history capacity is reserved.
    pub fn commit(self) {
        self.controller.nodes = self.nodes;
        if let Some(observations) = self.observations {
            self.controller.observations = observations;
        }
    }
}
fn micros(t: SimTime) -> i128 {
    i128::from(t.as_micros_since_epoch())
}
fn select(node: Node, camera: CameraInterest, time: SimTime, scale: u32) -> Mode {
    if camera.radius_mm == 0 {
        return Mode::L3;
    }
    let x = i128::from(node.point.east) - i128::from(camera.center.east);
    let y = i128::from(node.point.north) - i128::from(camera.center.north);
    let distance = x * x + y * y;
    let r = i128::from(camera.radius_mm);
    let within = |limit: i128| distance <= limit * limit;
    let mut mode = if within(r) || (node.mode == Mode::L1 && within(r + MARGIN_MM)) {
        Mode::L1
    } else if within(2 * r) || (node.mode == Mode::L2 && within(2 * r + MARGIN_MM)) {
        Mode::L2
    } else {
        Mode::L3
    };
    if scale > 10 && mode == Mode::L1 {
        mode = Mode::L2;
    }
    // Explicit warp coarsening overrides dwell; subsequent promotion does not.
    if scale > 10 && node.mode == Mode::L1 {
        return mode;
    }
    if micros(time) - micros(node.changed_at) < DWELL_US {
        node.mode
    } else {
        mode
    }
}
/// Canonical governing node, shared by controller counts and read-only transport.
/// Off-map journeys/stationary people retain their home node ownership.
#[must_use]
pub fn governing(location: Location, home: EntityId) -> EntityId {
    match location {
        Location::AtBuilding(id)
        | Location::InTransit {
            to: Place::Building(id),
            ..
        } => id,
        Location::InTransit {
            to: Place::OffMap(..),
            ..
        }
        | Location::OffMap(..) => home,
    }
}
fn index(nodes: &[Node; NODE_COUNT], id: EntityId) -> Result<usize, LodError> {
    nodes
        .binary_search_by_key(&id, |n| n.building)
        .map_err(|_| LodError::Invariant)
}
impl Controller {
    /// Initializes all nodes at L2 with no camera ownership or false changes.
    /// # Errors
    /// Unsupported baseline, unmatched behavior identities/portals or allocation failure.
    pub fn new(
        population: &PopulatedSlice,
        behavior: &Behavior,
        start: SimTime,
        epoch: SimTime,
    ) -> Result<Self, LodError> {
        if population.world().buildings().len() != NODE_COUNT || epoch != behavior.schedule_epoch()
        {
            return Err(LodError::Population);
        }
        let first = population.world().buildings().ids()[0];
        let mut nodes = [Node {
            building: first,
            point: Point { east: 0, north: 0 },
            mode: Mode::L2,
            changed_at: start,
        }; NODE_COUNT];
        for (node, id) in nodes.iter_mut().zip(population.world().buildings().ids()) {
            node.building = *id;
            node.point = behavior
                .navigation()
                .position(Place::Building(*id), Place::Building(*id), 0)
                .map_err(BehaviorError::Navigation)?;
        }
        let mut homes = Vec::new();
        homes
            .try_reserve(population.residents().len())
            .map_err(|_| LodError::Resource)?;
        for (id, r) in population.residents().iter() {
            index(&nodes, r.home)?;
            homes.push((id, r.home));
        }
        if !homes.iter().map(|v| v.0).eq(behavior.residents()) {
            return Err(LodError::Population);
        }
        let controller = Self {
            epoch,
            camera: None,
            nodes,
            homes,
            observations: Vec::new(),
        };
        controller.summary(behavior, start)?;
        Ok(controller)
    }
    /// Restores controller directly from saved camera interest and node modes.
    ///
    /// # Errors
    /// Invalid node count, unknown building or portal calculation failure.
    pub fn from_saved_parts(
        population: &PopulatedSlice,
        behavior: &Behavior,
        epoch: SimTime,
        camera: Option<CameraInterest>,
        saved_nodes: &[(EntityId, Mode, SimTime)],
    ) -> Result<Self, LodError> {
        if saved_nodes.len() != NODE_COUNT
            || population.world().buildings().len() != NODE_COUNT
            || epoch != behavior.schedule_epoch()
        {
            return Err(LodError::Population);
        }
        if let Some(c) = camera {
            c.validate()?;
        }
        let first = population.world().buildings().ids()[0];
        let mut nodes = [Node {
            building: first,
            point: Point { east: 0, north: 0 },
            mode: Mode::L2,
            changed_at: SimTime::EPOCH,
        }; NODE_COUNT];
        for (i, &(id, mode, changed_at)) in saved_nodes.iter().enumerate() {
            if id != population.world().buildings().ids()[i] {
                return Err(LodError::Population);
            }
            nodes[i] = Node {
                building: id,
                point: behavior
                    .navigation()
                    .position(Place::Building(id), Place::Building(id), 0)
                    .map_err(BehaviorError::Navigation)?,
                mode,
                changed_at,
            };
        }
        let mut homes = Vec::new();
        homes
            .try_reserve(population.residents().len())
            .map_err(|_| LodError::Resource)?;
        for (id, r) in population.residents().iter() {
            index(&nodes, r.home)?;
            homes.push((id, r.home));
        }
        if !homes.iter().map(|v| v.0).eq(behavior.residents()) {
            return Err(LodError::Population);
        }
        Ok(Self {
            epoch,
            camera,
            nodes,
            homes,
            observations: Vec::new(),
        })
    }
    /// Latest accepted input; absence leaves manual headless detail exercisable.
    pub const fn camera(&self) -> Option<CameraInterest> {
        self.camera
    }
    /// Canonical nodes, in building-ID order.
    pub const fn nodes(&self) -> &[Node; NODE_COUNT] {
        &self.nodes
    }
    /// Store a validated coarse request without stepping or restarting dwell.
    /// # Errors
    /// Unsupported integer input range, with no mutation on failure.
    pub fn set_camera(&mut self, camera: CameraInterest) -> Result<(), LodError> {
        camera.validate()?;
        self.camera = Some(camera);
        Ok(())
    }
    /// Current unexpired exact observation, inclusive through120 model seconds.
    pub fn observation(&self, id: EntityId, time: SimTime) -> Option<Observation> {
        let i = self
            .observations
            .binary_search_by_key(&id, |o| o.resident)
            .ok()?;
        let o = self.observations[i];
        (0..=OBSERVATION_WINDOW_US)
            .contains(&(micros(time) - micros(o.time)))
            .then_some(o)
    }
    /// Derived cache can be discarded without changing any canonical quantity.
    pub fn discard_observations(&mut self) {
        self.observations.clear();
    }
    /// Reconstruct a cache hit using retained current model state, never a frozen point.
    /// # Errors
    /// Invalid current projection; a miss returns None without changing anything.
    pub fn observed_projection(
        &self,
        behavior: &Behavior,
        id: EntityId,
        time: SimTime,
    ) -> Result<Option<FineLocation>, LodError> {
        self.observation(id, time)
            .map(|_| {
                behavior
                    .projected_location(id, time)
                    .map_err(LodError::from)
            })
            .transpose()
    }
    /// Prepare node policy, every-tick membership and cache alongside common needs.
    /// # Errors
    /// Invalid planned population/partition, behavior projection or reservation failure.
    pub fn prepare<'a>(
        &'a mut self,
        behavior: &mut BehaviorPlan<'_>,
        scale: u32,
    ) -> Result<Prepared<'a>, LodError> {
        if !self.homes.iter().map(|v| v.0).eq(behavior.residents()) {
            return Err(LodError::Invariant);
        }
        let time = behavior.time();
        let mut nodes = self.nodes;
        if nodes.iter().any(|node| time < node.changed_at) {
            return Err(LodError::Invariant);
        }
        if let Some(camera) = self.camera
            && (micros(time) - micros(self.epoch)).rem_euclid(UPDATE_US) == 0
        {
            for node in &mut nodes {
                let mode = select(*node, camera, time, scale);
                if mode != node.mode {
                    node.mode = mode;
                    node.changed_at = time;
                }
            }
        }
        let mut counts = [(0u32, 0u32); NODE_COUNT];
        let mut observations = None;
        if self.camera.is_some() {
            for &(id, home) in &self.homes {
                let snap = behavior.common(id)?;
                let at = index(&nodes, governing(snap.logical.location, home))?;
                let local = if nodes[at].mode == Mode::L1 {
                    match snap.logical.location {
                        Location::AtBuilding(_) => true,
                        Location::OffMap(..) => false,
                        Location::InTransit { .. } => {
                            matches!(behavior.projected_location(id)?, FineLocation::Local { .. })
                        }
                    }
                } else {
                    false
                };
                let level = if local { Detail::L1 } else { Detail::L2 };
                if level != snap.detail {
                    behavior.set_detail(id, level)?;
                    if level == Detail::L1 {
                        counts[at].1 += 1;
                    } else {
                        counts[at].0 += 1;
                        if observations.is_none() {
                            let mut entries = Vec::new();
                            entries
                                .try_reserve(self.homes.len())
                                .map_err(|_| LodError::Resource)?;
                            entries.extend(self.observations.iter().copied().filter(|o| {
                                (0..=OBSERVATION_WINDOW_US)
                                    .contains(&(micros(time) - micros(o.time)))
                            }));
                            observations = Some(entries);
                        }
                        let entries = observations.as_mut().unwrap();
                        let o = Observation {
                            resident: id,
                            time,
                            fine: behavior.projected_location(id)?,
                        };
                        match entries.binary_search_by_key(&id, |o| o.resident) {
                            Ok(i) => entries[i] = o,
                            Err(i) => entries.insert(i, o),
                        }
                    }
                }
            }
        }
        let summary = self.aggregate(&nodes, true, |id| behavior.common(id))?;
        let changes = core::array::from_fn(|i| {
            let (demoted, promoted) = counts[i];
            (nodes[i].mode != self.nodes[i].mode || demoted != 0 || promoted != 0).then_some(
                Change {
                    building: nodes[i].building,
                    from: self.nodes[i].mode,
                    to: nodes[i].mode,
                    demoted,
                    promoted,
                },
            )
        });
        Ok(Prepared {
            controller: self,
            nodes,
            observations,
            changes,
            summary,
        })
    }
    fn aggregate(
        &self,
        nodes: &[Node; NODE_COUNT],
        enforce_automatic: bool,
        mut snapshot: impl FnMut(EntityId) -> Result<CommonSnapshot, BehaviorError>,
    ) -> Result<Summary, LodError> {
        let mut summary = Summary {
            buildings: nodes.map(|n| BuildingSummary {
                building: n.building,
                mode: n.mode,
                occupancy: 0,
                activities: [0; 6],
                governed: 0,
            }),
            population: u32::try_from(self.homes.len()).map_err(|_| LodError::Invariant)?,
            local_stationary: 0,
            in_transit: 0,
            off_map_stationary: 0,
            l1: 0,
            l2: 0,
        };
        for &(id, home) in &self.homes {
            let s = snapshot(id)?;
            if s.resident != id
                || s.needs
                    .values()
                    .iter()
                    .any(|v| *v > crate::behavior::MAX_DEFICIT)
            {
                return Err(LodError::Invariant);
            }
            let gov = index(nodes, governing(s.logical.location, home))?;
            summary.buildings[gov].governed += 1;
            match s.detail {
                Detail::L1 => {
                    if self.camera.is_some()
                        && (nodes[gov].mode == Mode::L3
                            || (enforce_automatic
                                && (nodes[gov].mode != Mode::L1
                                    || matches!(s.logical.location, Location::OffMap(..)))))
                    {
                        return Err(LodError::Invariant);
                    }
                    summary.l1 += 1;
                }
                Detail::L2 => {
                    summary.l2 += 1;
                }
            }
            match s.logical.location {
                Location::AtBuilding(b) => {
                    let row = &mut summary.buildings[index(nodes, b)?];
                    row.occupancy += 1;
                    row.activities[s.logical.activity as usize] += 1;
                    summary.local_stationary += 1;
                }
                Location::InTransit { .. } => summary.in_transit += 1,
                Location::OffMap(..) => summary.off_map_stationary += 1,
            }
        }
        if summary.local_stationary + summary.in_transit + summary.off_map_stationary
            != summary.population
            || summary.l1 + summary.l2 != summary.population
            || summary.buildings.iter().map(|b| b.governed).sum::<u32>() != summary.population
            || summary
                .buildings
                .iter()
                .any(|b| b.activities.iter().sum::<u32>() != b.occupancy)
        {
            return Err(LodError::Invariant);
        }
        Ok(summary)
    }
    /// Pure committed summaries, validated against all retained residents.
    /// # Errors
    /// Unsupported time or a violated complete population/count partition.
    pub fn summary(&self, behavior: &Behavior, time: SimTime) -> Result<Summary, LodError> {
        if !self.homes.iter().map(|v| v.0).eq(behavior.residents()) {
            return Err(LodError::Invariant);
        }
        // The first camera input stores ownership without immediately changing
        // pre-existing diagnostic L1 views at initial L2 nodes. The next physical
        // plan reconciles them and enforces the stronger automatic invariant.
        self.aggregate(&self.nodes, false, |id| behavior.common(id, time))
    }
    /// Complete bulk snapshot; query capacity never enters canonical state.
    /// # Errors
    /// Invalid committed view or failed output reservation.
    pub fn view(&self, behavior: &Behavior, time: SimTime) -> Result<View, LodError> {
        let summary = self.summary(behavior, time)?;
        let mut residents = Vec::new();
        residents
            .try_reserve(self.homes.len())
            .map_err(|_| LodError::Resource)?;
        for &(id, _) in &self.homes {
            residents.push(behavior.snapshot(id, time)?);
        }
        Ok(View {
            time,
            summary,
            residents,
        })
    }
    /// Explicit canonical policy/input/node fields; no observation or query cache.
    pub fn state_hash(&self) -> u64 {
        let mut h = SipHasher24::new(0, Purpose::new("ws-life/slice-camera").value());
        h.write_words(&[
            u64::from(VERSION),
            self.epoch.as_micros_since_epoch() as u64,
        ]);
        match self.camera {
            Some(c) => {
                h.write_words(&[1, c.center.east as u64, c.center.north as u64, c.radius_mm])
            }
            None => h.write_u64(0),
        }
        for n in &self.nodes {
            h.write_words(&[
                n.building.raw(),
                n.mode as u64,
                n.changed_at.as_micros_since_epoch() as u64,
            ]);
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(mode: Mode) -> Node {
        Node {
            building: EntityId::spawned(1).unwrap(),
            point: Point { east: 0, north: 0 },
            mode,
            changed_at: SimTime::EPOCH,
        }
    }
    fn camera(east: i64, north: i64, radius_mm: u64) -> CameraInterest {
        CameraInterest {
            center: Point { east, north },
            radius_mm,
        }
    }
    #[test]
    fn exact_squared_thresholds_hysteresis_dwell_and_explicit_coarsening() {
        let time = SimTime::from_micros_since_epoch(1_000_000);
        assert_eq!(
            select(node(Mode::L3), camera(60_000, 80_000, 100_000), time, 1),
            Mode::L1
        );
        assert_eq!(
            select(node(Mode::L3), camera(100_001, 0, 100_000), time, 1),
            Mode::L2
        );
        assert_eq!(
            select(node(Mode::L1), camera(120_000, 0, 100_000), time, 1),
            Mode::L1
        );
        assert_eq!(
            select(node(Mode::L1), camera(120_001, 0, 100_000), time, 1),
            Mode::L2
        );
        assert_eq!(
            select(node(Mode::L2), camera(220_000, 0, 100_000), time, 1),
            Mode::L2
        );
        assert_eq!(
            select(node(Mode::L2), camera(220_001, 0, 100_000), time, 1),
            Mode::L3
        );
        let early = SimTime::from_micros_since_epoch(900_000);
        assert_eq!(
            select(node(Mode::L2), camera(0, 0, 100_000), early, 1),
            Mode::L2
        );
        assert_eq!(select(node(Mode::L1), camera(0, 0, 0), early, 1), Mode::L3);
        assert_eq!(
            select(node(Mode::L1), camera(0, 0, 100_000), early, 100),
            Mode::L2
        );
        assert_eq!(
            select(node(Mode::L3), camera(0, 0, 100_000), early, 100),
            Mode::L3
        );
        let mut extreme = node(Mode::L2);
        extreme.changed_at = SimTime::MIN;
        assert_eq!(
            select(
                extreme,
                camera(1_000_000_000, -1_000_000_000, 1_000_000_000),
                SimTime::MAX,
                10
            ),
            Mode::L2
        );
    }
}
