//! End-to-end tests run the real CLI in separate processes.
use std::{fs, process::Command};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ws-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ws-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}
const SCRIPT: &str = "step 10\nafflict first 5\ncamera 20000 30000 50000\nstep 20\n";

#[test]
fn slice_runs_in_separate_processes_print_identical_hashes() {
    let root = scratch("tier2");
    let script = root.join("slice.txt");
    fs::write(&script, SCRIPT).unwrap();
    let args = [
        "run",
        "--seed",
        "42",
        "--ticks",
        "60",
        "--hash-every",
        "10",
        "--script",
        script.to_str().unwrap(),
    ];
    let first = cli(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = cli(&args);
    assert!(second.status.success());
    let printed = String::from_utf8(first.stdout.clone()).unwrap();
    assert!(printed.contains("complete tick=60"), "{printed}");
    assert_eq!(printed.matches("state=").count(), 4, "{printed}");
    assert_eq!(first.stdout, second.stdout);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn slice_save_then_load_in_separate_processes_agree_and_never_overwrite() {
    let root = scratch("slice-save");
    let save = root.join("slice.wssave");
    let script = root.join("slice.txt");
    fs::write(&script, SCRIPT).unwrap();
    let args = [
        "save",
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
    ];
    let out = cli(&args);
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
    let before = fs::read(&save).unwrap();
    let again = cli(&args);
    assert!(
        !again.status.success(),
        "an existing save is never overwritten"
    );
    assert_eq!(fs::read(&save).unwrap(), before);

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
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn removed_toy_modes_and_options_are_refused() {
    for args in [
        &["replay", "--journal", "x.wsjr"][..],
        &["run", "--seed", "1", "--ticks", "1", "--journal", "x.wsjr"],
        &["run", "--slice", "--seed", "1", "--ticks", "1"],
    ] {
        let out = cli(args);
        assert!(!out.status.success(), "{args:?}");
    }
}
