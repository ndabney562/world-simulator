//! Standard-library-only headless runner for the Vertical Slice (ADR 0040).
use ws_sim::{
    EntityId, SimTime,
    slice::{CameraInterest, Operation, Point, SliceConfig, SliceSimulation},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
/// Runs the command-line entry point; argument errors never silently choose defaults.
pub fn main_entry(args: Vec<String>) -> Result<()> {
    if args.is_empty() || args[0] == "--help" {
        println!(
            "ws-cli run --seed S --ticks N [--hash-every K] [--script FILE]\nws-cli save --seed S --ticks N --out FILE [run options]\nws-cli load --file FILE [--ticks N] [--hash-every K]"
        );
        return Ok(());
    }
    let mode = &args[0];
    require(
        ["run", "save", "load"].contains(&mode.as_str()),
        "unknown mode",
    )?;
    let mut opts = std::collections::BTreeMap::new();
    let mut i = 1;
    while i < args.len() {
        require(i + 1 < args.len(), "option needs a value")?;
        require(
            opts.insert(args[i].as_str(), args[i + 1].as_str())
                .is_none(),
            "duplicate option",
        )?;
        i += 2;
    }
    require(
        opts.keys().all(|k| match mode.as_str() {
            "run" | "save" => {
                ["--seed", "--ticks", "--hash-every", "--script"].contains(k)
                    || (mode == "save" && *k == "--out")
            }
            "load" => ["--file", "--ticks", "--hash-every"].contains(k),
            _ => false,
        }),
        "unknown option/mode",
    )?;
    if mode == "load" {
        return load_and_continue(&opts);
    }
    let out = opts.get("--out").copied();
    require(
        (mode == "save") == out.is_some(),
        "save requires --out, and only save accepts it",
    )?;
    if let Some(out) = out {
        require(
            !std::path::Path::new(out).exists(),
            "the --out file already exists; a save is never silently overwritten",
        )?;
    }
    let seed = opts.get("--seed").ok_or("run requires --seed")?.parse()?;
    let ticks: u64 = opts.get("--ticks").ok_or("run requires --ticks")?.parse()?;
    let every: u64 = opts.get("--hash-every").unwrap_or(&"1").parse()?;
    require(every > 0, "hash interval must be positive")?;
    run_slice(seed, ticks, every, opts.get("--script").copied(), out)
}
fn run_slice(
    seed: u64,
    ticks: u64,
    every: u64,
    script_path: Option<&str>,
    out: Option<&str>,
) -> Result<()> {
    let config = SliceConfig::calendar_aligned_v1(seed, SimTime::EPOCH).or_else(|_| {
        SliceConfig::calendar_aligned_v1(seed, SimTime::from_micros_since_epoch(1_000_000_000))
    })?;
    let mut sim = SliceSimulation::new(config)?;

    if let Some(path) = script_path {
        let text = std::fs::read_to_string(path)?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["step", n] => {
                    let count: u64 = n.parse()?;
                    sim.apply(Operation::Steps(count))?;
                }
                ["afflict", who, severity, rest @ ..] => {
                    let resident = if *who == "first" {
                        sim.population().residents().ids()[0]
                    } else {
                        EntityId::from_raw(who.parse()?)?
                    };
                    let sev: u32 = severity.parse()?;
                    let duration_us = if rest.is_empty() {
                        10_000_000
                    } else {
                        rest[0].parse()?
                    };
                    sim.apply(Operation::Afflict {
                        resident,
                        severity: sev,
                        duration_us,
                    })?;
                }
                ["camera", east, north, radius] => {
                    sim.apply(Operation::CameraInterest(CameraInterest {
                        center: Point {
                            east: east.parse()?,
                            north: north.parse()?,
                        },
                        radius_mm: radius.parse()?,
                    }))?;
                }
                _ => return Err(format!("invalid script line: {line}").into()),
            }
        }
    }

    while sim.clock().steps() < ticks {
        sim.step()?;
        if sim.clock().steps().is_multiple_of(every) {
            println!(
                "tick={} state={:016x}",
                sim.clock().steps(),
                sim.state_hash()
            );
        }
    }

    println!(
        "complete tick={} state={:016x} journal={} events={}",
        sim.clock().steps(),
        sim.state_hash(),
        sim.operation_digest(),
        sim.events().records().len()
    );

    if let Some(out) = out {
        ws_persist::save_slice(&sim, std::path::Path::new(out))?;
        println!(
            "saved {out} tick={} state={:016x}",
            sim.clock().steps(),
            sim.state_hash()
        );
    }
    Ok(())
}

/// `load`: restores a slice save, then optionally continues it for `--ticks` more.
fn load_and_continue(opts: &std::collections::BTreeMap<&str, &str>) -> Result<()> {
    let path = std::path::Path::new(opts.get("--file").ok_or("load requires --file")?);
    let more: u64 = opts.get("--ticks").unwrap_or(&"0").parse()?;
    let every: u64 = opts.get("--hash-every").unwrap_or(&"1").parse()?;
    require(every > 0, "hash interval must be positive")?;

    require(
        ws_persist::read_header(path)?.is_slice(),
        "only Vertical Slice saves load (ADR 0040)",
    )?;
    let mut sim = ws_persist::load_slice(path)?;
    println!(
        "loaded tick={} state={:016x} journal={} events={}",
        sim.clock().steps(),
        sim.state_hash(),
        sim.operation_digest(),
        sim.events().records().len()
    );
    let end = sim
        .clock()
        .steps()
        .checked_add(more)
        .ok_or("tick budget overflows")?;
    while sim.clock().steps() < end {
        sim.step()?;
        if sim.clock().steps().is_multiple_of(every) {
            println!(
                "tick={} state={:016x}",
                sim.clock().steps(),
                sim.state_hash()
            );
        }
    }
    Ok(())
}
