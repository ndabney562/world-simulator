//! Serialized boundary transactions and one-way publication of domain facts.
use crate::*;
use commands::{
    Accepted, Action, Effect, IngressError, Intervention, InterventionTarget, MAX_BYTES,
    MAX_COMMANDS, Rejection, Timing,
};
use ws_events::{
    Actor, Candidate, Cause, Confidence, EventId, Location, Origin, Payload, Relation, Role, Target,
};
use ws_world::{Illness, SEVERITY_MAX, SEVERITY_MIN};
/// Infrastructure failure: current command and suffix remain pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    /// Counter exhausted.
    Counter,
    /// Fallible reservation failed.
    Resource,
    /// Domain could not prepare a valid transaction.
    Domain(LodError),
    /// Event validation/reservation failed.
    History(EventError),
    /// Invalid environmental fence or physical step with unflushed input.
    Fence,
}
impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "command processing: {self:?}")
    }
}
impl std::error::Error for ControlError {}
/// Successful boundary operation metadata, including an optional environmental fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushReport {
    /// Pending commands on entry.
    pub entry: u32,
    /// Outcomes committed in this invocation.
    pub committed: u32,
    /// First untouched sequence, zero when drained.
    pub next: u64,
}
impl World {
    /// Copies ownership of a bounded batch atomically; success means pending.
    pub fn submit(&mut self, commands: Vec<Command>) -> Result<Accepted, IngressError> {
        self.validate_submission(&commands)?;
        #[cfg(test)]
        if self.fail_after == Some(0) {
            return Err(IngressError::Resource);
        }
        self.pending
            .try_reserve(commands.len())
            .map_err(|_| IngressError::Resource)?;
        self.enqueue(commands)
    }
    /// Validates fixed queue limits before an environmental zero-enqueue fence.
    pub fn validate_submission(&self, commands: &[Command]) -> Result<(), IngressError> {
        let bytes = commands::validate_batch(commands)? - 8;
        if self.pending.len() + commands.len() > MAX_COMMANDS
            || self.pending_bytes + bytes + 8 > MAX_BYTES
        {
            return Err(IngressError::Capacity);
        }
        self.next_sequence
            .checked_add(commands.len() as u64)
            .ok_or(IngressError::Capacity)?;
        Ok(())
    }
    fn enqueue(&mut self, commands: Vec<Command>) -> Result<Accepted, IngressError> {
        let result = Accepted {
            count: commands.len() as u32,
            first_sequence: if commands.is_empty() {
                0
            } else {
                self.next_sequence
            },
        };
        for c in commands {
            self.pending_bytes += c.encoded_len();
            self.pending.push_back((self.next_sequence, c));
            self.next_sequence += 1;
        }
        Ok(result)
    }
    /// Decodes all borrowed bytes before transferring owned commands to the queue.
    pub fn submit_bytes(&mut self, bytes: &[u8]) -> Result<Accepted, IngressError> {
        self.validate_bytes(bytes)?;
        #[cfg(test)]
        if self.fail_after == Some(0) {
            return Err(IngressError::Resource);
        }
        self.submit(commands::decode_batch(bytes)?)
    }
    /// Allocation-free wire and fixed queue-limit preflight for environmental replay stops.
    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<(), IngressError> {
        let count = commands::validate_wire_batch(bytes)?;
        if self.pending.len() + count > MAX_COMMANDS
            || self.pending_bytes + bytes.len() > MAX_BYTES
            || self.next_sequence.checked_add(count as u64).is_none()
        {
            return Err(IngressError::Capacity);
        }
        Ok(())
    }
    /// Immutable pending commands with assigned enqueue sequences.
    pub fn pending(&self) -> impl Iterator<Item = &(u64, Command)> {
        self.pending.iter()
    }
    /// Immutable applied/rejected journal outcomes.
    pub fn journal(&self) -> &[Outcome] {
        &self.journal
    }
    /// Immutable factual history and derived read-only indices.
    pub fn events(&self) -> &EventStore {
        &self.events
    }
    /// Simulation, pending, journal, event checkpoints, independently compared.
    pub fn checkpoint(&self) -> [u64; 4] {
        [
            self.state_hash(),
            self.queue_digest(),
            self.journal_digest(),
            self.events.digest(),
        ]
    }
    /// Pending future-work digest, including enqueue allocator and typed input.
    pub fn queue_digest(&self) -> u64 {
        let mut h = ws_rng::SipHasher24::new(0, 0x515545554531);
        h.write_u64(self.next_sequence);
        for (seq, c) in &self.pending {
            h.write_u64(*seq);
            command_hash(&mut h, c);
        }
        h.write_u64(self.scheduled.len() as u64);
        for (seq, i) in &self.scheduled {
            h.write_u64(*seq);
            intervention_hash(&mut h, i);
        }
        h.finish()
    }
    /// Canonical field-wise journal digest, excluding human-readable messages.
    pub fn journal_digest(&self) -> u64 {
        let mut h = ws_rng::SipHasher24::new(0, 0x4a4f55524e31);
        for o in &self.journal {
            h.write_words(&[
                o.schema_version as u64,
                o.sequence,
                o.tick,
                o.time.as_micros_since_epoch() as u64,
                o.boundary,
                o.effect as u64,
                o.demoted as u64,
                o.promoted as u64,
            ]);
            command_hash(&mut h, &o.command);
            h.write_words(&reason(o.result));
        }
        h.finish()
    }
    /// Applies the pending prefix without advancing simulation time, even while paused.
    pub fn flush_commands(&mut self) -> Result<FlushReport, ControlError> {
        self.flush_prefix(None)
    }
    /// Replays a recorded environmental stop through normal handlers. A fence must
    /// leave at least one entry command untouched; it is not a semantic rejection.
    pub fn flush_prefix(&mut self, fence: Option<u32>) -> Result<FlushReport, ControlError> {
        let entry = self.pending.len() as u32;
        let count = fence.unwrap_or(entry);
        if fence.is_some() && count >= entry {
            return Err(ControlError::Fence);
        }
        let boundary = if count == 0 {
            self.boundary
        } else {
            self.boundary.checked_add(1).ok_or(ControlError::Counter)?
        };
        let mut committed = 0;
        for _ in 0..count {
            #[cfg(test)]
            if let Some(left) = &mut self.fail_after {
                if *left == 0 {
                    return Err(ControlError::Resource);
                }
                *left -= 1;
            }
            self.apply_front(boundary)?;
            self.boundary = boundary;
            committed += 1;
        }
        Ok(FlushReport {
            entry,
            committed,
            next: self.pending.front().map_or(0, |v| v.0),
        })
    }
    fn apply_front(&mut self, boundary: u64) -> Result<(), ControlError> {
        // Own the control payload before borrowing domain/history independently.
        let (sequence, command) = self.pending.front().expect("nonempty prefix").clone();
        self.journal
            .try_reserve(1)
            .map_err(|_| ControlError::Resource)?;
        let now = self.now();
        let tick = self.tick();
        let mut effect = 0;
        let mut demoted = 0;
        let mut promoted = 0;
        let result = if command.schema_version != 1 {
            Err(Rejection::Schema)
        } else {
            match &command.action {
                Action::SetNodeLod(request) if request.len() > 1024 => Err(Rejection::TooManyNodes),
                Action::SetNodeLod(request) => {
                    match self.toy.prepare_lod(now, self.clock.base_step(), request) {
                        Err(
                            e @ (LodError::UnknownNode(_)
                            | LodError::DuplicateNode(_)
                            | LodError::UnsupportedLevel(_)
                            | LodError::Coord(_)
                            | LodError::Time(_)),
                        ) => Err(Rejection::Lod(e)),
                        Err(e) => return Err(ControlError::Domain(e)),
                        Ok(plan) => {
                            effect = plan.changes().len() as u32;
                            demoted = u32::try_from(plan.report().demoted)
                                .map_err(|_| ControlError::Counter)?;
                            promoted = u32::try_from(plan.report().promoted)
                                .map_err(|_| ControlError::Counter)?;
                            let mut candidates = Vec::new();
                            candidates
                                .try_reserve(1 + plan.changes().len())
                                .map_err(|_| ControlError::Resource)?;
                            candidates.push(applied(now, sequence, 2, effect, demoted, promoted));
                            for (i, change) in plan.changes().iter().enumerate() {
                                let mut c = Candidate::new(
                                    now,
                                    20,
                                    i as u64,
                                    Payload::LodChanged {
                                        building: change.building,
                                        from: lod_tag(change.before),
                                        to: lod_tag(change.after),
                                        demoted: change.demoted as u32,
                                        promoted: change.promoted as u32,
                                    },
                                    Origin::Command(sequence),
                                );
                                c.actors
                                    .push(Actor {
                                        id: change.building,
                                        role: Role::Subject,
                                    })
                                    .expect("one actor");
                                c.location =
                                    Some(Location::building(change.building, change.position));
                                c.lod = lod_tag(change.after);
                                c.causes
                                    .push(Cause {
                                        target: Target::Local(0),
                                        relation: Relation::CausedBy,
                                        confidence: Confidence::Direct,
                                    })
                                    .expect("one cause");
                                candidates.push(c);
                            }
                            #[cfg(test)]
                            if self.fail_history {
                                return Err(ControlError::Resource);
                            }
                            let events = self.events.prepare(tick, &candidates).map_err(history)?;
                            plan.commit();
                            events.commit();
                            Ok(())
                        }
                    }
                }
                Action::Intervene(i) => match self.resolve_intervention(i) {
                    Err(r) => Err(r),
                    Ok(_) if matches!(i.timing, Timing::At(t) if t > now) => {
                        // Waits for its time; applied at the tick boundary
                        // that reaches it (ADR 0021 §1).
                        self.scheduled.insert(sequence, *i);
                        Ok(())
                    }
                    Ok(targets) => {
                        let (first, _) = self.events.counters().map_err(history)?;
                        let (candidates, batch) = intervention_candidates(
                            &self.toy,
                            now,
                            first,
                            &[(sequence, i.effect, targets)],
                        )
                        .map_err(|_| ControlError::Resource)?;
                        effect = batch.len() as u32;
                        #[cfg(test)]
                        if self.fail_history {
                            return Err(ControlError::Resource);
                        }
                        let events = self.events.prepare(tick, &candidates).map_err(history)?;
                        self.toy
                            .afflict(batch)
                            .expect("targets were resolved to healthy residents");
                        events.commit();
                        Ok(())
                    }
                },
                Action::NoOp | Action::SetTimeScale(_) => {
                    if let Action::SetTimeScale(scale) = command.action {
                        effect = scale.multiplier();
                    }
                    let events = self
                        .events
                        .prepare(
                            tick,
                            &[applied(now, sequence, command.opcode(), effect, 0, 0)],
                        )
                        .map_err(history)?;
                    if let Action::SetTimeScale(scale) = command.action {
                        self.clock.set_scale(scale);
                    }
                    events.commit();
                    Ok(())
                }
            }
        };
        self.journal.push(Outcome {
            schema_version: 1,
            sequence,
            time: now,
            tick,
            boundary,
            command,
            result,
            effect,
            demoted,
            promoted,
        });
        let (_, c) = self.pending.pop_front().expect("committed front");
        self.pending_bytes -= c.encoded_len();
        Ok(())
    }
    /// Preflights deterministic clock/domain failures for a replay environmental stop.
    /// Dropping the prepared plan restores all reconciliation state.
    pub fn validate_physical_step(&mut self) -> Result<(), StepError> {
        if !self.pending.is_empty() {
            return Err(StepError::Control(ControlError::Fence));
        }
        let mut clock = self.clock.clone();
        clock.step()?;
        let _plan = self
            .toy
            .prepare_tick(self.clock.now(), self.clock.base_step())?;
        Ok(())
    }
    /// Attempts exactly one physical tick without an implicit flush. Hosts/recorders
    /// must flush first. Domain, clock, and history publication form one transaction.
    pub fn physical_step(&mut self) -> Result<(), StepError> {
        if !self.pending.is_empty() {
            return Err(StepError::Control(ControlError::Fence));
        }
        let mut clock = self.clock.clone();
        clock.step()?;
        // Interventions whose time has been reached take effect at the end
        // of this tick, the boundary the next tick starts from (ADR 0021 §1).
        let reached = clock.now();
        let due_keys: Vec<u64> = self
            .scheduled
            .iter()
            .filter(|(_, i)| matches!(i.timing, Timing::At(t) if t <= reached))
            .map(|(seq, _)| *seq)
            .collect();
        // Targets that no longer resolve are dropped; unreachable while
        // only L1 and L2 exist.
        let due: Vec<_> = due_keys
            .iter()
            .filter_map(|seq| {
                let i = &self.scheduled[seq];
                self.resolve_intervention(i)
                    .ok()
                    .map(|targets| (*seq, i.effect, targets))
            })
            .collect();
        let (first, _) = self.events.counters().map_err(StepError::History)?;
        let (mut candidates, batch) = intervention_candidates(&self.toy, reached, first, &due)
            .map_err(|()| StepError::History(EventError::Resource))?;
        let plan = self
            .toy
            .prepare_tick(self.clock.now(), self.clock.base_step())?;
        let mut facts = Vec::new();
        let count = plan.arrivals().count();
        facts
            .try_reserve(count)
            .map_err(|_| StepError::History(EventError::Resource))?;
        facts.extend(plan.arrivals().copied());
        facts.sort_unstable_by_key(|f| {
            (
                lod_tag(f.lod),
                f.time.unwrap_or(clock.now()),
                f.resident,
                f.leg,
            )
        });
        candidates
            .try_reserve(2 * count)
            .map_err(|_| StepError::History(EventError::Resource))?;
        let mut local = [0u64; 2];
        let mut returned = Vec::new();
        for f in facts {
            let l = lod_tag(f.lod);
            let ix = usize::from(l - 1);
            let mut c = Candidate::new(
                f.time.unwrap_or(clock.now()),
                if l == 1 { 30 } else { 40 },
                local[ix],
                Payload::Arrived {
                    resident: f.resident,
                    destination: f.destination,
                    leg: f.leg,
                },
                Origin::System(if l == 1 { 30 } else { 40 }),
            );
            local[ix] += 1;
            c.lod = l;
            c.actors
                .push(Actor {
                    id: f.resident,
                    role: Role::Subject,
                })
                .expect("two actors");
            c.actors
                .push(Actor {
                    id: f.destination,
                    role: Role::Object,
                })
                .expect("two actors");
            c.location = Some(Location::building(f.destination, f.position));
            candidates.push(c);
            if let Some(root) = plan.illness_root_if_home(f.resident, f.destination) {
                returned.push((f, l, root));
            }
        }
        for (n, (f, l, root)) in returned.into_iter().enumerate() {
            let mut c = Candidate::new(
                f.time.unwrap_or(clock.now()),
                50,
                n as u64,
                Payload::ReturnedHome {
                    resident: f.resident,
                    home: f.destination,
                },
                Origin::System(50),
            );
            c.lod = l;
            c.actors
                .push(Actor {
                    id: f.resident,
                    role: Role::Subject,
                })
                .expect("two actors");
            c.actors
                .push(Actor {
                    id: f.destination,
                    role: Role::Object,
                })
                .expect("two actors");
            c.location = Some(Location::building(f.destination, f.position));
            c.causes
                .push(Cause {
                    target: Target::Id(EventId(root)),
                    relation: Relation::ContributedTo,
                    confidence: Confidence::Contributing,
                })
                .expect("one cause");
            candidates.push(c);
        }
        #[cfg(test)]
        if self.fail_after == Some(0) {
            return Err(StepError::History(EventError::Resource));
        }
        let events = self
            .events
            .prepare(clock.steps(), &candidates)
            .map_err(StepError::History)?;
        plan.commit();
        self.clock = clock;
        events.commit();
        self.toy
            .afflict(batch)
            .expect("targets were resolved to healthy residents");
        for seq in due_keys {
            self.scheduled.remove(&seq);
        }
        Ok(())
    }
}
/// Why a resident is ill: the chain from its illness to the command
/// (ADR 0021 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhyIll<'a> {
    /// The resident's `life.illness` event.
    pub illness_event: EventId,
    /// The `intervention.applied` root event.
    pub intervention_event: EventId,
    /// The journal entry of the command.
    pub journal: &'a Outcome,
}
impl World {
    /// "Why is this resident ill?": the root intervention event and the
    /// journal entry of the command. `None` if the resident is not ill.
    #[must_use]
    pub fn why_ill(&self, resident: EntityId) -> Option<WhyIll<'_>> {
        let ill = self.toy.illness().get(resident)?;
        let illness_event = EventId(ill.root_event);
        let root = self.events.root_intervention(illness_event)?;
        let Payload::InterventionApplied { sequence, .. } = root.record.payload else {
            return None;
        };
        let at = self.journal.partition_point(|o| o.sequence < sequence);
        let journal = self.journal.get(at).filter(|o| o.sequence == sequence)?;
        Some(WhyIll {
            illness_event,
            intervention_event: root.id,
            journal,
        })
    }
    /// Validates an intervention against the current world and resolves its
    /// targets in canonical (ascending ID) order. Changes nothing.
    fn resolve_intervention(&self, i: &Intervention) -> Result<Vec<EntityId>, Rejection> {
        let Effect::Afflict { severity, .. } = i.effect;
        if !(SEVERITY_MIN..=SEVERITY_MAX).contains(&severity) {
            return Err(Rejection::SeverityOutOfRange(severity));
        }
        let targets: Vec<EntityId> = match i.target {
            InterventionTarget::Entity(id) => {
                if !self.toy.residents().contains(id) {
                    return Err(Rejection::UnknownTarget(id));
                }
                vec![id]
            }
            InterventionTarget::ResidentsOf(b) => {
                if !self.toy.buildings().contains(b) {
                    return Err(Rejection::UnknownTarget(b));
                }
                self.toy.residents_of(b).to_vec()
            }
        };
        for id in &targets {
            if !matches!(self.toy.resident_level(*id), Some(Lod::L1 | Lod::L2)) {
                return Err(Rejection::UnsupportedLevel(*id));
            }
        }
        Ok(targets)
    }
}
/// Event candidates and illness rows of a batch of interventions.
type Planned = (Vec<Candidate>, Vec<(EntityId, Illness)>);
/// The events and illness rows of the interventions in `items` (journal
/// sequence, effect, resolved targets), applied together at `now`.
/// `first` is the ID the first candidate will get. Roots come first, then
/// one `life.illness` per newly ill resident; residents already ill (or
/// already ill earlier in this batch) are skipped.
fn intervention_candidates(
    toy: &ToyWorld,
    now: SimTime,
    first: u64,
    items: &[(u64, Effect, Vec<EntityId>)],
) -> Result<Planned, ()> {
    let mut fresh: Vec<Vec<EntityId>> = Vec::with_capacity(items.len());
    let mut seen = std::collections::BTreeSet::new();
    for (_, _, targets) in items {
        fresh.push(
            targets
                .iter()
                .copied()
                .filter(|id| !toy.illness().contains(*id) && seen.insert(*id))
                .collect(),
        );
    }
    let total: usize = fresh.iter().map(Vec::len).sum();
    let mut candidates = Vec::new();
    candidates
        .try_reserve(items.len() + total)
        .map_err(|_| ())?;
    let mut batch = Vec::new();
    batch.try_reserve(total).map_err(|_| ())?;
    for ((sequence, effect, _), new) in items.iter().zip(&fresh) {
        let Effect::Afflict { illness, severity } = *effect;
        candidates.push(Candidate::new(
            now,
            10,
            *sequence,
            Payload::InterventionApplied {
                sequence: *sequence,
                effect: 0,
                illness: illness.tag(),
                severity,
                affected: new.len() as u32,
            },
            Origin::Command(*sequence),
        ));
    }
    let mut j = 0u64;
    for (root, ((sequence, effect, _), new)) in items.iter().zip(&fresh).enumerate() {
        let Effect::Afflict { illness, severity } = *effect;
        for id in new {
            let mut c = Candidate::new(
                now,
                12,
                j,
                Payload::LifeIllness {
                    resident: *id,
                    illness: illness.tag(),
                    severity,
                },
                Origin::Command(*sequence),
            );
            c.actors
                .push(Actor {
                    id: *id,
                    role: Role::Subject,
                })
                .expect("one actor");
            c.causes
                .push(Cause {
                    target: Target::Local(root as u32),
                    relation: Relation::CausedBy,
                    confidence: Confidence::Direct,
                })
                .expect("one cause");
            batch.push((
                *id,
                Illness {
                    kind: illness,
                    severity,
                    root_event: first + candidates.len() as u64,
                },
            ));
            candidates.push(c);
            j += 1;
        }
    }
    Ok((candidates, batch))
}
fn history(e: EventError) -> ControlError {
    if e == EventError::Resource {
        ControlError::Resource
    } else {
        ControlError::History(e)
    }
}
fn applied(
    now: SimTime,
    sequence: u64,
    kind: u32,
    effect: u32,
    demoted: u32,
    promoted: u32,
) -> Candidate {
    Candidate::new(
        now,
        10,
        0,
        Payload::CommandApplied {
            sequence,
            kind,
            effect,
            demoted,
            promoted,
        },
        Origin::Command(sequence),
    )
}
fn lod_tag(l: Lod) -> u8 {
    match l {
        Lod::L1 => 1,
        Lod::L2 => 2,
        _ => 0,
    }
}
fn command_hash(h: &mut ws_rng::SipHasher24, c: &Command) {
    h.write_words(&[
        c.schema_version as u64,
        c.issued_by as u64,
        c.opcode() as u64,
    ]);
    match &c.action {
        Action::NoOp => {}
        Action::SetTimeScale(s) => h.write_u64(s.multiplier() as u64),
        Action::SetNodeLod(v) => {
            h.write_u64(v.len() as u64);
            for (id, l) in v {
                h.write_words(&[id.raw(), *l as u64]);
            }
        }
        Action::Intervene(i) => intervention_hash(h, i),
    }
}
fn intervention_hash(h: &mut ws_rng::SipHasher24, i: &Intervention) {
    let (kind, id) = match i.target {
        InterventionTarget::Entity(id) => (0, id),
        InterventionTarget::ResidentsOf(id) => (1, id),
    };
    let Effect::Afflict { illness, severity } = i.effect;
    let (timing, micros) = match i.timing {
        Timing::Now => (0, 0),
        Timing::At(t) => (1, t.as_micros_since_epoch() as u64),
    };
    h.write_words(&[
        kind,
        id.raw(),
        0,
        u64::from(illness.tag()),
        u64::from(severity),
        timing,
        micros,
    ]);
}
fn reason(r: Result<(), Rejection>) -> [u64; 2] {
    match r {
        Ok(()) => [0, 0],
        Err(Rejection::Schema) => [1, 0],
        Err(Rejection::TooManyNodes) => [2, 0],
        Err(Rejection::Lod(LodError::UnknownNode(id))) => [3, id.raw()],
        Err(Rejection::Lod(LodError::DuplicateNode(id))) => [4, id.raw()],
        Err(Rejection::Lod(LodError::UnsupportedLevel(l))) => [5, l as u64],
        Err(Rejection::Lod(_)) => [6, 0],
        Err(Rejection::UnknownTarget(id)) => [7, id.raw()],
        Err(Rejection::UnsupportedLevel(id)) => [8, id.raw()],
        Err(Rejection::SeverityOutOfRange(s)) => [9, s as u64],
    }
}
