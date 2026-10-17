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
