//! Tier 2 replay (testing-policy.md §1, #7 acceptance criteria): the same
//! configuration gives the same per-tick state hashes, in this process and
//! in a separate one; different seeds differ.

use std::process::Command;

use ws_sim::{SimTime, World, WorldConfig};

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
}
