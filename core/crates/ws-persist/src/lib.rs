//! Save and load for the World Simulator ([ADR 0020], [ADR 0025]).
//!
//! A save is one SQLite file written at a tick boundary. Loading restores
//! canonical state directly and never replays the journal; derived indices
//! are rebuilt, every invariant the running world keeps is checked, and the
//! restored world's state hash must equal the hash recorded in the header.
//!
//! Format 3 regenerates the version-pinned procedural baseline from the
//! `baseline` table and applies canonical structural deltas (`structural_overrides`,
//! `notables`, `tombstones`, `spawned`) before layering runtime state.
//!
//! Records are written by a hand-written canonical codec (`codec`), one
//! schema version per record kind. See the crate README for the table
//! layout and the invariants.
//!
//! [ADR 0020]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0020-save-format-and-versioning.md
//! [ADR 0025]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0025-baseline-plus-deltas-saves.md

mod codec;
mod error;
mod header;
mod store;

pub use codec::{COMPONENT_BUILDING, COMPONENT_RESIDENT};
pub use error::{LoadError, SaveError};
pub use header::{
    BaselineRecord, BaselineRegistry, FORMAT_VERSION, Header, MODEL_KIND_SLICE, MODEL_KIND_TOY,
    PURPOSE_BASELINE_DIGEST,
};
pub use store::{
    load, load_slice, load_slice_parts, load_with_registry, migrate_to_v3,
    migrate_to_v3_with_registry, read_baseline, read_header, save, save_slice,
    save_slice_background, save_slice_parts, save_with_registry,
};
