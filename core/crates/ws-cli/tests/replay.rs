//! End-to-end replay runs the real CLI in separate processes.
use std::{fs, process::Command};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ws-cli"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn mixed_commands_record_replay_and_corruption_fail_closed() {
    let root = std::env::temp_dir().join(format!("ws-cli-replay-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let script = root.join("commands.txt");
    let tape = root.join("run.wsjr");
    let corrupt = root.join("bad.wsjr");
    let _ = fs::remove_file(&tape);
    fs::write(&script,"noop\nscale paused\nflush\nnoop\nflush\nscale 10\nflush\nlod 9223372036854775809 2\nflush\nlod all 2\nstep 7000\nlod all 1\nflush\nscale 1\n").unwrap();
    let out = cli(&[
        "run",
        "--seed",
        "19",
        "--ticks",
        "14000",
        "--hash-every",
        "100",
        "--script",
        script.to_str().unwrap(),
        "--journal",
        tape.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = String::from_utf8(out.stdout).unwrap();
    assert!(run.contains("complete tick=14000"));
    let out = cli(&["replay", "--journal", tape.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let verified = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        run.lines()
            .last()
            .unwrap()
            .strip_prefix("complete ")
            .unwrap(),
        verified.trim().strip_prefix("verified ").unwrap()
    );
    let valid = fs::read(&tape).unwrap();
    let header = 4 + u32::from_le_bytes(valid[..4].try_into().unwrap()) as usize;
    // Header, frame, input, observation, checkpoint, footer, truncation and trailing-data faults.
    for at in [8, header + 8, header + 32 + 32 + 16, valid.len() - 8] {
        let mut b = valid.clone();
        b[at] ^= 1;
        fs::write(&corrupt, b).unwrap();
        assert!(
            !cli(&["replay", "--journal", corrupt.to_str().unwrap()])
                .status
                .success(),
            "corruption at {at}"
        );
    }
    for end in [
        0,
        3,
        header - 1,
        header + 31,
        valid.len() - 104,
        valid.len() - 1,
    ] {
        fs::write(&corrupt, &valid[..end]).unwrap();
        assert!(
            !cli(&["replay", "--journal", corrupt.to_str().unwrap()])
                .status
                .success(),
            "truncation at {end}"
        );
    }
    let mut b = valid;
    b.push(0);
    fs::write(&corrupt, b).unwrap();
    assert!(
        !cli(&["replay", "--journal", corrupt.to_str().unwrap()])
            .status
            .success()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn save_then_load_in_separate_processes_agree_and_never_overwrite() {
    let root = std::env::temp_dir().join(format!("ws-cli-save-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("commands.txt");
    let save = root.join("world.wssave");
    fs::write(
        &script,
        "scale 10\nflush\nlod all 2\nstep 300\nlod all 1\nflush\n",
    )
    .unwrap();
    let run = |args: &[&str]| cli(args);
    let out = run(&[
        "save",
        "--seed",
        "23",
        "--ticks",
        "500",
        "--hash-every",
        "500",
        "--script",
        script.to_str().unwrap(),
        "--out",
        save.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let saved = String::from_utf8(out.stdout).unwrap();
    let state = saved
        .lines()
        .find_map(|l| l.strip_prefix("saved "))
        .and_then(|l| l.split("state=").nth(1))
        .unwrap()
        .trim()
        .to_owned();

    let out = run(&["load", "--file", save.to_str().unwrap(), "--ticks", "20"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let loaded = String::from_utf8(out.stdout).unwrap();
    assert!(
        loaded.contains(&format!("loaded tick=500 state={state}")),
        "{loaded}"
    );
    assert!(loaded.contains("tick=520 state="));

    // A save is never silently replaced, and a flag from the wrong mode fails.
    let again = run(&[
        "save",
        "--seed",
        "23",
        "--ticks",
        "10",
        "--out",
        save.to_str().unwrap(),
    ]);
    assert!(!again.status.success());
    assert!(
        !run(&["run", "--seed", "1", "--ticks", "1", "--out", "x"])
            .status
            .success()
    );
    assert!(
        !run(&["save", "--seed", "1", "--ticks", "1"])
            .status
            .success()
    );
    // A file that is not a save fails closed.
    fs::write(root.join("junk"), b"not a save").unwrap();
    assert!(
        !run(&["load", "--file", root.join("junk").to_str().unwrap()])
            .status
            .success()
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_intervention_records_and_replays_to_identical_hashes() {
    let root = std::env::temp_dir().join(format!("ws-cli-intervene-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script = root.join("commands.txt");
    let tape = root.join("run.wsjr");
    // An immediate, a timed (3 s of simulated time, i.e. tick 30) and a
    // rejected intervention, across a level change.
    fs::write(
        &script,
        "step 10\nafflict building first 4\nafflict building first 7 at 3000000\nafflict building first 99\nlod all 2\nstep 800\nlod all 1\nflush\n",
    )
    .unwrap();
    let out = cli(&[
        "run",
        "--seed",
        "19",
        "--ticks",
        "3000",
        "--hash-every",
        "500",
        "--script",
        script.to_str().unwrap(),
        "--journal",
        tape.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = String::from_utf8(out.stdout).unwrap();
    let out = cli(&["replay", "--journal", tape.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let verified = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        run.lines()
            .last()
            .unwrap()
            .strip_prefix("complete ")
            .unwrap(),
        verified.trim().strip_prefix("verified ").unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn slice_save_then_load_in_separate_processes_agree() {
    let root = std::env::temp_dir().join(format!("ws-cli-slice-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let save = root.join("slice.wssave");
    let script = root.join("slice.txt");
    fs::write(
        &script,
        "step 10\nafflict first 5\ncamera 20000 30000 50000\nstep 20\n",
    )
    .unwrap();

    let out = cli(&[
        "save",
        "--slice",
        "--seed",
        "42",
        "--ticks",
        "50",
        "--hash-every",
        "10",
        "--script",
        script.to_str().unwrap(),
        "--out",
        save.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let saved = String::from_utf8(out.stdout).unwrap();
    let state = saved
        .lines()
        .find_map(|l| l.strip_prefix("saved "))
        .and_then(|l| l.split("state=").nth(1))
        .unwrap()
        .trim()
        .to_owned();

    let out = cli(&["load", "--file", save.to_str().unwrap(), "--ticks", "20"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let loaded = String::from_utf8(out.stdout).unwrap();
    assert!(
        loaded.contains(&format!("loaded tick=50 state={state}")),
        "{loaded}"
    );
    assert!(loaded.contains("tick=70 state="));
}
