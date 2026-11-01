//! The save header: every version the architecture requires, recorded from
//! the first save (D-14, ADR 0020).

/// The only save format this build writes and reads (ADR 0040).
pub const FORMAT_VERSION: u32 = 6;

/// Real-slice model kind (ADR 0036). Model kind 1 was the deleted toy world.
pub const MODEL_KIND_SLICE: u32 = 2;

/// Version of the closed effect vocabulary every slice header records
/// (ADR 0021 §7).
pub const EFFECT_VOCABULARY: u32 = 1;

/// The header row of a save. Cheap to read without loading the simulation
/// ([`crate::read_header`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Save format version.
    pub format_version: u32,
    /// The build that wrote the save: crate version, plus the git commit
    /// when the build provided one in `WS_GIT_COMMIT`.
    pub build: String,
    /// The world seed.
    pub seed: u64,
    /// Dataset version the world was generated from.
    pub dataset_version: u64,
    /// Generator version, as recorded in the world's first event.
    pub generator_version: u32,
    /// Rules version, as recorded in the world's first event.
    pub rules_version: u32,
    /// Simulation time at the save, microseconds since the epoch.
    pub sim_time_us: i64,
    /// Physical tick at the save.
    pub tick: u64,
    /// Zero in slice saves (a toy-world walking speed field).
    pub walk_speed_bits: u64,
    /// Zero in slice saves (a toy-world waiting field).
    pub walk_wait_steps: u32,
    /// The slice's resident count.
    pub population_min: u32,
    /// The slice's resident count.
    pub population_max: u32,
    /// `SliceSimulation::state_hash` at the save. Tier 2: a corruption check
    /// within one build and platform, not a cross-build guarantee.
    pub state_hash: u64,
    /// `SliceSimulation::operation_digest` at the save.
    pub journal_digest: u64,
    /// `EventStore::digest` at the save: detects a lost or altered event
    /// or causal link.
    pub events_digest: u64,
    /// Version of the intervention effect vocabulary the writer knew
    /// (ADR 0021 §7): [`EFFECT_VOCABULARY`].
    pub effect_vocabulary: u32,
    /// Model kind: always [`MODEL_KIND_SLICE`].
    pub model_kind: u32,
}

/// The build identifier written to new saves.
pub(crate) fn build_id() -> String {
    match option_env!("WS_GIT_COMMIT") {
        Some(commit) => format!("{}+{commit}", env!("CARGO_PKG_VERSION")),
        None => env!("CARGO_PKG_VERSION").to_owned(),
    }
}
