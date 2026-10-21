//! ADR0033 retained resident vitals; independent of population1 and LOD.
use super::population::PopulatedSlice;
use core::num::NonZeroU64;
use ws_ecs::{Column, EntityId};
use ws_rng::{Purpose, SipHasher24, Stream, below};

/// Independent initial-vitals policy.
pub const VERSION: u32 = 1;
/// Independent sex draw, never inferred from names.
pub const SEX: Purpose = Purpose::new("ws-world/slice/vitals/sex/v1");
/// Vitals digest domain.
pub const STATE: Purpose = Purpose::new("ws-world/slice/vitals-state/v1");
/// Explicit small synthetic baseline categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Sex {
    /// Female.
    Female = 0,
    /// Male.
    Male = 1,
}
impl Sex {
    /// Decode a closed version1 tag.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Female),
            1 => Some(Self::Male),
            _ => None,
        }
    }
}
/// Explicit retained life status; no mortality dynamics in this increment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Life {
    /// Living resident.
    Alive = 0,
    /// Dead; requires zero health.
    Dead = 1,
}
impl Life {
    /// Decode a closed version1 tag.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Alive),
            1 => Some(Self::Dead),
            _ => None,
        }
    }
}
/// Checked authoritative attributes, retained independently of render/detail state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VitalState {
    sex: Sex,
    health: u8,
    life: Life,
}
impl VitalState {
    /// Living health is1..100; dead health is0.
    pub fn checked(sex: Sex, health: u8, life: Life) -> Option<Self> {
        let valid = match life {
            Life::Alive => (1..=100).contains(&health),
            Life::Dead => health == 0,
        };
        valid.then_some(Self { sex, health, life })
    }
    /// Stored sex category.
    pub const fn sex(self) -> Sex {
        self.sex
    }
    /// Stored coarse health quantity,0..100.
    pub const fn health(self) -> u8 {
        self.health
    }
    /// Stored life status.
    pub const fn life(self) -> Life {
        self.life
    }
}
/// Exactly one retained row per existing resident. No public mutation API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vitals {
    rows: Column<VitalState>,
}
impl Vitals {
    /// Initialize once from the existing identity roster; never changes population1.
    pub fn new(population: &PopulatedSlice) -> Self {
        let seed = population.baseline().structural.seed;
        let mut rows = Column::new();
        rows.insert_batch(
            population
                .residents()
                .ids()
                .iter()
                .map(|&id| {
                    let tag = below(
                        Stream::new(seed, SEX, &[id.raw()]).draw(0),
                        NonZeroU64::new(2).unwrap(),
                    ) as u8;
                    (
                        id,
                        VitalState::checked(Sex::from_tag(tag).unwrap(), 100, Life::Alive).unwrap(),
                    )
                })
                .collect(),
        )
        .expect("validated unique population roster");
        Self { rows }
    }
    /// Canonical retained resident IDs.
    pub fn ids(&self) -> &[EntityId] {
        self.rows.ids()
    }
    /// Borrow a stored value; buildings/unknown IDs have no vitals.
    pub fn get(&self, id: EntityId) -> Option<&VitalState> {
        self.rows.get(id)
    }
    /// Canonical versioned integer digest; no allocation or randomness.
    pub fn state_hash(&self) -> u64 {
        let mut hash = SipHasher24::new(0, STATE.value());
        hash.write_u64(u64::from(VERSION));
        for (id, v) in self.rows.iter() {
            hash.write_words(&[id.raw(), v.sex as u64, u64::from(v.health), v.life as u64]);
        }
        hash.finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_field_participates_in_digest_without_changing_identity() {
        let p = PopulatedSlice::new(42).unwrap();
        let original = Vitals::new(&p);
        let id = original.ids()[0];
        let old = *original.get(id).unwrap();
        for changed in [
            VitalState::checked(
                if old.sex() == Sex::Female {
                    Sex::Male
                } else {
                    Sex::Female
                },
                100,
                Life::Alive,
            )
            .unwrap(),
            VitalState::checked(old.sex(), 99, Life::Alive).unwrap(),
            VitalState::checked(old.sex(), 0, Life::Dead).unwrap(),
            // Test the life digest word independently. This deliberately
            // unchecked private test value can never be constructed publicly.
            VitalState {
                life: Life::Dead,
                ..old
            },
        ] {
            let mut values = original.clone();
            *values.rows.get_mut(id).unwrap() = changed;
            assert_ne!(values.state_hash(), original.state_hash());
            assert_eq!(values.ids(), original.ids());
        }
        let mut values = original.clone();
        let v = values.rows.remove(id).unwrap();
        values
            .rows
            .insert(EntityId::spawned(1).unwrap(), v)
            .unwrap();
        assert_ne!(values.state_hash(), original.state_hash());
        assert_ne!(SEX.value(), STATE.value());
    }
}
