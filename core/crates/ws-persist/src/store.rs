//! The SQLite container of a slice save: schema, writing and reading
//! (ADR 0020, ADR 0036). Only the current format is read; every older
//! format is refused (ADR 0040).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use ws_ecs::EntityId;
use ws_events::{Event, EventId};
use ws_sim::slice::{SliceParts, SliceSimulation};

use crate::codec::{self, CodecError, Link};
use crate::error::{LoadError, SaveError};
use crate::header::{EFFECT_VOCABULARY, FORMAT_VERSION, Header, MODEL_KIND_SLICE, build_id};

/// `PRAGMA application_id` of a World Simulator save: ASCII "WSAV".
const APPLICATION_ID: i32 = 0x5753_4156;

/// Schema version of every record kind except the `clock` row.
const RECORD_SCHEMA: u32 = 1;

/// Schema version of the `clock` row (with `next_spawned_id` and
/// `next_event_id`, ADR 0025 §2–3).
const CLOCK_SCHEMA_V3: u32 = 2;

/// Tables of a format-6 slice save: the ADR 0036 §2 slice tables, the
/// ADR 0038 §6 block-power episodes (at most one row each, `id = 1`), and
/// ADR 0039 §4 faith (one row per retained resident) and goals (one row,
/// `id = 0`).
const TABLES: [&str; 14] = [
    "header",
    "slice_config",
    "clock",
    "slice_vitals",
    "slice_illness",
    "slice_behavior",
    "slice_lod",
    "events",
    "event_links",
    "slice_operations",
    "slice_storm",
    "slice_gathering",
    "slice_faith",
    "slice_goals",
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
    effect_vocabulary INTEGER NOT NULL,
    model_kind INTEGER NOT NULL
);
CREATE TABLE slice_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE slice_vitals (
    resident_id INTEGER PRIMARY KEY,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE slice_illness (
    resident_id INTEGER PRIMARY KEY,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE slice_behavior (
    resident_id INTEGER PRIMARY KEY,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE slice_lod (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE event_links (
    event_id INTEGER NOT NULL,
    kind INTEGER NOT NULL,
    idx INTEGER NOT NULL,
    target INTEGER NOT NULL,
    relation INTEGER NOT NULL,
    confidence INTEGER NOT NULL,
    PRIMARY KEY (event_id, kind, idx)
) WITHOUT ROWID;
CREATE TABLE slice_operations (
    idx INTEGER PRIMARY KEY,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE slice_storm (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE slice_gathering (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE slice_faith (
    resident_id INTEGER PRIMARY KEY,
    faith INTEGER NOT NULL
);
CREATE TABLE slice_goals (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    day INTEGER NOT NULL,
    data BLOB NOT NULL
);
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

/// Saves `sim` to `path` as a format-6 slice save, replacing any file there atomically.
///
/// # Errors
///
/// Returns [`SaveError`] on SQLite, I/O or unsaveable state.
pub fn save_slice(sim: &SliceSimulation, path: &Path) -> Result<(), SaveError> {
    save_slice_parts(
        sim.to_parts(),
        sim.state_hash(),
        sim.operation_digest(),
        sim.events().digest(),
        path,
    )
}

/// Writes `parts` to `path` as a format-6 slice save, replacing any file there atomically.
///
/// # Errors
///
/// Returns [`SaveError`] on SQLite, I/O or unsaveable state.
pub fn save_slice_parts(
    parts: SliceParts,
    state_hash: u64,
    op_digest: u64,
    events_digest: u64,
    path: &Path,
) -> Result<(), SaveError> {
    if parts.god.is_none() {
        return Err(SaveError::Unsaveable("a slice save needs the god state"));
    }
    let header = header_of_slice(&parts, state_hash, op_digest, events_digest);
    let links = parts
        .events
        .iter()
        .map(|e| {
            codec::event_links(e).ok_or(SaveError::Unsaveable("an event has an unresolved cause"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let tmp = sibling(path, ".saving");
    let _ = fs::remove_file(&tmp);
    let written = write_slice_file(&tmp, &header, &parts, &links);
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

/// Spawns a background thread to write `parts` to `path` as a format-6 slice save.
pub fn save_slice_background(
    parts: SliceParts,
    state_hash: u64,
    op_digest: u64,
    events_digest: u64,
    path: &Path,
) -> std::thread::JoinHandle<Result<(), SaveError>> {
    let path_buf = path.to_path_buf();
    std::thread::spawn(move || {
        save_slice_parts(parts, state_hash, op_digest, events_digest, &path_buf)
    })
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn header_of_slice(
    parts: &SliceParts,
    state_hash: u64,
    op_digest: u64,
    events_digest: u64,
) -> Header {
    Header {
        format_version: FORMAT_VERSION,
        build: build_id(),
        seed: parts.config.seed,
        dataset_version: ws_world::slice::DATASET_VERSION,
        generator_version: ws_world::slice::GENERATOR_VERSION,
        rules_version: ws_life::RULES_VERSION,
        sim_time_us: parts.clock.now().as_micros_since_epoch(),
        tick: parts.clock.steps(),
        walk_speed_bits: 0,
        walk_wait_steps: 0,
        population_min: parts.vitals.len() as u32,
        population_max: parts.vitals.len() as u32,
        state_hash,
        journal_digest: op_digest,
        events_digest,
        effect_vocabulary: EFFECT_VOCABULARY,
        model_kind: MODEL_KIND_SLICE,
    }
}

fn write_slice_file(
    path: &Path,
    h: &Header,
    parts: &SliceParts,
    links: &[Vec<Link>],
) -> Result<(), SaveError> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "OFF")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    conn.pragma_update(None, "user_version", h.format_version)?;
    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA)?;
    tx.execute(
        "INSERT INTO header VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            h.model_kind,
        ],
    )?;
    tx.execute(
        "INSERT INTO slice_config VALUES (1, ?, ?)",
        params![RECORD_SCHEMA, codec::put_slice_config(&parts.config)],
    )?;
    tx.execute(
        "INSERT INTO clock VALUES (1, ?, ?)",
        params![
            CLOCK_SCHEMA_V3,
            codec::put_clock_v3(&parts.clock, 0, 0, 0, (parts.events.len() as u64) + 1,)
        ],
    )?;
    {
        let mut st =
            tx.prepare("INSERT INTO slice_vitals (resident_id, schema, data) VALUES (?, ?, ?)")?;
        for (id, v) in &parts.vitals {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_slice_vital(v),
            ])?;
        }
    }
    {
        let mut st =
            tx.prepare("INSERT INTO slice_illness (resident_id, schema, data) VALUES (?, ?, ?)")?;
        for ep in &parts.episodes {
            st.execute(params![
                key(ep.resident.raw()),
                RECORD_SCHEMA,
                codec::put_slice_episode(ep),
            ])?;
        }
    }
    {
        let mut st =
            tx.prepare("INSERT INTO slice_behavior (resident_id, schema, data) VALUES (?, ?, ?)")?;
        for st_snap in &parts.behavior_states {
            st.execute(params![
                key(st_snap.id.raw()),
                RECORD_SCHEMA,
                codec::put_slice_resident_state(st_snap),
            ])?;
        }
    }
    tx.execute(
        "INSERT INTO slice_lod VALUES (1, ?, ?)",
        params![
            RECORD_SCHEMA,
            codec::put_slice_lod(parts.camera, &parts.nodes),
        ],
    )?;
    {
        let mut st = tx.prepare("INSERT INTO events VALUES (?, ?, ?)")?;
        let mut lt = tx.prepare("INSERT INTO event_links VALUES (?, ?, ?, ?, ?, ?)")?;
        for (e, ls) in parts.events.iter().zip(links.iter()) {
            st.execute(params![
                key(e.id.0),
                RECORD_SCHEMA,
                codec::put_event(e).map_err(|e| SaveError::Unsaveable(e.0))?,
            ])?;
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
                    l.confidence,
                ])?;
                *idx += 1;
            }
        }
    }
    {
        let mut st =
            tx.prepare("INSERT INTO slice_operations (idx, schema, data) VALUES (?, ?, ?)")?;
        for (i, op) in parts.operations.iter().enumerate() {
            st.execute(params![
                i as i64,
                RECORD_SCHEMA,
                codec::put_operation_record(op),
            ])?;
        }
    }
    if parts.storm.is_some() || parts.storm_window.is_some() {
        tx.execute(
            "INSERT INTO slice_storm VALUES (1, ?, ?)",
            params![
                RECORD_SCHEMA,
                codec::put_slice_storm(parts.storm, parts.storm_window)
            ],
        )?;
    }
    if let Some(g) = parts.gathering {
        tx.execute(
            "INSERT INTO slice_gathering VALUES (1, ?, ?)",
            params![RECORD_SCHEMA, codec::put_slice_gathering(&g)],
        )?;
    }
    if let Some(god) = &parts.god {
        let mut st = tx.prepare("INSERT INTO slice_faith (resident_id, faith) VALUES (?, ?)")?;
        for (id, faith) in god.faith() {
            st.execute(params![key(id.raw()), i64::from(*faith)])?;
        }
        tx.execute(
            "INSERT INTO slice_goals VALUES (0, ?, ?)",
            params![god.goals().day, codec::put_slice_goals(god)],
        )?;
    }
    tx.commit()?;
    conn.close().map_err(|(_, e)| e)?;
    Ok(())
}

/// Opens a save read-only and checks that it is one this build can read.
fn open(path: &Path) -> Result<Connection, LoadError> {
    #[cfg(not(target_os = "emscripten"))]
    if !path.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such save file").into());
    }
    #[cfg(target_os = "emscripten")]
    if !path.exists() {
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
    if found != FORMAT_VERSION {
        return Err(LoadError::UnsupportedVersion {
            found,
            supported: FORMAT_VERSION,
        });
    }
    for table in TABLES {
        let present: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
            [table],
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
                effect_vocabulary: r.get(16)?,
                model_kind: r.get(17)?,
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("header", "no header row"),
            e => LoadError::Sqlite(e),
        })?;
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if h.format_version != found {
        return Err(bad("header", "header and file format versions differ"));
    }
    if h.model_kind != MODEL_KIND_SLICE {
        return Err(bad("header", "save file is not a slice simulation"));
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

/// Loads the raw parts and header of a format-6 slice save.
///
/// # Errors
///
/// Returns [`LoadError`] if the file cannot be opened, is not a valid
/// format-6 slice save, has missing tables, or contains undecodable records.
pub fn load_slice_parts(path: &Path) -> Result<(Header, SliceParts), LoadError> {
    let conn = open(path)?;
    let header = header_row(&conn)?;

    let (cdata, cschema): (Vec<u8>, u32) = conn
        .query_row(
            "SELECT data, schema FROM slice_config WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("slice_config", "no slice_config row"),
            e => LoadError::Sqlite(e),
        })?;
    if cschema != RECORD_SCHEMA {
        return Err(bad("slice_config", "unsupported record schema version"));
    }
    let config = codec::get_slice_config(&cdata).map_err(|e| corrupt("slice_config", e))?;

    let (kdata, kschema): (Vec<u8>, u32) = conn
        .query_row("SELECT data, schema FROM clock WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("clock", "no clock row"),
            e => LoadError::Sqlite(e),
        })?;
    if kschema != CLOCK_SCHEMA_V3 {
        return Err(bad("clock", "unsupported record schema version"));
    }
    let clock = codec::get_clock_v3(&kdata).map_err(|e| corrupt("clock", e))?;

    let mut vitals = Vec::new();
    for (id, _, data) in rows(
        &conn,
        "slice_vitals",
        "SELECT resident_id, 0, schema, data FROM slice_vitals ORDER BY resident_id",
    )? {
        let ent = entity("slice_vitals", id)?;
        let v = codec::get_slice_vital(&data).map_err(|e| corrupt("slice_vitals", e))?;
        vitals.push((ent, v));
    }

    let mut episodes = Vec::new();
    for (id, _, data) in rows(
        &conn,
        "slice_illness",
        "SELECT resident_id, 0, schema, data FROM slice_illness ORDER BY resident_id",
    )? {
        let ent = entity("slice_illness", id)?;
        let ep = codec::get_slice_episode(&data).map_err(|e| corrupt("slice_illness", e))?;
        if ep.resident != ent {
            return Err(bad(
                "slice_illness",
                "key resident ID differs from episode resident",
            ));
        }
        episodes.push(ep);
    }

    let mut behavior_states = Vec::new();
    for (id, _, data) in rows(
        &conn,
        "slice_behavior",
        "SELECT resident_id, 0, schema, data FROM slice_behavior ORDER BY resident_id",
    )? {
        let ent = entity("slice_behavior", id)?;
        let st_snap =
            codec::get_slice_resident_state(&data).map_err(|e| corrupt("slice_behavior", e))?;
        if st_snap.id != ent {
            return Err(bad(
                "slice_behavior",
                "key resident ID differs from state resident",
            ));
        }
        behavior_states.push(st_snap);
    }

    let (lod_data, lod_schema): (Vec<u8>, u32) = conn
        .query_row("SELECT data, schema FROM slice_lod WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("slice_lod", "no slice_lod row"),
            e => LoadError::Sqlite(e),
        })?;
    if lod_schema != RECORD_SCHEMA {
        return Err(bad("slice_lod", "unsupported record schema version"));
    }
    let (camera, nodes) = codec::get_slice_lod(&lod_data).map_err(|e| corrupt("slice_lod", e))?;

    let events = load_events(&conn)?;

    if clock.next_event_id != (events.len() as u64) + 1 {
        return Err(bad("clock", "next_event_id disagrees with events"));
    }

    let mut operations = Vec::new();
    for (idx, _, data) in rows(
        &conn,
        "slice_operations",
        "SELECT idx, 0, schema, data FROM slice_operations ORDER BY idx",
    )? {
        if idx != operations.len() as i64 {
            return Err(bad("slice_operations", "operation indices not contiguous"));
        }
        let op = codec::get_operation_record(&data).map_err(|e| corrupt("slice_operations", e))?;
        operations.push(op);
    }

    let (storm, storm_window) = match single_row(&conn, "slice_storm")? {
        Some(data) => codec::get_slice_storm(&data).map_err(|e| corrupt("slice_storm", e))?,
        None => (None, None),
    };
    let gathering = single_row(&conn, "slice_gathering")?
        .map(|data| codec::get_slice_gathering(&data).map_err(|e| corrupt("slice_gathering", e)))
        .transpose()?;

    let god = Some(load_god(&conn)?);

    let parts = SliceParts {
        config,
        clock: clock.clock,
        vitals,
        episodes,
        behavior_states,
        camera,
        nodes,
        events,
        operations,
        storm,
        storm_window,
        gathering,
        god,
    };
    Ok((header, parts))
}

/// The format-6 faith rows and the single goals row (ADR 0039 §4).
fn load_god(conn: &Connection) -> Result<ws_sim::slice::God, LoadError> {
    let mut faith = Vec::new();
    {
        let mut st =
            conn.prepare("SELECT resident_id, faith FROM slice_faith ORDER BY resident_id")?;
        let mapped = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for row in mapped {
            let (k, f) = row?;
            let id = entity("slice_faith", k)?;
            let f = i16::try_from(f).map_err(|_| bad("slice_faith", "faith out of range"))?;
            faith.push((id, f));
        }
    }
    let mut goals = None;
    {
        let mut st = conn.prepare("SELECT id, day, data FROM slice_goals ORDER BY id")?;
        let mapped = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        for row in mapped {
            let (id, day, data) = row?;
            if id != 0 || goals.is_some() {
                return Err(bad("slice_goals", "goals row is not id 0"));
            }
            goals =
                Some(codec::get_slice_goals(day, &data).map_err(|e| corrupt("slice_goals", e))?);
        }
    }
    let (goals, gathering) = goals.ok_or(bad("slice_goals", "no slice_goals row"))?;
    ws_sim::slice::God::from_parts(faith, goals, gathering).map_err(|e| {
        bad(
            "slice_faith",
            match e {
                ws_sim::slice::GodError::FaithRange => "faith out of range",
                ws_sim::slice::GodError::Order => "faith resident IDs not ascending",
                _ => "invalid god state",
            },
        )
    })
}

/// The optional `id = 1` row of an episode table.
fn single_row(conn: &Connection, table: &'static str) -> Result<Option<Vec<u8>>, LoadError> {
    let mut out = None;
    for (id, _, data) in rows(
        conn,
        table,
        &format!("SELECT id, 0, schema, data FROM {table} ORDER BY id"),
    )? {
        if id != 1 || out.is_some() {
            return Err(bad(table, "episode row is not id 1"));
        }
        out = Some(data);
    }
    Ok(out)
}

/// Loads a [`SliceSimulation`] from a format-6 save file.
///
/// # Errors
///
/// Returns [`LoadError`] if the save is corrupt, invalid, or fails state-hash
/// or events/journal digest verification against the header.
pub fn load_slice(path: &Path) -> Result<SliceSimulation, LoadError> {
    let (header, parts) = load_slice_parts(path)?;
    let sim = SliceSimulation::from_parts(parts)?;
    if sim.state_hash() != header.state_hash {
        return Err(LoadError::HashMismatch {
            what: "state",
            stored: header.state_hash,
            computed: sim.state_hash(),
        });
    }
    if sim.operation_digest() != header.journal_digest {
        return Err(LoadError::HashMismatch {
            what: "journal",
            stored: header.journal_digest,
            computed: sim.operation_digest(),
        });
    }
    if sim.events().digest() != header.events_digest {
        return Err(LoadError::HashMismatch {
            what: "events",
            stored: header.events_digest,
            computed: sim.events().digest(),
        });
    }
    Ok(sim)
}

fn load_events(conn: &Connection) -> Result<Vec<Event>, LoadError> {
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
        conn,
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
    Ok(events)
}
