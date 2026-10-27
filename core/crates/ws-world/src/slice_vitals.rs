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
/// Exactly one retained row per existing resident. Checked living-health mutation only.
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
    /// Checked ADR0034 health update for a retained living resident. No revival or mortality.
    pub fn set_living_health(&mut self, id: EntityId, health: u8) -> bool {
        let Some(v) = self.rows.get_mut(id) else {
            return false;
        };
        if v.life != Life::Alive || !(1..=100).contains(&health) {
            return false;
        }
        v.health = health;
        true
    }
    /// Borrowed canonical rows in entity ID order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &VitalState)> {
        self.rows.iter()
    }
    /// Restores vitals from saved rows, validating exact population roster match.
    pub fn from_rows(
        population: &PopulatedSlice,
        vitals: impl IntoIterator<Item = (EntityId, VitalState)>,
    ) -> Option<Self> {
        let mut rows = Column::new();
        rows.insert_batch(vitals.into_iter().collect()).ok()?;
        if rows.ids() != population.residents().ids() {
            return None;
        }
        Some(Self { rows })
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
    fn checked_health_updates_preserve_sex_life_and_reject_invalid_or_dead_targets() {
        let p = PopulatedSlice::new(42).unwrap();
        let mut values = Vitals::new(&p);
        let id = values.ids()[0];
        let old = *values.get(id).unwrap();
        for health in [1, 55, 100] {
            assert!(values.set_living_health(id, health));
            let v = values.get(id).unwrap();
            assert_eq!(v.health(), health);
            assert_eq!(v.sex(), old.sex());
            assert_eq!(v.life(), Life::Alive);
        }
        let before = values.clone();
        for health in [0, 101, 255] {
            assert!(!values.set_living_health(id, health));
            assert_eq!(values, before);
        }
        assert!(!values.set_living_health(EntityId::spawned(1).unwrap(), 100));
        assert_eq!(values, before);
        *values.rows.get_mut(id).unwrap() = VitalState::checked(old.sex(), 0, Life::Dead).unwrap();
        let dead = values.clone();
        assert!(!values.set_living_health(id, 100));
        assert_eq!(values, dead);
    }
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
    #[test]
    fn from_rows_roundtrips_and_validates_roster() {
        let p = PopulatedSlice::new(42).unwrap();
        let mut original = Vitals::new(&p);
        let id = original.ids()[0];
        original.set_living_health(id, 85);
        let rows: Vec<_> = original.iter().map(|(id, v)| (id, *v)).collect();
        let restored = Vitals::from_rows(&p, rows).expect("valid rows match roster");
        assert_eq!(restored, original);
        assert_eq!(restored.state_hash(), original.state_hash());

        // Empty rows fail.
        assert!(Vitals::from_rows(&p, []).is_none());
        // Missing row fails.
        let missing = original.iter().skip(1).map(|(id, v)| (id, *v));
        assert!(Vitals::from_rows(&p, missing).is_none());
        // Alien entity ID fails.
        let mut alien: Vec<_> = original.iter().map(|(id, v)| (id, *v)).collect();
        alien[0].0 = EntityId::spawned(999).unwrap();
        assert!(Vitals::from_rows(&p, alien).is_none());
    }
}
