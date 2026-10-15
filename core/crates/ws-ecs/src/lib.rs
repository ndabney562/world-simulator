//! Entity identity and canonical component storage for the World Simulator
//! ([ADR 0016] §1–2).
//!
//! - [`EntityId`]: a 64-bit identity with two namespace bits, procedural
//!   (`01`) or spawned (`10`). It is the only persistent identity in the
//!   core; outside it travels as its raw `u64`.
//! - [`Column`]: the components of one type, sorted by `EntityId`. Every
//!   iteration is in ascending ID order, and a column's contents depend only
//!   on the set of `(id, component)` pairs, never on insertion history.
//! - [`join`] and [`join_mut`]: merge-joins over two columns, in ascending
//!   ID order.
//!
//! There is no scheduler and no threading: systems are plain functions that
//! the caller (`ws-sim`) runs in a fixed order. No storage index or handle
//! leaves this crate. This is a foundation crate with no dependencies, so
//! `ws-world` and `ws-events` can name entities.
//!
//! [ADR 0016]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0016-entity-identity-storage-and-randomness.md

mod column;
mod id;

pub use column::{Column, DuplicateEntity, join, join_mut};
pub use id::{EntityId, IdError, Namespace};
