//! Rebuilding a [`World`] from saved parts (ADR 0020).
//!
//! Loading restores canonical state directly and never replays the journal.
//! Derived indices are rebuilt, and every invariant the running world keeps
//! is checked before the world is returned, so a corrupt save is an error
//! and never a half-valid world.

use core::fmt;
use std::collections::VecDeque;

use ws_events::{Event, EventError, EventStore};

use crate::clock::InvalidClock;
use crate::commands::{Intervention, Outcome};
use crate::{Clock, LodViolation, ToyWorld, World};

/// The canonical state of a [`World`] at a boundary with no pending
/// commands: everything a save stores.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldParts {
    /// The root seed.
    pub seed: u64,
    /// The clock.
    pub clock: Clock,
    /// The toy world.
    pub toy: ToyWorld,
    /// Every committed event, in commit order.
    pub events: Vec<Event>,
    /// The next enqueue sequence to assign.
    pub next_sequence: u64,
    /// The count of non-empty command flushes so far.
    pub boundary: u64,
    /// One outcome per processed command, in processing order.
    pub journal: Vec<Outcome>,
    /// Interventions waiting for their time, by journal sequence.
    pub scheduled: Vec<(u64, Intervention)>,
}

/// Why saved parts do not form a valid world.
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreError {
    /// The toy world's seed differs from the world's.
    SeedMismatch,
    /// The clock fields are inconsistent.
    Clock(InvalidClock),
    /// The toy world breaks an LOD invariant.
    Lod(Vec<LodViolation>),
    /// The event history does not rebuild.
    History(EventError),
    /// The journal or the sequence counter is inconsistent.
    Journal(&'static str),
    /// An illness does not trace to a `life.illness` event for the same
    /// resident (ADR 0021 §7).
    Illness(ws_ecs::EntityId),
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SeedMismatch => f.write_str("the toy world's seed differs from the world's"),
            Self::Clock(e) => write!(f, "{e}"),
            Self::Lod(v) => write!(f, "{} LOD invariants are violated", v.len()),
            Self::History(e) => write!(f, "{e}"),
            Self::Journal(why) => write!(f, "inconsistent journal: {why}"),
            Self::Illness(id) => write!(f, "the illness of {id} has no matching event"),
        }
    }
}

impl std::error::Error for RestoreError {}

impl World {
    /// The next enqueue sequence that will be assigned.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// How many non-empty command flushes have happened.
    #[must_use]
    pub const fn boundary(&self) -> u64 {
        self.boundary
    }

    /// Whether commands are queued but not yet flushed. A world in this
    /// state cannot be saved: a save is taken at a boundary after a flush.
    #[must_use]
    pub fn has_pending_commands(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The canonical state, for saving.
    ///
    /// The pending queue is not part of the parts. Callers check
    /// [`World::has_pending_commands`] first, because a save that dropped
    /// queued commands would lose input.
    #[must_use]
    pub fn to_parts(&self) -> WorldParts {
        WorldParts {
            seed: self.seed,
            clock: self.clock.clone(),
            toy: self.toy.clone(),
            events: self.events.records().to_vec(),
            next_sequence: self.next_sequence,
            boundary: self.boundary,
            journal: self.journal.clone(),
            scheduled: self.scheduled.iter().map(|(s, i)| (*s, *i)).collect(),
        }
    }

    /// Rebuilds a world from saved parts, with no pending commands.
    ///
    /// # Errors
    ///
    /// [`RestoreError`] if the parts are not a valid world. Nothing is
    /// repaired.
    pub fn from_parts(parts: WorldParts) -> Result<Self, RestoreError> {
        let WorldParts {
            seed,
            clock,
            toy,
            events,
            next_sequence,
            boundary,
            journal,
            scheduled,
        } = parts;
        if toy.seed() != seed {
            return Err(RestoreError::SeedMismatch);
        }
        toy.check_lod_invariants(clock.now(), clock.base_step(), false)
            .map_err(RestoreError::Lod)?;
        let known = toy
            .buildings()
            .ids()
            .iter()
            .chain(toy.residents().ids())
            .copied()
            .collect();
        let events = EventStore::from_records(known, &events).map_err(RestoreError::History)?;
        if next_sequence == 0 {
            return Err(RestoreError::Journal("the next sequence is zero"));
        }
        let mut last = 0;
        for o in &journal {
            if o.sequence <= last {
                return Err(RestoreError::Journal("sequences do not increase"));
            }
            if o.sequence >= next_sequence {
                return Err(RestoreError::Journal(
                    "an outcome is at or past the next sequence",
                ));
            }
            if o.tick > clock.steps() {
                return Err(RestoreError::Journal("an outcome is from a future tick"));
            }
            last = o.sequence;
        }
        for (id, ill) in toy.illness().iter() {
            let ok = events.get(ws_events::EventId(ill.root_event)).is_some_and(|e| {
                matches!(
                    e.record.payload,
                    ws_events::Payload::LifeIllness { resident, illness, severity }
                        if resident == id && illness == ill.kind.tag() && severity == ill.severity
                )
            });
            if !ok {
                return Err(RestoreError::Illness(id));
            }
        }
        let mut scheduled_map = std::collections::BTreeMap::new();
        for (seq, i) in scheduled {
            if seq == 0 || seq >= next_sequence || scheduled_map.insert(seq, i).is_some() {
                return Err(RestoreError::Journal(
                    "a scheduled intervention is inconsistent",
                ));
            }
        }
        Ok(Self {
            seed,
            clock,
            toy,
            events,
            pending: VecDeque::new(),
            pending_bytes: 0,
            next_sequence,
            boundary,
            journal,
            scheduled: scheduled_map,
            #[cfg(test)]
            fail_after: None,
            #[cfg(test)]
            fail_history: false,
        })
    }
}
