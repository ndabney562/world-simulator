//! Serialized boundary transactions and one-way publication of domain facts.
use crate::*;
use commands::{Accepted, Action, IngressError, MAX_BYTES, MAX_COMMANDS, Rejection};
use ws_events::{
    Actor, Candidate, Cause, Confidence, Location, Origin, Payload, Relation, Role, Target,
};
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
                o.before,
                o.after,
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
        let before = self.state_hash();
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
        let after = self.state_hash();
        self.journal.push(Outcome {
            schema_version: 1,
            sequence,
            time: now,
            tick,
            boundary,
            command,
            result,
            before,
            after,
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
        let mut candidates = Vec::new();
        candidates
            .try_reserve(count)
            .map_err(|_| StepError::History(EventError::Resource))?;
        let mut local = [0u64; 2];
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
        Ok(())
    }
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
    }
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
    }
}
