//! The save header: every version the architecture requires, recorded from
//! the first save (D-14, ADR 0020).

/// The newest save format this build writes and reads.
pub const FORMAT_VERSION: u32 = 2;

/// The header row of a save. Cheap to read without loading the world
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
    /// Walking speed in metres per second, as raw `f64` bits.
    pub walk_speed_bits: u64,
    /// Steps an agent waits at a building before leaving.
    pub walk_wait_steps: u32,
    /// Minimum residents per building (a generation input).
    pub population_min: u32,
    /// Maximum residents per building (a generation input).
    pub population_max: u32,
    /// `World::state_hash` at the save. Tier 2: a corruption check within
    /// one build and platform, not a cross-build guarantee.
    pub state_hash: u64,
    /// `World::journal_digest` at the save: detects a lost or altered
    /// journal outcome.
    pub journal_digest: u64,
    /// `EventStore::digest` at the save: detects a lost or altered event
    /// or causal link.
    pub events_digest: u64,
    /// Version of the intervention effect vocabulary the writer knew
    /// (ADR 0021 §7); zero in format-1 saves, which predate interventions.
    pub effect_vocabulary: u32,
}

/// The build identifier written to new saves.
pub(crate) fn build_id() -> String {
    match option_env!("WS_GIT_COMMIT") {
        Some(commit) => format!("{}+{commit}", env!("CARGO_PKG_VERSION")),
        None => env!("CARGO_PKG_VERSION").to_owned(),
    }
}
