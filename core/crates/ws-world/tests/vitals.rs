//! Contract tests prepared from ADR0033 and an independent Python oracle.
use ws_world::slice::{
    population::PopulatedSlice,
    vitals::{Life, Sex, VitalState, Vitals},
};
#[test]
fn independent_vitals_golden_preserves_population_one_and_exact_membership() {
    let mut text = String::new();
    let mut tags = [false; 2];
    for seed in [0, 42, u64::MAX] {
        let population = PopulatedSlice::new(seed).unwrap();
        let before = population.clone();
        let values = Vitals::new(&population);
        assert_eq!(population, before);
        assert_eq!(values.ids(), population.residents().ids());
        assert!(
            values
                .get(population.world().buildings().ids()[0])
                .is_none()
        );
        for &id in values.ids() {
            let v = values.get(id).unwrap();
            tags[v.sex() as usize] = true;
            assert_eq!(v.health(), 100);
            assert_eq!(v.life(), Life::Alive);
            text.push_str(&format!(
                "{seed:016x} {:016x} {} {} {}\n",
                id.raw(),
                v.sex() as u8,
                v.health(),
                v.life() as u8
            ));
            assert_eq!(v, Vitals::new(&population).get(id).unwrap());
        }
    }
    assert_eq!(tags, [true, true]);
    assert_eq!(text, include_str!("golden/vitals-v1.txt"));
}
#[test]
fn checked_core_tags_and_health_life_invariants_are_closed() {
    for tag in 0..=255 {
        assert_eq!(Sex::from_tag(tag).is_some(), tag <= 1);
        assert_eq!(Life::from_tag(tag).is_some(), tag <= 1);
    }
    for health in 0..=255 {
        assert_eq!(
            VitalState::checked(Sex::Female, health, Life::Alive).is_some(),
            (1..=100).contains(&health)
        );
        assert_eq!(
            VitalState::checked(Sex::Male, health, Life::Dead).is_some(),
            health == 0
        );
    }
}
