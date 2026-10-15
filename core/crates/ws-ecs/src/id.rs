//! `EntityId`: the persistent identity of every simulated entity (ADR 0016
//! §1).

use core::fmt;
use core::num::NonZeroU64;

/// Bit position of the two namespace bits (bits 63–62).
const NAMESPACE_SHIFT: u32 = 62;
/// The low 62 bits that carry the namespace's payload.
const PAYLOAD_MASK: u64 = (1 << NAMESPACE_SHIFT) - 1;
const PROCEDURAL_BITS: u64 = 0b01;
const SPAWNED_BITS: u64 = 0b10;

/// Which ID namespace an [`EntityId`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Namespace {
    /// Bits `01`: derived from a generation address (Tier 1).
    Procedural,
    /// Bits `10`: allocated from a counter in canonical state.
    Spawned,
}

/// Why a raw value or counter is not a valid [`EntityId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdError {
    /// The value is zero, which is never an ID.
    Zero,
    /// The namespace bits are `00` or `11`, which are reserved.
    ReservedNamespace,
    /// A spawn counter does not fit in 62 bits.
    CounterExhausted,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Zero => "zero is not a valid entity ID",
            Self::ReservedNamespace => "entity ID uses a reserved namespace",
            Self::CounterExhausted => "spawn counter does not fit in 62 bits",
        })
    }
}

impl std::error::Error for IdError {}

/// A 64-bit entity identity: two namespace bits and a 62-bit payload.
///
/// | Bits 63–62 | Namespace | Low 62 bits |
/// |---|---|---|
/// | `01` | [`Namespace::Procedural`] | The top 62 bits of a `ws-rng` derivation |
/// | `10` | [`Namespace::Spawned`] | A spawn counter, never reused |
/// | `00`, `11` | Reserved | Never issued; `0` is never valid |
///
/// It is the only identity that components, events, saves, and FFI
/// snapshots use. Outside the core it is opaque: it travels as its raw
/// `u64` ([`EntityId::raw`]), little-endian when serialised. The order of
/// IDs is the order of their raw values, which is the canonical iteration
/// order of every store.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(NonZeroU64);

impl EntityId {
    /// The procedural ID for a derivation result: namespace `01` and the
    /// top 62 bits of `hash`. Never fails.
    #[must_use]
    pub const fn procedural(hash: u64) -> Self {
        let raw = (PROCEDURAL_BITS << NAMESPACE_SHIFT) | (hash >> 2);
        match NonZeroU64::new(raw) {
            Some(v) => Self(v),
            None => unreachable!(),
        }
    }

    /// The spawned ID for a counter value: namespace `10` and the counter.
    ///
    /// # Errors
    ///
    /// [`IdError::CounterExhausted`] if `counter` does not fit in 62 bits.
    pub const fn spawned(counter: u64) -> Result<Self, IdError> {
        if counter > PAYLOAD_MASK {
            return Err(IdError::CounterExhausted);
        }
        match NonZeroU64::new((SPAWNED_BITS << NAMESPACE_SHIFT) | counter) {
            Some(v) => Ok(Self(v)),
            None => unreachable!(),
        }
    }

    /// The ID with this raw value, as stored in a save or received over the
    /// FFI.
    ///
    /// # Errors
    ///
    /// [`IdError::Zero`] for 0 and [`IdError::ReservedNamespace`] for
    /// namespace bits `00` or `11`.
    pub const fn from_raw(raw: u64) -> Result<Self, IdError> {
        let Some(v) = NonZeroU64::new(raw) else {
            return Err(IdError::Zero);
        };
        match raw >> NAMESPACE_SHIFT {
            PROCEDURAL_BITS | SPAWNED_BITS => Ok(Self(v)),
            _ => Err(IdError::ReservedNamespace),
        }
    }

    /// The raw 64-bit value, the ID's only external representation.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0.get()
    }

    /// The namespace.
    #[must_use]
    pub const fn namespace(self) -> Namespace {
        if self.raw() >> NAMESPACE_SHIFT == PROCEDURAL_BITS {
            Namespace::Procedural
        } else {
            Namespace::Spawned
        }
    }

    /// The low 62 bits: the derivation bits or the spawn counter.
    #[must_use]
    pub const fn payload(self) -> u64 {
        self.raw() & PAYLOAD_MASK
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityId({:016x})", self.raw())
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.raw())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_ids_use_namespace_01_and_the_top_62_bits() {
        let id = EntityId::procedural(u64::MAX);
        assert_eq!(id.raw(), 0x7fff_ffff_ffff_ffff);
        assert_eq!(id.namespace(), Namespace::Procedural);
        let id = EntityId::procedural(0);
        assert_eq!(id.raw(), 0x4000_0000_0000_0000);
        assert_eq!(EntityId::procedural(0b11).raw(), 0x4000_0000_0000_0000);
        assert_eq!(
            EntityId::procedural(0x8000_0000_0000_0000).payload(),
            1 << 61
        );
    }

    #[test]
    fn spawned_ids_use_namespace_10_and_the_counter() {
        let id = EntityId::spawned(0).unwrap();
        assert_eq!(id.raw(), 0x8000_0000_0000_0000);
        assert_eq!(id.namespace(), Namespace::Spawned);
        assert_eq!(EntityId::spawned(5).unwrap().payload(), 5);
        assert_eq!(
            EntityId::spawned((1 << 62) - 1).unwrap().raw(),
            0xbfff_ffff_ffff_ffff
        );
        assert_eq!(EntityId::spawned(1 << 62), Err(IdError::CounterExhausted));
    }

    #[test]
    fn from_raw_rejects_zero_and_reserved_namespaces() {
        assert_eq!(EntityId::from_raw(0), Err(IdError::Zero));
        assert_eq!(EntityId::from_raw(1), Err(IdError::ReservedNamespace));
        assert_eq!(
            EntityId::from_raw(0x3fff_ffff_ffff_ffff),
            Err(IdError::ReservedNamespace)
        );
        assert_eq!(
            EntityId::from_raw(0xc000_0000_0000_0000),
            Err(IdError::ReservedNamespace)
        );
        assert_eq!(
            EntityId::from_raw(u64::MAX),
            Err(IdError::ReservedNamespace)
        );
        for raw in [
            0x4000_0000_0000_0000,
            0x7fff_ffff_ffff_ffff,
            0x8000_0000_0000_0001,
        ] {
            assert_eq!(EntityId::from_raw(raw).unwrap().raw(), raw);
        }
    }

    #[test]
    fn raw_round_trips_and_orders_by_value() {
        let a = EntityId::procedural(10 << 2);
        let b = EntityId::spawned(0).unwrap();
        assert!(a < b, "procedural IDs sort before spawned IDs");
        assert_eq!(EntityId::from_raw(a.raw()), Ok(a));
        assert_eq!(a.to_string(), "400000000000000a");
        assert_eq!(format!("{a:?}"), "EntityId(400000000000000a)");
    }

    #[test]
    fn option_of_an_id_is_eight_bytes() {
        assert_eq!(size_of::<Option<EntityId>>(), 8);
    }
}
