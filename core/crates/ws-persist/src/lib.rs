//! Save and load for the World Simulator ([ADR 0020]).
//!
//! A save is one SQLite file written at a tick boundary. Loading restores
//! canonical state directly and never replays the journal; derived indices
//! are rebuilt, every invariant the running world keeps is checked, and the
//! restored world's state hash must equal the hash recorded in the header.
//!
//! Records are written by a hand-written canonical codec (`codec`), one
//! schema version per record kind. See the crate README for the table
//! layout and the invariants.
//!
//! [ADR 0020]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0020-save-format-and-versioning.md

mod codec;
mod error;
mod header;
mod store;

pub use error::{LoadError, SaveError};
pub use header::{FORMAT_VERSION, Header};
pub use store::{load, read_header, save};
