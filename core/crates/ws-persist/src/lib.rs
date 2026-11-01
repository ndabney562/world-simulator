//! Save and load for the Vertical Slice ([ADR 0020], [ADR 0036]).
//!
//! A save is one SQLite file written at a tick boundary. Loading restores
//! canonical state directly and never replays the operation log; derived
//! indices are rebuilt, every invariant the running simulation keeps is
//! checked, and the restored simulation's state, operation and event digests
//! must equal those recorded in the header.
//!
//! Only the current save format is read; every older format, including the
//! deleted toy world's, is refused ([ADR 0040]).
//!
//! Records are written by a hand-written canonical codec (`codec`), one
//! schema version per record kind. See the crate README for the table
//! layout and the invariants.
//!
//! [ADR 0020]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0020-save-format-and-versioning.md
//! [ADR 0036]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0036-slice-persistence.md
//! [ADR 0040]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0040-delete-toy-world-and-old-formats.md

mod codec;
mod error;
mod header;
mod store;

pub use error::{LoadError, SaveError};
pub use header::{EFFECT_VOCABULARY, FORMAT_VERSION, Header, MODEL_KIND_SLICE};
pub use store::{
    load_slice, load_slice_parts, read_header, save_slice, save_slice_background, save_slice_parts,
};
