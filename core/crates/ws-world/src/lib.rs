//! World model of the Vertical Slice: the real Bay Ridge block.
//!
//! [`slice::SliceWorld`] imports the four ADR 0023 buildings as an integer
//! structural baseline; [`slice::population`] and [`slice::vitals`] generate
//! its residents. [`ids`] holds the procedural identities (ADR 0016),
//! [`environment`] the site's calendar sky (ADR 0030) and [`slice_render`]
//! the immutable block meshes (ADR 0031).
//!
//! The Technical Prototype toy world was deleted by [ADR 0040].
//!
//! [ADR 0040]: https://github.com/ndabney562/world-simulator/blob/main/docs/adr/0040-delete-toy-world-and-old-formats.md

pub mod environment;
pub mod ids;
pub mod slice;
pub mod slice_render;

pub use ids::ResidentAddress;
pub use ws_rng;
