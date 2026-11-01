//! Simulation core of the World Simulator: the Vertical Slice.
//!
//! A [`slice::SliceSimulation`] is created from a [`slice::SliceConfig`] and
//! advanced in fixed base steps by its [`Clock`] (TP-004). Operations,
//! history and causality follow ADRs 0019 and 0027 onwards; read-only logs
//! never drive domain logic. The Technical Prototype toy world was deleted
//! by [ADR 0040].
//!
//! The simulation never reads the wall clock, the environment, or the
//! filesystem; its state is a pure function of its configuration and the
//! sequence of calls made on it. Host-measured elapsed time and the start
//! time are inputs ([ADR 0014] §7).
//!
//! [ADR 0014]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0014-time-representation.md
//! [ADR 0040]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0040-delete-toy-world-and-old-formats.md

mod clock;
mod environment;
pub mod slice;
pub use ws_events;

pub use clock::{Clock, InvalidClock, TimeScale};
pub use environment::CalendarSky;
pub use slice::{SliceParts, SliceRestoreError};
pub use ws_ecs::EntityId;
pub use ws_time::{CivilError, SimTime, TimeOverflow, UtcDateTime};
pub use ws_world::ResidentAddress;
pub use ws_world::environment::{
    LEAP_SECOND_TABLE_VERSION, NY_POLICY_VERSION, SITE_DATASET_VERSION, SOLAR_VERSION,
};
