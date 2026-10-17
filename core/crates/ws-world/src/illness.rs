//! The `Illness` component (ADR 0021): one column keyed by resident ID,
//! independent of the L1 and L2 stores, so it survives demotion, promotion,
//! saving and loading by construction.

/// The kind of an illness. Closed and versioned with the effect vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum IllnessKind {
    /// A fever: the only kind in effect vocabulary v1.
    Fever = 0,
}

impl IllnessKind {
    /// The stable wire and save tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// The kind with `tag`, if any.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Fever),
            _ => None,
        }
    }
}

/// The lowest accepted severity.
pub const SEVERITY_MIN: u8 = 1;
/// The highest accepted severity.
pub const SEVERITY_MAX: u8 = 10;

/// An ill resident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Illness {
    /// What the resident has.
    pub kind: IllnessKind,
    /// How bad it is, [`SEVERITY_MIN`]..=[`SEVERITY_MAX`].
    pub severity: u8,
    /// The raw ID of the `life.illness` event that explains this state, so
    /// "why is this agent ill?" is one lookup (ADR 0021 §3).
    pub root_event: u64,
}
