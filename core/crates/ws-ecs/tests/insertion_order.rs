//! Insertion history never affects a column's observable state or its
//! iteration order (ADR 0016 §2, #7 acceptance criterion).

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use ws_ecs::{Column, EntityId, join};

/// A fixed set of entities with components, in an arbitrary order.
fn entities() -> Vec<(EntityId, (u32, f32))> {
    (0..200u64)
        .map(|i| {
            let hash = i.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (i << 40);
            (EntityId::procedural(hash), (i as u32, i as f32 * 0.25))
        })
        .collect()
}

/// Fisher-Yates with a small fixed LCG, so every permutation is the same
/// on every run.
fn shuffled<T: Clone>(items: &[T], seed: u64) -> Vec<T> {
    let mut v = items.to_vec();
    let mut x = seed.wrapping_mul(2).wrapping_add(1);
    for i in (1..v.len()).rev() {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let j = ((x >> 33) % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

fn canonical(c: &Column<(u32, f32)>) -> Vec<(u64, u32, u32)> {
    c.iter()
        .map(|(id, (a, b))| (id.raw(), *a, b.to_bits()))
        .collect()
}

#[test]
fn many_insertion_orders_give_identical_columns() {
    let items = entities();
    let mut reference = Column::new();
    for (id, c) in &items {
        reference.insert(*id, *c).unwrap();
    }
    let expected = canonical(&reference);
    assert!(
        expected.windows(2).all(|w| w[0].0 < w[1].0),
        "ascending IDs"
    );
    for seed in 0..100 {
        let order = shuffled(&items, seed);
        let mut one_by_one = Column::new();
        for (id, c) in &order {
            one_by_one.insert(*id, *c).unwrap();
        }
        assert_eq!(one_by_one, reference, "single inserts, order {seed}");
        let mut batched = Column::new();
        batched.insert_batch(order.clone()).unwrap();
        assert_eq!(batched, reference, "one batch, order {seed}");
        // Mixed: half in a batch, the rest one by one in reverse.
        let (first, rest) = order.split_at(order.len() / 2);
        let mut mixed = Column::new();
        mixed.insert_batch(first.to_vec()).unwrap();
        for (id, c) in rest.iter().rev() {
            mixed.insert(*id, *c).unwrap();
        }
        assert_eq!(canonical(&mixed), expected, "mixed, order {seed}");
    }
}

#[test]
fn removal_history_does_not_matter_either() {
    let items = entities();
    let mut reference = Column::new();
    reference.insert_batch(items[..150].to_vec()).unwrap();
    for seed in 0..20 {
        // Insert everything, then remove the last 50 in a shuffled order.
        let mut c = Column::new();
        c.insert_batch(shuffled(&items, seed)).unwrap();
        for (id, _) in shuffled(&items[150..], seed + 1000) {
            c.remove(id).unwrap();
        }
        assert_eq!(c, reference);
        let mut d = Column::new();
        d.insert_batch(items.clone()).unwrap();
        let doomed: Vec<EntityId> = shuffled(&items[150..], seed)
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(d.remove_batch(&doomed).len(), 50);
        assert_eq!(d, reference);
    }
}

#[test]
fn joins_are_independent_of_insertion_order() {
    let items = entities();
    let build = |seed| {
        let mut a = Column::new();
        let mut b = Column::new();
        for (id, (x, _)) in shuffled(&items, seed) {
            a.insert(id, x).unwrap();
            if x % 3 != 0 {
                b.insert(id, x * 2).unwrap();
            }
        }
        join(&a, &b)
            .map(|(id, x, y)| (id.raw(), *x, *y))
            .collect::<Vec<_>>()
    };
    let expected = build(0);
    assert_eq!(expected.len(), 133);
    for seed in 1..30 {
        assert_eq!(build(seed), expected);
    }
}

fn config() -> Config {
    Config {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x5753_5f45_4353_0001),
        failure_persistence: None,
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// A column behaves like an ordered map: same contents and order as a
    /// `BTreeMap` after any sequence of inserts and removes.
    #[test]
    fn column_matches_an_ordered_map(ops in proptest::collection::vec((any::<bool>(), 0u64..64, any::<u32>()), 0..300)) {
        let mut col = Column::new();
        let mut map = std::collections::BTreeMap::new();
        for (insert, k, v) in ops {
            let id = EntityId::procedural(k << 2);
            if insert {
                let fresh = !map.contains_key(&id);
                prop_assert_eq!(col.insert(id, v).is_ok(), fresh);
                map.entry(id).or_insert(v);
            } else {
                prop_assert_eq!(col.remove(id), map.remove(&id));
            }
        }
        let got: Vec<_> = col.iter().map(|(i, v)| (i, *v)).collect();
        let want: Vec<_> = map.into_iter().collect();
        prop_assert_eq!(got, want);
    }
}
