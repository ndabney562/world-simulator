//! The SQLite container: schema, writing, and reading (ADR 0020).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use ws_ecs::EntityId;
use ws_events::{EventId, Payload};
use ws_sim::{World, WorldParts};
use ws_world::ToyParts;

use crate::codec::{self, CodecError, Link};
use crate::error::{LoadError, SaveError};
use crate::header::{FORMAT_VERSION, Header, build_id};

/// `PRAGMA application_id` of a World Simulator save: ASCII "WSAV".
const APPLICATION_ID: i32 = 0x5753_4156;

/// Schema version of every record kind in format 1. Each row carries its
/// own, so one kind can move to a new version without the others.
const RECORD_SCHEMA: u32 = 1;

/// Tables added by format 2 (ADR 0021 §7); a format-1 file lacks them and
/// loads with none of their rows.
const TABLES_V2: [&str; 2] = ["illness", "scheduled"];

const TABLES: [&str; 9] = [
    "header",
    "clock",
    "buildings",
    "residents",
    "l1_state",
    "l2_state",
    "journal",
    "events",
    "event_links",
];

const SCHEMA: &str = "
CREATE TABLE header (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format_version INTEGER NOT NULL,
    build TEXT NOT NULL,
    seed INTEGER NOT NULL,
    dataset_version INTEGER NOT NULL,
    generator_version INTEGER NOT NULL,
    rules_version INTEGER NOT NULL,
    sim_time_us INTEGER NOT NULL,
    tick INTEGER NOT NULL,
    walk_speed_bits INTEGER NOT NULL,
    walk_wait_steps INTEGER NOT NULL,
    population_min INTEGER NOT NULL,
    population_max INTEGER NOT NULL,
    state_hash INTEGER NOT NULL,
    journal_digest INTEGER NOT NULL,
    events_digest INTEGER NOT NULL,
    effect_vocabulary INTEGER NOT NULL
);
CREATE TABLE clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE buildings (
    cell INTEGER NOT NULL, id INTEGER NOT NULL, schema INTEGER NOT NULL, data BLOB NOT NULL,
    PRIMARY KEY (cell, id)
) WITHOUT ROWID;
CREATE TABLE residents (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE l1_state (
    cell INTEGER NOT NULL, id INTEGER NOT NULL, schema INTEGER NOT NULL, data BLOB NOT NULL,
    PRIMARY KEY (cell, id)
) WITHOUT ROWID;
CREATE TABLE l2_state (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE journal (
    sequence INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE illness (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE scheduled (
    sequence INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE events (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE event_links (
    event_id INTEGER NOT NULL, kind INTEGER NOT NULL, idx INTEGER NOT NULL,
    target INTEGER NOT NULL, relation INTEGER NOT NULL, confidence INTEGER NOT NULL,
    PRIMARY KEY (event_id, kind, idx)
) WITHOUT ROWID;
";

/// A `u64` as a SQLite integer that sorts like the `u64`: the sign bit is
/// flipped, so entity IDs and Hilbert cell IDs (which use the top bits)
/// keep their order in primary keys and range scans.
const fn key(v: u64) -> i64 {
    (v ^ (1 << 63)) as i64
}

const fn unkey(v: i64) -> u64 {
    (v as u64) ^ (1 << 63)
}

/// A `u64` field stored as its two's-complement `i64` (not a sort key).
const fn word(v: u64) -> i64 {
    v as i64
}

const fn unword(v: i64) -> u64 {
    v as u64
}

fn corrupt(table: &'static str, e: CodecError) -> LoadError {
    LoadError::Corrupt { table, reason: e.0 }
}

fn bad(table: &'static str, reason: &'static str) -> LoadError {
    LoadError::Corrupt { table, reason }
}

/// Writes `world` to `path` as a version-1 save, replacing any file there.
///
/// The save is written to a sibling file and moved into place only when
/// complete, so `path` holds either the previous file or the new save.
///
/// # Errors
///
/// [`SaveError::PendingCommands`] if commands are queued; the other
/// variants for file-system or SQLite failures. On error `path` is
/// untouched.
pub fn save(world: &World, path: &Path) -> Result<(), SaveError> {
    if world.has_pending_commands() {
        return Err(SaveError::PendingCommands);
    }
    let parts = world.to_parts();
    let header = header_of(world, &parts)?;
    let links = parts
        .events
        .iter()
        .map(|e| {
            codec::event_links(e).ok_or(SaveError::Unsaveable("an event has an unresolved cause"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let tmp = sibling(path, ".saving");
    let _ = fs::remove_file(&tmp);
    let written = write_file(&tmp, &header, &parts, &links);
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let moved = fs::File::open(&tmp)
        .and_then(|f| f.sync_all())
        .and_then(|()| fs::rename(&tmp, path));
    if let Err(e) = moved {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn header_of(world: &World, parts: &WorldParts) -> Result<Header, SaveError> {
    let Some(first) = parts.events.first() else {
        return Err(SaveError::Unsaveable("the world has no creation event"));
    };
    let Payload::WorldCreated {
        generator,
        rules,
        min,
        max,
        ..
    } = first.record.payload
    else {
        return Err(SaveError::Unsaveable(
            "the first event is not the creation event",
        ));
    };
    let walk = parts.toy.walk_params();
    Ok(Header {
        format_version: FORMAT_VERSION,
        build: build_id(),
        seed: parts.seed,
        dataset_version: parts.toy.dataset_version(),
        generator_version: generator,
        rules_version: rules,
        sim_time_us: world.now().as_micros_since_epoch(),
        tick: world.tick(),
        walk_speed_bits: walk.speed_m_s.to_bits(),
        walk_wait_steps: walk.wait_steps,
        population_min: min,
        population_max: max,
        state_hash: world.state_hash(),
        journal_digest: world.journal_digest(),
        events_digest: world.events().digest(),
        effect_vocabulary: ws_sim::commands::EFFECT_VOCABULARY_VERSION,
    })
}

fn write_file(
    path: &Path,
    h: &Header,
    parts: &WorldParts,
    links: &[Vec<Link>],
) -> Result<(), SaveError> {
    let mut conn = Connection::open(path)?;
    // The file is new and moved into place only when complete, so no
    // journal is needed for crash safety.
    conn.pragma_update(None, "journal_mode", "OFF")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    conn.pragma_update(None, "user_version", FORMAT_VERSION)?;
    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA)?;
    tx.execute(
        "INSERT INTO header VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            h.format_version,
            h.build,
            word(h.seed),
            word(h.dataset_version),
            h.generator_version,
            h.rules_version,
            h.sim_time_us,
            word(h.tick),
            word(h.walk_speed_bits),
            h.walk_wait_steps,
            h.population_min,
            h.population_max,
            word(h.state_hash),
            word(h.journal_digest),
            word(h.events_digest),
            h.effect_vocabulary,
        ],
    )?;
    tx.execute(
        "INSERT INTO clock VALUES (1, ?, ?)",
        params![
            RECORD_SCHEMA,
            codec::put_clock(&parts.clock, parts.next_sequence, parts.boundary)
        ],
    )?;
    let toy = parts.toy.to_parts();
    {
        let mut st = tx.prepare("INSERT INTO buildings VALUES (?, ?, ?, ?)")?;
        for (id, b, level) in &toy.buildings {
            st.execute(params![
                key(b.pos.cell().raw()),
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_building(b, *level)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO residents VALUES (?, ?, ?)")?;
        for (id, r) in &toy.residents {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_resident(r)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO l1_state VALUES (?, ?, ?, ?)")?;
        for (id, pos, walk) in &toy.l1 {
            st.execute(params![
                key(pos.cell().raw()),
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_l1(*pos, walk)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO l2_state VALUES (?, ?, ?)")?;
        for (id, bg) in &toy.l2 {
            st.execute(params![key(id.raw()), RECORD_SCHEMA, codec::put_l2(bg)])?;
        }
        let mut st = tx.prepare("INSERT INTO illness VALUES (?, ?, ?)")?;
        for (id, ill) in &toy.illness {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_illness(ill)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO scheduled VALUES (?, ?, ?)")?;
        for (sequence, i) in &parts.scheduled {
            st.execute(params![
                key(*sequence),
                RECORD_SCHEMA,
                codec::put_scheduled(i)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO journal VALUES (?, ?, ?)")?;
        for o in &parts.journal {
            st.execute(params![
                key(o.sequence),
                RECORD_SCHEMA,
                codec::put_outcome(o)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO events VALUES (?, ?, ?)")?;
        let mut lt = tx.prepare("INSERT INTO event_links VALUES (?, ?, ?, ?, ?, ?)")?;
        for (e, ls) in parts.events.iter().zip(links) {
            st.execute(params![key(e.id.0), RECORD_SCHEMA, codec::put_event(e)])?;
            let (mut cause_idx, mut related_idx) = (0_i64, 0_i64);
            for l in ls {
                let idx = if l.kind == 0 {
                    &mut cause_idx
                } else {
                    &mut related_idx
                };
                lt.execute(params![
                    key(e.id.0),
                    l.kind,
                    *idx,
                    key(l.target.0),
                    l.relation,
                    l.confidence
                ])?;
                *idx += 1;
            }
        }
    }
    tx.commit()?;
    conn.close().map_err(|(_, e)| e)?;
    Ok(())
}

/// Opens a save read-only and checks that it is one this build can read.
fn open(path: &Path) -> Result<Connection, LoadError> {
    if !path.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such save file").into());
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let not_a_save = |e: rusqlite::Error| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::NotADatabase => {
            LoadError::NotASave
        }
        e => LoadError::Sqlite(e),
    };
    let app: i32 = conn
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(not_a_save)?;
    if app != APPLICATION_ID {
        return Err(LoadError::NotASave);
    }
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if found == 0 || found > FORMAT_VERSION {
        return Err(LoadError::UnsupportedVersion {
            found,
            supported: FORMAT_VERSION,
        });
    }
    let required = TABLES.iter().chain(if found >= 2 {
        TABLES_V2.iter()
    } else {
        [].iter()
    });
    for table in required {
        let present: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
            [*table],
            |r| r.get(0),
        )?;
        if !present {
            return Err(bad(table, "missing table"));
        }
    }
    Ok(conn)
}

/// Reads only the header of a save: versions, seed, time, and state hash.
///
/// # Errors
///
/// [`LoadError::NotASave`], [`LoadError::UnsupportedVersion`], or
/// [`LoadError::Corrupt`] if there is no valid header.
pub fn read_header(path: &Path) -> Result<Header, LoadError> {
    header_row(&open(path)?)
}

fn header_row(conn: &Connection) -> Result<Header, LoadError> {
    let h = conn
        .query_row("SELECT * FROM header WHERE id = 1", [], |r| {
            Ok(Header {
                format_version: r.get(1)?,
                build: r.get(2)?,
                seed: unword(r.get(3)?),
                dataset_version: unword(r.get(4)?),
                generator_version: r.get(5)?,
                rules_version: r.get(6)?,
                sim_time_us: r.get(7)?,
                tick: unword(r.get(8)?),
                walk_speed_bits: unword(r.get(9)?),
                walk_wait_steps: r.get(10)?,
                population_min: r.get(11)?,
                population_max: r.get(12)?,
                state_hash: unword(r.get(13)?),
                journal_digest: unword(r.get(14)?),
                events_digest: unword(r.get(15)?),
                effect_vocabulary: if r.as_ref().column_count() > 16 {
                    r.get(16)?
                } else {
                    0
                },
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("header", "no header row"),
            e => LoadError::Sqlite(e),
        })?;
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if h.format_version != found || !(1..=FORMAT_VERSION).contains(&found) {
        return Err(bad("header", "header and file format versions differ"));
    }
    Ok(h)
}

/// Reads `(key columns..., data)` rows of one table in key order, checking
/// each row's record schema version.
fn rows(
    conn: &Connection,
    table: &'static str,
    sql: &str,
) -> Result<Vec<(i64, i64, Vec<u8>)>, LoadError> {
    let mut st = conn.prepare(sql)?;
    let mapped = st.query_map([], |r| {
        let schema: u32 = r.get(2)?;
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            schema,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in mapped {
        let (a, b, schema, data) = row?;
        if schema != RECORD_SCHEMA {
            return Err(bad(table, "unsupported record schema version"));
        }
        out.push((a, b, data));
    }
    Ok(out)
}

fn entity(table: &'static str, k: i64) -> Result<EntityId, LoadError> {
    EntityId::from_raw(unkey(k)).map_err(|_| bad(table, "invalid entity ID"))
}

/// Loads the world saved at `path`.
///
/// Nothing is replayed. The records are decoded, the world is rebuilt and
/// checked against every invariant a running world keeps, and its state
/// hash must equal the header's.
///
/// # Errors
///
/// [`LoadError`]; no partial world is returned.
pub fn load(path: &Path) -> Result<World, LoadError> {
    let conn = open(path)?;
    let header = header_row(&conn)?;

    let (cdata, cschema): (Vec<u8>, u32) = conn
        .query_row("SELECT data, schema FROM clock WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("clock", "no clock row"),
            e => LoadError::Sqlite(e),
        })?;
    if cschema != RECORD_SCHEMA {
        return Err(bad("clock", "unsupported record schema version"));
    }
    let clock = codec::get_clock(&cdata).map_err(|e| corrupt("clock", e))?;

    let mut toy = ToyParts {
        seed: header.seed,
        dataset_version: header.dataset_version,
        walk: ws_world::WalkParams {
            speed_m_s: f64::from_bits(header.walk_speed_bits),
            wait_steps: header.walk_wait_steps,
        },
        buildings: Vec::new(),
        residents: Vec::new(),
        l1: Vec::new(),
        l2: Vec::new(),
        illness: Vec::new(),
    };
    for (cell, id, data) in rows(
        &conn,
        "buildings",
        "SELECT cell, id, schema, data FROM buildings ORDER BY cell, id",
    )? {
        let (b, level) = codec::get_building(&data).map_err(|e| corrupt("buildings", e))?;
        if key(b.pos.cell().raw()) != cell {
            return Err(bad("buildings", "key cell differs from the record's cell"));
        }
        toy.buildings.push((entity("buildings", id)?, b, level));
    }
    for (id, _, data) in rows(
        &conn,
        "residents",
        "SELECT id, 0, schema, data FROM residents ORDER BY id",
    )? {
        let r = codec::get_resident(&data).map_err(|e| corrupt("residents", e))?;
        toy.residents.push((entity("residents", id)?, r));
    }
    for (cell, id, data) in rows(
        &conn,
        "l1_state",
        "SELECT cell, id, schema, data FROM l1_state ORDER BY cell, id",
    )? {
        let (pos, walk) = codec::get_l1(&data).map_err(|e| corrupt("l1_state", e))?;
        if key(pos.cell().raw()) != cell {
            return Err(bad("l1_state", "key cell differs from the record's cell"));
        }
        toy.l1.push((entity("l1_state", id)?, pos, walk));
    }
    for (id, _, data) in rows(
        &conn,
        "l2_state",
        "SELECT id, 0, schema, data FROM l2_state ORDER BY id",
    )? {
        let bg = codec::get_l2(&data).map_err(|e| corrupt("l2_state", e))?;
        toy.l2.push((entity("l2_state", id)?, bg));
    }
    if header.format_version >= 2 {
        for (id, _, data) in rows(
            &conn,
            "illness",
            "SELECT id, 0, schema, data FROM illness ORDER BY id",
        )? {
            let ill = codec::get_illness(&data).map_err(|e| corrupt("illness", e))?;
            toy.illness.push((entity("illness", id)?, ill));
        }
    }
    let toy = ws_world::ToyWorld::from_parts(toy).map_err(LoadError::Toy)?;

    let mut journal = Vec::new();
    for (sequence, _, data) in rows(
        &conn,
        "journal",
        "SELECT sequence, 0, schema, data FROM journal ORDER BY sequence",
    )? {
        let o = codec::get_outcome(&data).map_err(|e| corrupt("journal", e))?;
        if key(o.sequence) != sequence {
            return Err(bad("journal", "key sequence differs from the record's"));
        }
        journal.push(o);
    }

    let mut scheduled = Vec::new();
    if header.format_version >= 2 {
        for (sequence, _, data) in rows(
            &conn,
            "scheduled",
            "SELECT sequence, 0, schema, data FROM scheduled ORDER BY sequence",
        )? {
            let i = codec::get_scheduled(&data).map_err(|e| corrupt("scheduled", e))?;
            scheduled.push((unkey(sequence), i));
        }
    }

    let mut links: BTreeMap<u64, Vec<Link>> = BTreeMap::new();
    {
        let mut st = conn.prepare(
            "SELECT event_id, kind, target, relation, confidence FROM event_links \
             ORDER BY event_id, kind, idx",
        )?;
        let mapped = st.query_map([], |r| {
            Ok((
                unkey(r.get(0)?),
                r.get::<_, u8>(1)?,
                unkey(r.get(2)?),
                r.get::<_, u8>(3)?,
                r.get::<_, u8>(4)?,
            ))
        })?;
        for row in mapped {
            let (event, kind, target, relation, confidence) = row?;
            links.entry(event).or_default().push(Link {
                kind,
                target: EventId(target),
                relation,
                confidence,
            });
        }
    }
    let mut events = Vec::new();
    for (id, _, data) in rows(
        &conn,
        "events",
        "SELECT id, 0, schema, data FROM events ORDER BY id",
    )? {
        let own = links.remove(&unkey(id)).unwrap_or_default();
        let e = codec::get_event(&data, &own).map_err(|e| corrupt("events", e))?;
        if key(e.id.0) != id {
            return Err(bad("events", "key ID differs from the record's"));
        }
        events.push(e);
    }
    if !links.is_empty() {
        return Err(bad("event_links", "links for an event that does not exist"));
    }

    let world = World::from_parts(WorldParts {
        seed: header.seed,
        clock: clock.clock,
        toy,
        events,
        next_sequence: clock.next_sequence,
        boundary: clock.boundary,
        journal,
        scheduled,
    })
    .map_err(LoadError::Invalid)?;

    if world.now().as_micros_since_epoch() != header.sim_time_us || world.tick() != header.tick {
        return Err(bad("header", "time or tick differs from the clock"));
    }
    for (what, stored, computed) in [
        ("state", header.state_hash, world.state_hash()),
        ("journal", header.journal_digest, world.journal_digest()),
        ("events", header.events_digest, world.events().digest()),
    ] {
        if computed != stored {
            return Err(LoadError::HashMismatch {
                what,
                stored,
                computed,
            });
        }
    }
    Ok(world)
}
