//! Tier 2 replay (testing-policy.md §1, #7 acceptance criteria): the same
//! configuration gives the same per-tick state hashes, in this process and
//! in a separate one; different seeds differ. With scripted LOD requests
//! (ADR 0017 §14, `replay_with_transitions`) as well as without.

use std::process::Command;

use ws_sim::{EntityId, Lod, SimTime, World, WorldConfig};

fn world(seed: u64) -> World {
    World::new(WorldConfig::new(seed, SimTime::EPOCH)).unwrap()
}

fn per_tick_hashes(seed: u64, ticks: usize) -> Vec<u64> {
    let mut w = world(seed);
    (0..ticks)
        .map(|_| {
            w.step().unwrap();
            w.state_hash()
        })
        .collect()
}

/// The scripted request before tick `k`, if any: on roughly one tick in
/// twenty, a pseudo-random subset of buildings moves to L1 or L2.
fn request(w: &World, k: u64) -> Option<Vec<(EntityId, Lod)>> {
    let x = ws_rng::hash(7, 11, &[k]);
    x.is_multiple_of(20).then(|| subset(w, x))
}

/// The buildings selected by the bits of `x`, all moving to one level
/// chosen by another bit, listed in descending ID order so the world must
/// sort the request.
fn subset(w: &World, x: u64) -> Vec<(EntityId, Lod)> {
    let level = if x >> 8 & 1 == 1 { Lod::L2 } else { Lod::L1 };
    let mut req: Vec<_> = w
        .toy()
        .buildings()
        .ids()
        .iter()
        .enumerate()
        .filter(|(i, _)| x >> (16 + i % 32) & 1 == 1)
        .map(|(_, id)| (*id, level))
        .collect();
    req.reverse();
    req
}

fn per_tick_hashes_with_transitions(seed: u64, ticks: usize) -> Vec<u64> {
    let mut w = world(seed);
    (0..ticks as u64)
        .map(|k| {
            if let Some(req) = request(&w, k) {
                w.set_lod(&req).unwrap();
            }
            w.step().unwrap();
            w.state_hash()
        })
        .collect()
}

/// One number summarising a run's per-tick hashes.
fn digest(hashes: &[u64]) -> u64 {
    ws_rng::hash(1, 2, hashes)
}

#[test]
fn same_seed_same_per_tick_hashes_for_10000_ticks() {
    let a = per_tick_hashes(2026, 10_000);
    assert_eq!(a, per_tick_hashes(2026, 10_000));
    let b = per_tick_hashes(2027, 10_000);
    assert!(
        a.iter().zip(&b).all(|(x, y)| x != y),
        "different seeds differ at every tick"
    );
}

#[test]
fn replay_with_transitions() {
    let a = per_tick_hashes_with_transitions(2026, 10_000);
    assert_eq!(a, per_tick_hashes_with_transitions(2026, 10_000));
    // The transitions are real: the runs leave the all-L1 trajectory, and
    // both levels are populated somewhere along the way.
    assert_ne!(a, per_tick_hashes(2026, 10_000));
    let mut w = world(2026);
    let (mut saw_l1, mut saw_l2) = (false, false);
    for k in 0..10_000 {
        if let Some(req) = request(&w, k) {
            w.set_lod(&req).unwrap();
        }
        w.step().unwrap();
        w.check_lod_invariants(false).unwrap();
        saw_l1 |= !w.toy().walks().is_empty();
        saw_l2 |= !w.toy().background().is_empty();
    }
    assert!(saw_l1 && saw_l2);
}

#[test]
fn host_elapsed_with_transitions_matches_step() {
    let mut a = world(6);
    let mut b = world(6);
    let hosts = [250_000, 33_333, 1_000_000, 16_667, 99_999, 7_000_000];
    for (k, host) in hosts.iter().cycle().take(60).enumerate() {
        let req = subset(&a, ws_rng::hash(7, 12, &[k as u64]));
        a.set_lod(&req).unwrap();
        b.set_lod(&req).unwrap();
        let taken = a.advance_by_host_elapsed(*host).unwrap();
        for _ in 0..taken {
            b.step().unwrap();
        }
        assert_eq!(a.tick(), b.tick());
        assert_eq!(a.state_hash(), b.state_hash());
    }
    assert_eq!(a.toy(), b.toy());
}

#[test]
fn host_elapsed_ticks_run_the_same_systems_as_step() {
    let mut a = world(5);
    let mut b = world(5);
    let mut taken = 0;
    for host in [250_000, 33_333, 1_000_000, 16_667, 99_999, 7_000_000] {
        taken += a.advance_by_host_elapsed(host).unwrap();
    }
    for _ in 0..taken {
        b.step().unwrap();
    }
    assert_eq!(a.tick(), b.tick());
    assert_eq!(a.toy(), b.toy());
    assert_eq!(a.state_hash(), b.state_hash());
}

const CHILD_ENV: &str = "WS_SIM_REPLAY_CHILD";
const PROCESS_SEED: u64 = 31_337;
const PROCESS_TICKS: usize = 3_000;

/// Runs only inside the child process started by
/// `separate_process_reproduces_the_hashes`: prints the digest.
#[test]
fn replay_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    println!(
        "digest={:016x}",
        digest(&per_tick_hashes(PROCESS_SEED, PROCESS_TICKS))
    );
    println!(
        "lod_digest={:016x}",
        digest(&per_tick_hashes_with_transitions(
            PROCESS_SEED,
            PROCESS_TICKS
        ))
    );
}

#[test]
fn separate_process_reproduces_the_hashes() {
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .args(["--exact", "replay_child", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    // libtest may print the test's name on the same line as its output.
    let (_, hex) = stdout
        .split_once("digest=")
        .expect("child printed a digest");
    let child = u64::from_str_radix(&hex[..16], 16).unwrap();
    assert_eq!(child, digest(&per_tick_hashes(PROCESS_SEED, PROCESS_TICKS)));
    let (_, hex) = stdout
        .split_once("lod_digest=")
        .expect("child printed a digest with transitions");
    let child = u64::from_str_radix(&hex[..16], 16).unwrap();
    assert_eq!(
        child,
        digest(&per_tick_hashes_with_transitions(
            PROCESS_SEED,
            PROCESS_TICKS
        ))
    );
}
