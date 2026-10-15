//! A draw is a function of its documented inputs only (ADR 0016 §3).
//!
//! The golden values are Tier 1: they must match exactly on Linux x86-64
//! and macOS arm64. They were cross-checked against an independent
//! reference implementation of SipHash-2-4 and FNV-1a (Python) while
//! writing TP-006. Changing them changes every procedural ID and decision,
//! so it needs Product Owner approval (testing-policy.md §2).

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use ws_rng::{Purpose, Stream, derive};

const GOLDEN: Purpose = Purpose::new("ws-rng/golden");

#[test]
fn golden_derivations_match_exactly() {
    let cases = [
        (0, 0xed7f_3961_4cf3_764f_u64),
        (1, 0x354d_9b70_a787_e29a),
        (0xdead_beef, 0xe5ae_420b_43de_62bb),
        (u64::MAX, 0x3aae_58be_624a_8d0f),
    ];
    for (seed, expected) in cases {
        assert_eq!(derive(seed, GOLDEN, &[1, 2, 3]), expected, "seed {seed:#x}");
    }
}

#[test]
fn golden_stream_draws_match_exactly() {
    let s = Stream::new(42, GOLDEN, &[7]);
    let cases = [
        (0, 0x29da_9e32_b550_400d_u64),
        (1, 0x4de9_7474_b833_c431),
        (2, 0xf075_6507_5a34_186f),
        (1000, 0x1d9a_6124_0964_291c),
        (u64::MAX, 0x50ff_cfe0_4c93_75cb),
    ];
    for (n, expected) in cases {
        assert_eq!(s.draw(n), expected, "draw {n}");
    }
}

#[test]
fn identical_inputs_reproduce_identical_outputs() {
    let p = Purpose::new("ws-rng/test/repeat");
    for seed in [0, 1, 99, u64::MAX] {
        let a = Stream::new(seed, p, &[5, 6]);
        let b = Stream::new(seed, p, &[5, 6]);
        for n in 0..100 {
            assert_eq!(a.draw(n), b.draw(n));
        }
        assert_eq!(derive(seed, p, &[5, 6]), derive(seed, p, &[5, 6]));
    }
}

#[test]
fn different_purpose_labels_are_distinct_domains() {
    let labels = [
        "ws-rng/test/a",
        "ws-rng/test/b",
        "ws-world/agent/next-target",
        "ws-world/id/resident",
        "ws-world/id/building",
    ];
    let mut values = BTreeMap::new();
    for label in labels {
        let p = Purpose::new(label);
        assert!(values.insert(p.value(), label).is_none(), "{label}");
    }
    // Same seed, address, and counter; only the purpose differs.
    let draws: Vec<u64> = labels
        .iter()
        .map(|l| Stream::new(7, Purpose::new(l), &[1]).draw(0))
        .collect();
    for (i, a) in draws.iter().enumerate() {
        for b in &draws[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

/// The draws of a set of entities, computed visiting them in `order`.
fn draws_in_order(seed: u64, order: &[u64]) -> BTreeMap<u64, Vec<u64>> {
    let p = Purpose::new("ws-rng/test/order");
    order
        .iter()
        .map(|&id| {
            let s = Stream::new(seed, p, &[id]);
            (id, (0..8).map(|n| s.draw(n)).collect())
        })
        .collect()
}

#[test]
fn entity_iteration_order_does_not_change_results() {
    let ids: Vec<u64> = (0..64).map(|i| 0x4000_0000_0000_0000 | (i * 977)).collect();
    let forward = draws_in_order(3, &ids);
    let reversed: Vec<u64> = ids.iter().rev().copied().collect();
    assert_eq!(draws_in_order(3, &reversed), forward);
    // A fixed interleaving: even positions, then odd ones.
    let interleaved: Vec<u64> = ids
        .iter()
        .step_by(2)
        .chain(ids.iter().skip(1).step_by(2))
        .copied()
        .collect();
    assert_eq!(draws_in_order(3, &interleaved), forward);
}

#[test]
fn an_unrelated_draw_does_not_change_another_stream() {
    let p = Purpose::new("ws-rng/test/isolation");
    let other = Purpose::new("ws-rng/test/unrelated");
    let a = Stream::new(11, p, &[1]);
    let before: Vec<u64> = (0..10).map(|n| a.draw(n)).collect();
    // Draw heavily from another entity's stream and from another purpose
    // of the same entity, interleaved with the first stream's draws.
    let b = Stream::new(11, p, &[2]);
    let c = Stream::new(11, other, &[1]);
    let mut after = Vec::new();
    for n in 0..10 {
        for m in 0..50 {
            let _ = b.draw(m);
            let _ = c.draw(m);
        }
        after.push(a.draw(n));
    }
    assert_eq!(before, after);
}

fn config() -> Config {
    Config {
        cases: 512,
        rng_seed: RngSeed::Fixed(0x5753_5f52_4e47_0001),
        failure_persistence: None,
        ..Config::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Any change to a single input changes the draw (no input is ignored).
    #[test]
    fn every_input_matters(seed in any::<u64>(), id in any::<u64>(), n in any::<u64>()) {
        let p = Purpose::new("ws-rng/test/inputs");
        let q = Purpose::new("ws-rng/test/inputs2");
        let base = Stream::new(seed, p, &[id]).draw(n);
        prop_assert_ne!(base, Stream::new(seed.wrapping_add(1), p, &[id]).draw(n));
        prop_assert_ne!(base, Stream::new(seed, p, &[id.wrapping_add(1)]).draw(n));
        prop_assert_ne!(base, Stream::new(seed, p, &[id]).draw(n.wrapping_add(1)));
        prop_assert_ne!(base, Stream::new(seed, q, &[id]).draw(n));
        prop_assert_ne!(base, Stream::new(seed, p, &[id, 0]).draw(n));
    }
}
