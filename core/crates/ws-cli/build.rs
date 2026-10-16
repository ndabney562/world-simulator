//! Deterministic source/compiler/target identity for same-build replay artifacts.
use std::process::Command;
fn output(program: &str, args: &[&str]) -> String {
    let out = Command::new(program)
        .args(args)
        .output()
        .expect("build identity tool");
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .expect("UTF-8 build identity")
        .trim()
        .to_owned()
}
fn main() {
    // Include the complete tracked+untracked Rust source tree in the build identity;
    // a dirty checkout must not masquerade as the same committed build.
    fn tree(path: &std::path::Path, hash: &mut u64) {
        let mut paths: Vec<_> = std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                tree(&p, hash);
            } else if p.extension().is_some_and(|e| e == "rs" || e == "toml")
                || p.file_name().is_some_and(|n| n == "Cargo.lock")
            {
                for b in p
                    .to_string_lossy()
                    .bytes()
                    .chain(std::fs::read(&p).unwrap())
                {
                    *hash = (*hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
                }
            }
        }
    }
    let mut source = 0xcbf29ce484222325;
    tree(std::path::Path::new(".."), &mut source);
    for file in ["../../Cargo.toml", "../../Cargo.lock"] {
        for b in std::fs::read(file).unwrap() {
            source = (source ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    for key in [
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_FEATURE",
        "HOST",
    ] {
        for b in std::env::var(key).unwrap_or_default().bytes() {
            source = (source ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    println!("cargo:rustc-env=WS_SOURCE={source:016x}");
    println!("cargo:rerun-if-changed=..");
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let sha = output("git", &["rev-parse", "HEAD"]);
    let rustc = output("rustc", &["--version"]);
    println!(
        "cargo:rustc-env=WS_BUILD={sha}|{rustc}|{}|{}",
        std::env::var("TARGET").unwrap(),
        std::env::var("PROFILE").unwrap()
    );
    for path in ["HEAD", "refs/heads"] {
        let file = output("git", &["rev-parse", "--git-path", path]);
        println!("cargo:rerun-if-changed={file}");
    }
}
