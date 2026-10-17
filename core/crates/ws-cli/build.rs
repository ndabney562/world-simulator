//! Deterministic source/compiler/target identity for same-build replay artifacts.
mod build_identity;
use build_identity::try_output;
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
    let sha = try_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "nogit".into());
    let rustc = try_output("rustc", &["--version"]).expect("Rust compiler identity");
    println!(
        "cargo:rustc-env=WS_BUILD={sha}|{rustc}|{}|{}",
        std::env::var("TARGET").unwrap(),
        std::env::var("PROFILE").unwrap()
    );
    for path in ["HEAD", "refs/heads"] {
        if let Some(file) = try_output("git", &["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={file}");
        }
    }
}
