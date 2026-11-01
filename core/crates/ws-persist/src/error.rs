//! Typed save and load errors.

use core::fmt;

use ws_sim::SliceRestoreError;

/// Why a simulation could not be saved. Nothing is left at the target path.
#[derive(Debug)]
pub enum SaveError {
    /// The simulation holds a record the format cannot represent.
    Unsaveable(&'static str),
    /// The file system refused.
    Io(std::io::Error),
    /// SQLite refused.
    Sqlite(rusqlite::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
    /// The save was written in a format this build does not read: an older
    /// one (ADR 0040) or a newer one. Nothing else in the file is read.
    UnsupportedVersion {
        /// The version the file declares.
        found: u32,
        /// The only version this build reads.
        supported: u32,
    },
    /// A table or the header is missing, or a record does not decode.
    Corrupt {
        /// Where.
        table: &'static str,
        /// What is wrong.
        reason: &'static str,
    },
    /// The records decode but do not form a valid slice simulation.
    Slice(SliceRestoreError),
    /// The restored simulation does not hash to a value recorded in the header.
    HashMismatch {
        /// Which digest: `"state"`, `"journal"` or `"events"`.
        what: &'static str,
        /// The digest recorded at save time.
        stored: u64,
        /// The digest of the restored simulation.
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
                "save format version {found} is not supported (this build reads only version {supported})"
            ),
            Self::Corrupt { table, reason } => write!(f, "corrupt save ({table}): {reason}"),
            Self::Slice(e) => write!(f, "the save is not a valid slice simulation: {e}"),
            Self::HashMismatch {
                what,
                stored,
                computed,
            } => write!(
                f,
                "{what} digest mismatch: the header says {stored:#018x}, the loaded simulation is {computed:#018x}"
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
