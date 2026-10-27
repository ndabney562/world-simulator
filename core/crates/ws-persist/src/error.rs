//! Typed save and load errors.

use core::fmt;

use ws_sim::{RestoreError, SliceRestoreError, WorldError};

/// Why a world could not be saved. Nothing is left at the target path.
#[derive(Debug)]
pub enum SaveError {
    /// Commands are queued but not flushed. Saves are taken at a boundary
    /// after a flush, so no queued input is lost.
    PendingCommands,
    /// The world holds a record the format cannot represent.
    Unsaveable(&'static str),
    /// The file system refused.
    Io(std::io::Error),
    /// SQLite refused.
    Sqlite(rusqlite::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PendingCommands => f.write_str("commands are queued; flush them before saving"),
            Self::Unsaveable(why) => write!(f, "cannot save: {why}"),
            Self::Io(e) => write!(f, "save failed: {e}"),
            Self::Sqlite(e) => write!(f, "save failed: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<rusqlite::Error> for SaveError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

/// Why a save could not be loaded. No partial world is ever returned.
#[derive(Debug)]
pub enum LoadError {
    /// The file system refused, or the file does not exist.
    Io(std::io::Error),
    /// SQLite refused.
    Sqlite(rusqlite::Error),
    /// The file is not a World Simulator save.
    NotASave,
    /// The save was written by a format this build does not know, usually a
    /// newer one. Nothing in the file is read.
    UnsupportedVersion {
        /// The version the file declares.
        found: u32,
        /// The newest version this build reads.
        supported: u32,
    },
    /// The save's pinned baseline dataset or content digest is not available
    /// in the local baseline registry (ADR 0025 §3).
    MissingBaseline {
        /// Dataset identifier recorded in the save.
        dataset_id: u32,
        /// Dataset version recorded in the save.
        dataset_version: u64,
        /// Expected 32-byte content SHA-256 recorded in the save.
        sha256: [u8; 32],
    },
    /// The save's baseline declares a generator, rules, or identity-policy
    /// version (or collision overrides) that this build does not support
    /// (ADR 0025 §1, §3).
    UnsupportedBaseline {
        /// Which baseline field is unsupported.
        field: &'static str,
        /// The version or count recorded in the save.
        version: u64,
    },
    /// A table or the header is missing, or a record does not decode.
    Corrupt {
        /// Where.
        table: &'static str,
        /// What is wrong.
        reason: &'static str,
    },
    /// The toy-world rows do not fit together (a repeated ID or a missing
    /// home building).
    Toy(WorldError),
    /// The records decode but do not form a valid world.
    Invalid(RestoreError),
    /// The records decode but do not form a valid slice simulation.
    Slice(SliceRestoreError),
    /// The restored world does not hash to a value recorded in the header.
    HashMismatch {
        /// Which digest: `"state"`, `"journal"` or `"events"`.
        what: &'static str,
        /// The digest recorded at save time.
        stored: u64,
        /// The digest of the restored world.
        computed: u64,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "load failed: {e}"),
            Self::Sqlite(e) => write!(f, "load failed: {e}"),
            Self::NotASave => f.write_str("the file is not a World Simulator save"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "save format version {found} is not supported (this build reads up to {supported})"
            ),
            Self::MissingBaseline {
                dataset_id,
                dataset_version,
                sha256,
            } => {
                write!(
                    f,
                    "missing baseline dataset {dataset_id} version {dataset_version} (sha256 "
                )?;
                for b in sha256 {
                    write!(f, "{b:02x}")?;
                }
                f.write_str(")")
            }
            Self::UnsupportedBaseline { field, version } => {
                write!(f, "unsupported baseline {field} version {version}")
            }
            Self::Corrupt { table, reason } => write!(f, "corrupt save ({table}): {reason}"),
            Self::Toy(e) => write!(f, "the save's entities do not fit together: {e}"),
            Self::Invalid(e) => write!(f, "the save is not a valid world: {e}"),
            Self::Slice(e) => write!(f, "the save is not a valid slice simulation: {e}"),
            Self::HashMismatch {
                what,
                stored,
                computed,
            } => write!(
                f,
                "{what} digest mismatch: the header says {stored:#018x}, the loaded world is {computed:#018x}"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<rusqlite::Error> for LoadError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<SliceRestoreError> for LoadError {
    fn from(e: SliceRestoreError) -> Self {
        Self::Slice(e)
    }
}
