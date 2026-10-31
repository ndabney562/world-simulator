//! The save header and baseline identity: every version the architecture
//! requires, recorded from the first save (D-14, ADR 0020, ADR 0025).

use std::collections::BTreeMap;

use ws_world::ws_rng::{Purpose, SipHasher24};
use ws_world::{Dataset, TOY_DATASET_ID};

use crate::error::LoadError;

/// The newest save format this build writes and reads.
pub const FORMAT_VERSION: u32 = 5;

/// Model kind indicator in save format 4 (ADR 0036).
pub const MODEL_KIND_TOY: u32 = 1;
/// Real-slice model kind.
pub const MODEL_KIND_SLICE: u32 = 2;

/// Purpose key for the canonical baseline record digest (ADR 0025 §1, §3).
pub const PURPOSE_BASELINE_DIGEST: Purpose = Purpose::new("ws-persist/baseline-digest");

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
    /// `World::state_hash` or `SliceSimulation::state_hash` at the save. Tier 2: a corruption check within
    /// one build and platform, not a cross-build guarantee.
    pub state_hash: u64,
    /// `World::journal_digest` or `SliceSimulation::operation_digest` at the save.
    pub journal_digest: u64,
    /// `EventStore::digest` at the save: detects a lost or altered event
    /// or causal link.
    pub events_digest: u64,
    /// Version of the intervention effect vocabulary the writer knew
    /// (ADR 0021 §7); zero in format-1 saves, which predate interventions.
    pub effect_vocabulary: u32,
    /// Model kind: 1 for Toy World, 2 for Slice Simulation. In format 1-3, defaults to 1.
    pub model_kind: u32,
}

impl Header {
    /// Whether this save is a real Bay Ridge slice simulation.
    pub fn is_slice(&self) -> bool {
        self.model_kind == MODEL_KIND_SLICE
            || self.dataset_version == ws_world::slice::DATASET_VERSION
    }

    /// Whether this save is a toy world simulation.
    pub fn is_toy(&self) -> bool {
        !self.is_slice()
    }
}

/// The version-pinned baseline identity and configuration stored in the
/// `baseline` table of a format-3 save (ADR 0025 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineRecord {
    /// The world seed.
    pub seed: u64,
    /// Stable dataset identifier ([`TOY_DATASET_ID`] for the toy dataset).
    pub dataset_id: u32,
    /// Dataset version.
    pub dataset_version: u64,
    /// Full 32-byte content SHA-256 of the canonical dataset bytes.
    pub dataset_sha256: [u8; 32],
    /// Version of the building structural generator.
    pub building_generator_version: u32,
    /// Version of the resident identity generator.
    pub resident_generator_version: u32,
    /// Version of the rules.
    pub rules_version: u32,
    /// Identity collision policy version (1 = fail-on-collision).
    pub identity_policy_version: u32,
    /// Minimum residents per building (generation parameter).
    pub population_min: u32,
    /// Maximum residents per building (generation parameter).
    pub population_max: u32,
    /// Walking speed in metres per second, as raw `f64` bits.
    pub walk_speed_bits: u64,
    /// Steps an agent waits at a building before leaving.
    pub walk_wait_steps: u32,
    /// Initial world start time, microseconds since the epoch.
    pub start_time_us: i64,
    /// Number of buildings in the regenerated baseline.
    pub building_count: u32,
    /// Number of residents in the regenerated baseline.
    pub resident_count: u32,
    /// `ToyWorld::structural_digest` of the regenerated baseline.
    pub structural_digest: u64,
    /// Canonical digest of this baseline record and its collision overrides.
    pub baseline_digest: u64,
    /// Canonical collision overrides (empty under fail-on-collision policy 1).
    pub collision_overrides: Vec<u8>,
}

impl BaselineRecord {
    /// Computes the canonical `baseline_digest` over all identity,
    /// configuration, manifest, and collision-override fields.
    #[must_use]
    pub fn compute_digest(&self) -> u64 {
        let mut h = SipHasher24::new(0, PURPOSE_BASELINE_DIGEST.value());
        h.write_words(&[self.seed, u64::from(self.dataset_id), self.dataset_version]);
        for chunk in self.dataset_sha256.chunks_exact(8) {
            let word = u64::from_le_bytes(chunk.try_into().expect("8-byte chunk"));
            h.write_u64(word);
        }
        h.write_words(&[
            u64::from(self.building_generator_version),
            u64::from(self.resident_generator_version),
            u64::from(self.rules_version),
            u64::from(self.identity_policy_version),
            u64::from(self.population_min),
            u64::from(self.population_max),
            self.walk_speed_bits,
            u64::from(self.walk_wait_steps),
            self.start_time_us as u64,
            u64::from(self.building_count),
            u64::from(self.resident_count),
            self.structural_digest,
            self.collision_overrides.len() as u64,
        ]);
        for b in &self.collision_overrides {
            h.write_u64(u64::from(*b));
        }
        h.finish()
    }
}

/// Trusted local registry of version-pinned baseline datasets (ADR 0025 §3).
///
/// The loader selects the exact `(dataset_id, dataset_version)` recorded in a
/// save and verifies its full 32-byte content SHA-256 before regenerating any
/// baseline entities.
#[derive(Debug, Clone, Default)]
pub struct BaselineRegistry {
    datasets: BTreeMap<(u32, u64), Dataset>,
}

impl BaselineRegistry {
    /// An empty registry with no datasets registered.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            datasets: BTreeMap::new(),
        }
    }

    /// A registry populated with the built-in toy dataset
    /// (`(TOY_DATASET_ID, TOY_DATASET_VERSION)`).
    #[must_use]
    pub fn with_builtin() -> Self {
        let mut reg = Self::new();
        let toy = Dataset::toy().expect("built-in toy dataset is valid");
        reg.register(TOY_DATASET_ID, toy);
        reg
    }

    /// Registers `dataset` under `(dataset_id, dataset.version)`.
    pub fn register(&mut self, dataset_id: u32, dataset: Dataset) {
        self.datasets.insert((dataset_id, dataset.version), dataset);
    }

    /// Looks up `(dataset_id, dataset_version)` without checking the content
    /// digest.
    #[must_use]
    pub fn get(&self, dataset_id: u32, dataset_version: u64) -> Option<&Dataset> {
        self.datasets.get(&(dataset_id, dataset_version))
    }

    /// Resolves `(dataset_id, dataset_version)` and verifies that its
    /// [`Dataset::content_sha256`] equals `expected_sha256`.
    ///
    /// # Errors
    ///
    /// [`LoadError::MissingBaseline`] if the dataset is absent or its content
    /// SHA-256 differs.
    pub fn resolve(
        &self,
        dataset_id: u32,
        dataset_version: u64,
        expected_sha256: &[u8; 32],
    ) -> Result<&Dataset, LoadError> {
        let Some(ds) = self.datasets.get(&(dataset_id, dataset_version)) else {
            return Err(LoadError::MissingBaseline {
                dataset_id,
                dataset_version,
                sha256: *expected_sha256,
            });
        };
        if ds.content_sha256() != *expected_sha256 {
            return Err(LoadError::MissingBaseline {
                dataset_id,
                dataset_version,
                sha256: *expected_sha256,
            });
        }
        Ok(ds)
    }
}

/// The build identifier written to new saves.
pub(crate) fn build_id() -> String {
    match option_env!("WS_GIT_COMMIT") {
        Some(commit) => format!("{}+{commit}", env!("CARGO_PKG_VERSION")),
        None => env!("CARGO_PKG_VERSION").to_owned(),
    }
}
