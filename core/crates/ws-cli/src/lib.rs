//! Standard-library-only headless runner and bounded WSJR debug tape codec.
#[cfg(test)]
#[path = "../build_identity.rs"]
mod build_identity;
use std::io::{Read, Write};
use ws_sim::{
    EntityId, IllnessKind, Lod, SimTime, World, WorldConfig,
    commands::{self, Action, Command, Effect, Intervention, InterventionTarget, IssuedBy, Timing},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CAP: usize = 65_600;
const BUILD: &str = concat!(env!("WS_BUILD"), "|", env!("WS_SOURCE"));
const PLATFORM: &str = std::env::consts::ARCH;
fn u32v(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn u64v(b: &mut Vec<u8>, v: u64) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn n32(b: &[u8], i: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(i..i + 4).ok_or("truncated u32")?.try_into()?,
    ))
}
fn n64(b: &[u8], i: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(i..i + 8).ok_or("truncated u64")?.try_into()?,
    ))
}
fn words(values: impl IntoIterator<Item = u64>) -> Vec<u8> {
    let mut b = Vec::new();
    for v in values {
        u64v(&mut b, v);
    }
    b
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn header(seed: u64) -> Vec<u8> {
    let mut h = Vec::from(*b"WSJR");
    u32v(&mut h, 1);
    u64v(&mut h, seed);
    u64v(&mut h, 0);
    u32v(&mut h, 4);
    u32v(&mut h, 12);
    u64v(&mut h, 100_000);
    u64v(&mut h, ws_sim::TOY_DATASET_VERSION);
    u32v(&mut h, 1);
    u32v(&mut h, 1);
    for s in [BUILD, PLATFORM, std::env::consts::OS] {
        u32v(&mut h, s.len() as u32);
        h.extend_from_slice(s.as_bytes());
    }
    h
}
struct Tape<W: Write> {
    writer: W,
    frames: u64,
    operations: u64,
    journal: usize,
    events: usize,
}
impl<W: Write> Tape<W> {
    fn new(mut writer: W, seed: u64) -> Result<Self> {
        let h = header(seed);
        writer.write_all(&(h.len() as u32).to_le_bytes())?;
        writer.write_all(&h)?;
        Ok(Self {
            writer,
            frames: 0,
            operations: 0,
            journal: 0,
            events: 0,
        })
    }
    fn frame(&mut self, kind: u32, body: &[u8]) -> Result<()> {
        require(body.len() <= CAP - 32, "record cap")?;
        self.frames = self.frames.checked_add(1).ok_or("frame counter")?;
        let mut f = Vec::with_capacity(32);
        u32v(&mut f, (32 + body.len()) as u32);
        u32v(&mut f, 1);
        u32v(&mut f, kind);
        u32v(&mut f, 0);
        u64v(&mut f, self.operations);
        u32v(&mut f, body.len() as u32);
        u32v(&mut f, 0);
        self.writer.write_all(&f)?;
        self.writer.write_all(body)?;
        Ok(())
    }
    fn observations(&mut self, w: &World) -> Result<()> {
        for o in &w.journal()[self.journal..] {
            self.frame(4, &outcome(o)?)?;
        }
        self.journal = w.journal().len();
        for e in &w.events().records()[self.events..] {
            self.frame(5, &event(e))?;
        }
        self.events = w.events().records().len();
        self.frame(6, &words(w.checkpoint()))
    }
    fn operation(&mut self, kind: u32, body: &[u8], w: &World) -> Result<()> {
        self.operations = self.operations.checked_add(1).ok_or("operation counter")?;
        self.frame(kind, body)?;
        self.observations(w)
    }
    fn submit(&mut self, w: &mut World, batch: &[u8]) -> Result<()> {
        commands::validate_wire_batch(batch)?;
        let result = w.submit_bytes(batch);
        let (status, count, first) = match result {
            Ok(a) => (0, a.count, a.first_sequence),
            Err(e) => (e as u32, 0, 0),
        };
        let mut b = Vec::new();
        u32v(&mut b, batch.len() as u32);
        u32v(&mut b, status);
        u32v(&mut b, count);
        u32v(&mut b, 0);
        u64v(&mut b, first);
        u64v(&mut b, 0);
        b.extend_from_slice(batch);
        self.operation(1, &b, w)
    }
    fn flush(&mut self, w: &mut World) -> Result<()> {
        let entry = w.pending().count() as u32;
        let before = w.journal().len();
        let result = w.flush_commands();
        let committed = (w.journal().len() - before) as u32;
        let next = w.pending().next().map_or(0, |p| p.0);
        let status = flush_status(result.err());
        let mut b = Vec::new();
        u32v(&mut b, entry);
        u32v(&mut b, committed);
        u64v(&mut b, next);
        u32v(&mut b, status);
        u32v(&mut b, 0);
        b.extend_from_slice(&words(w.checkpoint()));
        self.operation(2, &b, w)
    }
    fn step(&mut self, w: &mut World) -> Result<()> {
        self.flush(w)?;
        require(
            w.pending().next().is_none(),
            "command flush stopped; tape retains the pending suffix",
        )?;
        let result = w.physical_step();
        let status = step_status(result.err());
        let mut b = Vec::new();
        u32v(&mut b, status);
        u32v(&mut b, 0);
        b.extend_from_slice(&words(w.checkpoint()));
        self.operation(3, &b, w)?;
        require(status == 0, "physical step failed; see recorded status")
    }
    fn finish(mut self, w: &World) -> Result<()> {
        let body = words(
            [
                self.frames + 1,
                self.operations,
                self.journal as u64,
                self.events as u64,
                w.tick(),
            ]
            .into_iter()
            .chain(w.checkpoint()),
        );
        self.frame(7, &body)?;
        self.writer.flush()?;
        Ok(())
    }
}
fn flush_status(e: Option<ws_sim::ControlError>) -> u32 {
    use ws_sim::ControlError::*;
    match e {
        None => 0,
        Some(Resource) => 1,
        Some(Counter) => 2,
        Some(Domain(_)) => 3,
        Some(History(_)) => 4,
        Some(Fence) => 5,
    }
}
fn step_status(e: Option<ws_sim::StepError>) -> u32 {
    use ws_sim::{StepError::*, ws_events::EventError};
    match e {
        None => 0,
        Some(History(EventError::Resource)) => 1,
        Some(Time(_)) => 2,
        Some(World(_)) => 3,
        Some(History(_)) => 4,
        Some(Control(_)) => 5,
    }
}
fn event(e: &ws_events::Event) -> Vec<u8> {
    let mut b = Vec::new();
    e.visit_words(|v| {
        for n in v {
            u64v(&mut b, *n)
        }
    });
    b
}
fn outcome(o: &commands::Outcome) -> Result<Vec<u8>> {
    let (reason, detail) = match o.result {
        Ok(()) => (0, 0),
        Err(commands::Rejection::Schema) => (1, 0),
        Err(commands::Rejection::TooManyNodes) => (2, 0),
        Err(commands::Rejection::Lod(ws_sim::LodError::UnknownNode(id))) => (3, id.raw()),
        Err(commands::Rejection::Lod(ws_sim::LodError::DuplicateNode(id))) => (4, id.raw()),
        Err(commands::Rejection::Lod(ws_sim::LodError::UnsupportedLevel(l))) => (5, l as u64),
        Err(commands::Rejection::Lod(_)) => (6, 0),
        Err(commands::Rejection::UnknownTarget(id)) => (7, id.raw()),
        Err(commands::Rejection::UnsupportedLevel(id)) => (8, id.raw()),
        Err(commands::Rejection::SeverityOutOfRange(v)) => (9, v as u64),
    };
    let mut b = words([
        o.schema_version as u64,
        o.sequence,
        o.tick,
        o.time.as_micros_since_epoch() as u64,
        o.boundary,
        o.effect as u64,
        o.demoted as u64,
        o.promoted as u64,
        reason,
        detail,
    ]);
    b.extend_from_slice(&commands::encode_batch(std::slice::from_ref(&o.command))?);
    Ok(b)
}
fn read_frame(r: &mut impl Read) -> Result<(u32, u64, Vec<u8>)> {
    let mut h = [0u8; 32];
    r.read_exact(&mut h)
        .map_err(|_| "incomplete artifact: missing frame/footer")?;
    let total = n32(&h, 0)? as usize;
    let kind = n32(&h, 8)?;
    let size = n32(&h, 24)? as usize;
    require(
        n32(&h, 4)? == 1 && n32(&h, 12)? == 0 && n32(&h, 28)? == 0,
        "frame schema/flags/reserved",
    )?;
    require(
        (32..=CAP).contains(&total) && size == total - 32,
        "frame size cap/mismatch",
    )?;
    require(
        match kind {
            1 => (32..=65_568).contains(&size),
            2 => size == 56,
            3 => size == 40,
            4 => (104..=12_396).contains(&size),
            5 => (104..=2048).contains(&size) && size.is_multiple_of(8),
            6 => size == 32,
            7 => size == 72,
            _ => false,
        },
        "unknown kind or invalid kind length",
    )?;
    let mut body = vec![0; size];
    r.read_exact(&mut body)
        .map_err(|_| "incomplete artifact: truncated body")?;
    Ok((kind, n64(&h, 16)?, body))
}
/// Strictly verifies a complete same-build/platform artifact by independently executing inputs.
pub fn replay(mut reader: impl Read) -> Result<World> {
    let mut n = [0; 4];
    reader.read_exact(&mut n)?;
    let size = u32::from_le_bytes(n) as usize;
    require((68..=1024).contains(&size), "header length cap")?;
    let mut h = vec![0; size];
    reader.read_exact(&mut h)?;
    require(
        h.get(..4) == Some(b"WSJR") && n32(&h, 4)? == 1,
        "header magic/version",
    )?;
    let seed = n64(&h, 8)?;
    require(
        h == header(seed),
        "incompatible header/configuration/build/platform",
    )?;
    let mut w = World::new(WorldConfig::new(seed, SimTime::EPOCH))?;
    let mut operations = 0u64;
    let mut frames = 0u64;
    let mut observed_journal = 0;
    let mut observed_events = 0;
    loop {
        let (kind, op, b) = read_frame(&mut reader)?;
        frames = frames.checked_add(1).ok_or("frame count exhausted")?;
        if kind == 7 {
            require(op == operations, "footer operation sequence")?;
            let expected = words(
                [
                    frames,
                    operations,
                    observed_journal as u64,
                    observed_events as u64,
                    w.tick(),
                ]
                .into_iter()
                .chain(w.checkpoint()),
            );
            require(b == expected, "completion footer counts/checkpoint")?;
            require(
                observed_journal == w.journal().len()
                    && observed_events == w.events().records().len(),
                "missing observations",
            )?;
            let mut extra = [0];
            require(reader.read(&mut extra)? == 0, "trailing data")?;
            return Ok(w);
        }
        operations = operations
            .checked_add(1)
            .ok_or("operation count exhausted")?;
        require(op == operations, "operation sequence")?;
        match kind {
            1 => {
                let len = n32(&b, 0)? as usize;
                require(
                    len <= commands::MAX_BYTES
                        && b.len() == 32 + len
                        && n32(&b, 12)? == 0
                        && n64(&b, 24)? == 0,
                    "submit metadata/length",
                )?;
                let status = n32(&b, 4)?;
                let batch = &b[32..];
                commands::validate_wire_batch(batch)?;
                let actual = if status == 4 {
                    w.validate_bytes(batch)?;
                    Err(commands::IngressError::Resource)
                } else {
                    w.submit_bytes(batch)
                };
                let meta = match actual {
                    Ok(a) => (0, a.count, a.first_sequence),
                    Err(e) => (e as u32, 0, 0),
                };
                require(
                    meta == (status, n32(&b, 8)?, n64(&b, 16)?),
                    "ingress outcome mismatch",
                )?;
            }
            2 => {
                let entry = n32(&b, 0)?;
                let committed = n32(&b, 4)?;
                let next = n64(&b, 8)?;
                let status = n32(&b, 16)?;
                require(
                    n32(&b, 20)? == 0 && entry == w.pending().count() as u32,
                    "flush entry/reserved",
                )?;
                let before = w.journal().len();
                let actual = if status == 1 {
                    require(
                        committed < entry,
                        "environmental flush fence must leave a suffix",
                    )?;
                    w.flush_prefix(Some(committed))?;
                    1
                } else {
                    flush_status(w.flush_commands().err())
                };
                require(
                    actual == status
                        && committed == (w.journal().len() - before) as u32
                        && next == w.pending().next().map_or(0, |p| p.0),
                    "flush prefix/status mismatch",
                )?;
                require(
                    b[24..] == words(w.checkpoint()),
                    "flush checkpoint mismatch",
                )?;
            }
            3 => {
                require(n32(&b, 4)? == 0, "step reserved")?;
                let status = n32(&b, 0)?;
                require(
                    w.pending().next().is_none(),
                    "physical step requires prior complete flush",
                )?;
                // A zero-commit environmental stop preserves the post-flush world.
                // Time overflow is deterministic and must never be hidden as allocation failure.
                let actual = if status == 1 {
                    w.validate_physical_step()?;
                    1
                } else {
                    step_status(w.physical_step().err())
                };
                require(status == actual, "step status mismatch")?;
                require(b[8..] == words(w.checkpoint()), "step checkpoint mismatch")?;
            }
            _ => return Err("expected operation before observations".into()),
        }
        for o in &w.journal()[observed_journal..] {
            let (k, s, b) = read_frame(&mut reader)?;
            frames = frames.checked_add(1).ok_or("frame count exhausted")?;
            require(
                k == 4 && s == operations && b == outcome(o)?,
                "journal observation mismatch",
            )?;
        }
        observed_journal = w.journal().len();
        for e in &w.events().records()[observed_events..] {
            let (k, s, b) = read_frame(&mut reader)?;
            frames = frames.checked_add(1).ok_or("frame count exhausted")?;
            require(
                k == 5 && s == operations && b == event(e),
                "event observation mismatch",
            )?;
        }
        observed_events = w.events().records().len();
        let (k, s, b) = read_frame(&mut reader)?;
        frames = frames.checked_add(1).ok_or("frame count exhausted")?;
        require(
            k == 6 && s == operations && b == words(w.checkpoint()),
            "operation checkpoint mismatch",
        )?;
    }
}
/// Runs the command-line entry point; argument errors never silently choose defaults.
pub fn main_entry(args: Vec<String>) -> Result<()> {
    if args.is_empty() || args[0] == "--help" {
        println!(
            "ws-cli run --seed S --ticks N [--journal FILE] [--hash-every K] [--script FILE] [--batch FILE]\\nws-cli replay --journal FILE\\nws-cli save --seed S --ticks N --out FILE [run options]\\nws-cli load --file FILE [--ticks N] [--hash-every K]"
        );
        return Ok(());
    }
    let mode = &args[0];
    let mut opts = std::collections::BTreeMap::new();
    for pair in args[1..].chunks(2) {
        require(pair.len() == 2, "option needs a value")?;
        require(
            opts.insert(pair[0].as_str(), pair[1].as_str()).is_none(),
            "duplicate option",
        )?;
    }
    require(
        opts.keys().all(|k| match mode.as_str() {
            "run" | "save" => {
                [
                    "--seed",
                    "--ticks",
                    "--journal",
                    "--hash-every",
                    "--script",
                    "--batch",
                ]
                .contains(k)
                    || (mode == "save" && *k == "--out")
            }
            "replay" => *k == "--journal",
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
    if mode == "replay" {
        let path = opts.get("--journal").ok_or("replay requires --journal")?;
        let w = replay(std::io::BufReader::new(std::fs::File::open(path)?))?;
        println!(
            "verified tick={} state={:016x} journal={} events={}",
            w.tick(),
            w.state_hash(),
            w.journal().len(),
            w.events().records().len()
        );
        return Ok(());
    }
    require(mode == "run" || mode == "save", "unknown mode")?;
    let seed = opts.get("--seed").ok_or("run requires --seed")?.parse()?;
    let ticks: u64 = opts.get("--ticks").ok_or("run requires --ticks")?.parse()?;
    let every: u64 = opts.get("--hash-every").unwrap_or(&"1").parse()?;
    require(every > 0, "hash interval must be positive")?;
    let writer: Box<dyn Write> = if let Some(path) = opts.get("--journal") {
        Box::new(std::io::BufWriter::new(std::fs::File::create_new(path)?))
    } else {
        Box::new(std::io::sink())
    };
    let mut w = World::new(WorldConfig::new(seed, SimTime::EPOCH))?;
    let mut tape = Tape::new(writer, seed)?;
    let run_result = (|| -> Result<()> {
        if let Some(path) = opts.get("--batch") {
            let f = std::fs::File::open(path)?;
            let mut b = Vec::new();
            f.take(65_537).read_to_end(&mut b)?;
            require(b.len() <= 65_536, "batch file exceeds cap")?;
            tape.submit(&mut w, &b)?;
        }
        if let Some(path) = opts.get("--script") {
            let script = std::fs::read_to_string(path)?;
            for line in script.lines() {
                let line = line.split('#').next().unwrap_or("").trim();
                if line.is_empty() {
                    continue;
                }
                let p: Vec<_> = line.split_whitespace().collect();
                match p.as_slice() {
                    ["flush"] => tape.flush(&mut w)?,
                    ["step", n] => {
                        let count = n.parse::<u64>()?;
                        require(
                            w.tick().checked_add(count).is_some_and(|end| end <= ticks),
                            "script steps exceed --ticks budget",
                        )?;
                        for _ in 0..count {
                            tape.step(&mut w)?;
                        }
                    }
                    ["noop"] => submit_action(&mut tape, &mut w, Action::NoOp)?,
                    ["scale", s] => submit_action(
                        &mut tape,
                        &mut w,
                        Action::SetTimeScale(match *s {
                            "paused" => ws_sim::TimeScale::Paused,
                            "1" => ws_sim::TimeScale::X1,
                            "10" => ws_sim::TimeScale::X10,
                            "100" => ws_sim::TimeScale::X100,
                            _ => return Err("unknown scale".into()),
                        }),
                    )?,
                    ["lod", "all", l] => {
                        let lod = parse_lod(l)?;
                        let req = w
                            .toy()
                            .buildings()
                            .ids()
                            .iter()
                            .map(|id| (*id, lod))
                            .collect();
                        submit_action(&mut tape, &mut w, Action::SetNodeLod(req))?;
                    }
                    ["lod", id, l] => {
                        let id = EntityId::from_raw(id.parse()?)?;
                        submit_action(
                            &mut tape,
                            &mut w,
                            Action::SetNodeLod(vec![(id, parse_lod(l)?)]),
                        )?;
                    }
                    ["afflict", kind, who, severity, rest @ ..] => {
                        let id = if *who == "first" {
                            *w.toy().buildings().ids().first().ok_or("no buildings")?
                        } else {
                            EntityId::from_raw(who.parse()?)?
                        };
                        let target = match *kind {
                            "building" => InterventionTarget::ResidentsOf(id),
                            "resident" => InterventionTarget::Entity(id),
                            _ => return Err("afflict needs building or resident".into()),
                        };
                        let timing = match rest {
                            [] => Timing::Now,
                            ["at", micros] => {
                                Timing::At(SimTime::from_micros_since_epoch(micros.parse::<i64>()?))
                            }
                            _ => return Err(format!("invalid script line: {line}").into()),
                        };
                        submit_action(
                            &mut tape,
                            &mut w,
                            Action::Intervene(Intervention {
                                target,
                                effect: Effect::Afflict {
                                    illness: IllnessKind::Fever,
                                    severity: severity.parse()?,
                                },
                                timing,
                            }),
                        )?;
                    }
                    _ => return Err(format!("invalid script line: {line}").into()),
                }
            }
        }
        while w.tick() < ticks {
            tape.step(&mut w)?;
            if w.tick().is_multiple_of(every) {
                println!("tick={} state={:016x}", w.tick(), w.state_hash());
            }
        }
        tape.flush(&mut w)?;
        require(w.pending().next().is_none(), "final command flush failed")?;
        Ok(())
    })();
    tape.finish(&w)?;
    run_result?;
    println!(
        "complete tick={} state={:016x} journal={} events={}",
        w.tick(),
        w.state_hash(),
        w.journal().len(),
        w.events().records().len()
    );
    if let Some(out) = out {
        ws_persist::save(&w, std::path::Path::new(out))?;
        println!(
            "saved {out} tick={} state={:016x}",
            w.tick(),
            w.state_hash()
        );
    }
    Ok(())
}
/// `load`: restores a save, then optionally continues it for `--ticks` more.
fn load_and_continue(opts: &std::collections::BTreeMap<&str, &str>) -> Result<()> {
    let path = opts.get("--file").ok_or("load requires --file")?;
    let more: u64 = opts.get("--ticks").unwrap_or(&"0").parse()?;
    let every: u64 = opts.get("--hash-every").unwrap_or(&"1").parse()?;
    require(every > 0, "hash interval must be positive")?;
    let mut w = ws_persist::load(std::path::Path::new(path))?;
    println!(
        "loaded tick={} state={:016x} journal={} events={}",
        w.tick(),
        w.state_hash(),
        w.journal().len(),
        w.events().records().len()
    );
    let end = w.tick().checked_add(more).ok_or("tick budget overflows")?;
    while w.tick() < end {
        w.step()?;
        if w.tick().is_multiple_of(every) {
            println!("tick={} state={:016x}", w.tick(), w.state_hash());
        }
    }
    Ok(())
}
fn parse_lod(s: &str) -> Result<Lod> {
    match s {
        "1" => Ok(Lod::L1),
        "2" => Ok(Lod::L2),
        _ => Err("LOD must be 1 or 2".into()),
    }
}
fn submit_action<W: Write>(t: &mut Tape<W>, w: &mut World, action: Action) -> Result<()> {
    t.submit(
        w,
        &commands::encode_batch(&[Command::new(IssuedBy::Script, action)])?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slice_lod_tape_event_keeps_explicit_tag9_words() {
        use ws_events::{Actor, Candidate, EventStore, Origin, Payload, Role};
        let building = ws_sim::EntityId::procedural(4);
        let mut c = Candidate::new(
            SimTime::from_micros_since_epoch(1_000_000),
            70,
            0,
            Payload::SliceLodChanged {
                building,
                from: 1,
                to: 3,
                demoted: 2,
                promoted: 0,
            },
            Origin::System(70),
        );
        c.lod = 3;
        c.actors
            .push(Actor {
                id: building,
                role: Role::Subject,
            })
            .unwrap();
        let mut store = EventStore::new(vec![building]);
        store.prepare(10, &[c]).unwrap().commit();
        let b = 0x4000_0000_0000_0001;
        assert_eq!(
            event(&store.records()[0]),
            words([
                1, 1, 10, 1, 1_000_000, 70, 0, 9, 16384, 0, 3, 0, 70, b, 1, 3, 2, 0, 1, b, 0, 0, 0,
                0
            ])
        );
    }
    fn world() -> World {
        World::new(WorldConfig::new(7, SimTime::EPOCH)).unwrap()
    }
    #[test]
    fn journal_observation_has_ten_words_and_exact_command() {
        let mut w = world();
        let command = Command::new(IssuedBy::Test, Action::NoOp);
        w.submit(vec![command.clone()]).unwrap();
        w.flush_commands().unwrap();
        let bytes = outcome(&w.journal()[0]).unwrap();
        assert_eq!(bytes.len(), 80 + 24);
        let expected = [1u64, 1, 0, 0, 1, 0, 0, 0, 0, 0];
        assert_eq!(&bytes[..80], words(expected));
        assert_eq!(commands::decode_batch(&bytes[80..]).unwrap(), vec![command]);
    }
    #[test]
    fn environmental_fences_preserve_pending_retry_and_ordinals() {
        let mut bytes = Vec::new();
        let mut t = Tape::new(&mut bytes, 7).unwrap();
        let mut w = world();
        let c = Command::new(IssuedBy::Test, Action::NoOp);
        let batch = commands::encode_batch(&[c.clone(), c]).unwrap();
        // Structurally valid zero-enqueue resource stop; the following submit retries it.
        w.validate_submission(&commands::decode_batch(&batch).unwrap())
            .unwrap();
        let mut b = Vec::new();
        for n in [batch.len() as u32, 4, 0, 0] {
            u32v(&mut b, n);
        }
        u64v(&mut b, 0);
        u64v(&mut b, 0);
        b.extend_from_slice(&batch);
        t.operation(1, &b, &w).unwrap();
        t.submit(&mut w, &batch).unwrap();
        // Zero-prefix stop consumes no boundary ordinal, then a partial stop commits A.
        for count in [0, 1] {
            let r = w.flush_prefix(Some(count)).unwrap();
            let mut b = Vec::new();
            u32v(&mut b, r.entry);
            u32v(&mut b, r.committed);
            u64v(&mut b, r.next);
            u32v(&mut b, 1);
            u32v(&mut b, 0);
            b.extend_from_slice(&words(w.checkpoint()));
            t.operation(2, &b, &w).unwrap();
        }
        t.flush(&mut w).unwrap();
        assert_eq!(
            w.journal().iter().map(|o| o.boundary).collect::<Vec<_>>(),
            vec![1, 2]
        );
        let mut b = Vec::new();
        u32v(&mut b, 1);
        u32v(&mut b, 0);
        b.extend_from_slice(&words(w.checkpoint()));
        t.operation(3, &b, &w).unwrap();
        t.step(&mut w).unwrap();
        t.finish(&w).unwrap();
        let reproduced = replay(bytes.as_slice()).unwrap();
        assert_eq!(reproduced, w);
        // A modified fence must disagree with retained next-sequence/outcome/checkpoints.
        let mut changed = bytes;
        let mut at = 4 + n32(&changed, 0).unwrap() as usize;
        while n32(&changed, at + 8).unwrap() != 2 {
            at += n32(&changed, at).unwrap() as usize;
        }
        changed[at + 36] = 1;
        assert!(replay(changed.as_slice()).is_err());
    }
    #[test]
    fn frame_fixture_is_exact_little_endian_and_rejects_before_body_read() {
        let mut b = Vec::new();
        let mut t = Tape {
            writer: &mut b,
            frames: 0,
            operations: 9,
            journal: 0,
            events: 0,
        };
        t.frame(6, &[0; 32]).unwrap();
        assert_eq!(
            &b[..32],
            &[
                64, 0, 0, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0,
                0, 0, 0, 0, 0
            ]
        );
        assert_eq!(read_frame(&mut b.as_slice()).unwrap().0, 6);
        for (offset, value) in [(0, 65_601u32), (4, 2), (8, 99), (12, 1), (24, 33), (28, 1)] {
            let mut corrupt = b.clone();
            corrupt[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(read_frame(&mut corrupt.as_slice()).is_err());
        }
    }
    #[test]
    fn maximum_batch_and_record_round_trip_and_forged_lengths_fail() {
        let mut w = world();
        let id = w.toy().buildings().ids()[0];
        let mut c = vec![
            Command::new(
                IssuedBy::Test,
                Action::SetNodeLod(vec![(id, Lod::L2); 1024])
            );
            5
        ];
        c.push(Command::new(
            IssuedBy::Test,
            Action::SetTimeScale(ws_sim::TimeScale::X1),
        ));
        c.extend(vec![Command::new(IssuedBy::Test, Action::NoOp); 248]);
        let batch = commands::encode_batch(&c).unwrap();
        assert_eq!(batch.len(), 65_536);
        let mut bytes = Vec::new();
        let mut t = Tape::new(&mut bytes, 7).unwrap();
        t.submit(&mut w, &batch).unwrap();
        t.flush(&mut w).unwrap();
        assert!(w.journal()[0].result.is_err());
        t.finish(&w).unwrap();
        assert_eq!(replay(bytes.as_slice()).unwrap(), w);
        let at = 4 + n32(&bytes, 0).unwrap() as usize;
        assert_eq!(n32(&bytes, at).unwrap(), 65_600);
        for (offset, value) in [
            (at, 65_601u32),
            (at + 24, 65_567),
            (at + 32, 65_537),
            (at + 44, 1),
        ] {
            let mut corrupt = bytes.clone();
            corrupt[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(replay(corrupt.as_slice()).is_err());
        }
        let mut oversize = batch;
        oversize.push(0);
        assert_eq!(
            commands::decode_batch(&oversize),
            Err(commands::IngressError::Capacity)
        );
    }
    #[test]
    fn empty_and_max_count_batches_replay() {
        for commands in [
            vec![],
            vec![Command::new(IssuedBy::Test, Action::NoOp); 1024],
        ] {
            let mut w = world();
            let mut bytes = Vec::new();
            let mut t = Tape::new(&mut bytes, 7).unwrap();
            t.submit(&mut w, &commands::encode_batch(&commands).unwrap())
                .unwrap();
            t.flush(&mut w).unwrap();
            t.finish(&w).unwrap();
            assert_eq!(replay(bytes.as_slice()).unwrap(), w);
        }
    }
}
