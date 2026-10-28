//! The SQLite container: schema, writing, reading, and format-3 migration
//! (ADR 0020, ADR 0021, ADR 0025).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use ws_ecs::{EntityId, Namespace};
use ws_events::{Event, EventId, Payload};
use ws_math::SurfacePos;
use ws_sim::slice::{SliceParts, SliceSimulation};
use ws_sim::{World, WorldParts};
use ws_world::{
    BUILDING_GENERATOR_VERSION, BaselineAddress, Building, IDENTITY_POLICY_FAIL_ON_COLLISION,
    Population, RESIDENT_GENERATOR_VERSION, Resident, TOY_DATASET_ID, Tombstone, ToyParts,
    ToyWorld,
};

use crate::codec::{self, COMPONENT_BUILDING, COMPONENT_RESIDENT, CodecError, EntityRecord, Link};
use crate::error::{LoadError, SaveError};
use crate::header::{BaselineRecord, BaselineRegistry, FORMAT_VERSION, Header, build_id};

/// `PRAGMA application_id` of a World Simulator save: ASCII "WSAV".
const APPLICATION_ID: i32 = 0x5753_4156;

/// Schema version of every record kind in formats 1–3, except format-3's
/// extended `clock` row (`CLOCK_SCHEMA_V3`).
const RECORD_SCHEMA: u32 = 1;

/// Schema version of the `clock` row in format 3 (adds `next_spawned_id` and
/// `next_event_id`, ADR 0025 §2–3).
const CLOCK_SCHEMA_V3: u32 = 2;

/// Format-3 implementation workload limit, checked before baseline generation.
/// Shared by reader and writer so every newly written save can be loaded.
const MAX_BASELINE_ENTITIES: u32 = 1_000_000;

fn baseline_counts(
    seed: u64,
    dataset: &ws_world::Dataset,
    population: Population,
) -> Result<(u32, u32), &'static str> {
    if population.min_per_building > population.max_per_building {
        return Err("invalid population bounds");
    }
    let buildings =
        u32::try_from(dataset.sites.len()).map_err(|_| "baseline exceeds entity limit")?;
    if buildings > MAX_BASELINE_ENTITIES {
        return Err("baseline exceeds entity limit");
    }
    let mut residents = 0_u32;
    for index in 0..buildings {
        residents = residents
            .checked_add(ws_world::ids::resident_count(
                seed,
                dataset.version,
                index,
                population,
            ))
            .ok_or("baseline exceeds entity limit")?;
        if residents > MAX_BASELINE_ENTITIES - buildings {
            return Err("baseline exceeds entity limit");
        }
    }
    Ok((buildings, residents))
}

/// Tables in format 1 (ADR 0020).
const TABLES_V1: [&str; 9] = [
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

/// Tables added by format 2 (ADR 0021 §7); a format-1 file lacks them and
/// loads with none of their rows.
const TABLES_V2: [&str; 2] = ["illness", "scheduled"];

/// Tables in format 3 (ADR 0025 §2).
const TABLES_V3: [&str; 15] = [
    "header",
    "baseline",
    "clock",
    "structural_overrides",
    "tombstones",
    "notables",
    "spawned",
    "node_lod",
    "l1_state",
    "l2_state",
    "illness",
    "scheduled",
    "journal",
    "events",
    "event_links",
];

const SCHEMA_V3: &str = "
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
CREATE TABLE baseline (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE structural_overrides (
    cell INTEGER NOT NULL,
    id INTEGER NOT NULL,
    component_tag INTEGER NOT NULL,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL,
    PRIMARY KEY (cell, id, component_tag)
) WITHOUT ROWID;
CREATE TABLE tombstones (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE notables (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE spawned (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE node_lod (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE l1_state (
    cell INTEGER NOT NULL, id INTEGER NOT NULL, schema INTEGER NOT NULL, data BLOB NOT NULL,
    PRIMARY KEY (cell, id)
) WITHOUT ROWID;
CREATE TABLE l2_state (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE illness (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE scheduled (
    sequence INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE journal (
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

const SCHEMA_V4_TOY: &str = "
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
CREATE TABLE baseline (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema INTEGER NOT NULL,
    data BLOB NOT NULL
);
CREATE TABLE structural_overrides (
    cell INTEGER NOT NULL,
    id INTEGER NOT NULL,
    component_tag INTEGER NOT NULL,
    schema INTEGER NOT NULL,
    data BLOB NOT NULL,
    PRIMARY KEY (cell, id, component_tag)
) WITHOUT ROWID;
CREATE TABLE tombstones (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE notables (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE spawned (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE node_lod (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE l1_state (
    cell INTEGER NOT NULL, id INTEGER NOT NULL, schema INTEGER NOT NULL, data BLOB NOT NULL,
    PRIMARY KEY (cell, id)
) WITHOUT ROWID;
CREATE TABLE l2_state (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE illness (
    id INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE scheduled (
    sequence INTEGER PRIMARY KEY, schema INTEGER NOT NULL, data BLOB NOT NULL
) WITHOUT ROWID;
CREATE TABLE journal (
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

/// Tables in format 4 for the real slice (ADR 0036 §2).
const TABLES_V4_SLICE: [&str; 10] = [
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
];

const SCHEMA_V4_SLICE: &str = "
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

fn same_pos_bits(a: SurfacePos, b: SurfacePos) -> bool {
    a.cell() == b.cell()
        && a.offset()
            .iter()
            .zip(b.offset())
            .all(|(x, y)| x.to_bits() == y.to_bits())
}

fn same_building_bits(a: &Building, b: &Building) -> bool {
    a.index == b.index
        && same_pos_bits(a.pos, b.pos)
        && a.half_extent_m.to_bits() == b.half_extent_m.to_bits()
        && a.height_m.to_bits() == b.height_m.to_bits()
}

struct PreparedDeltas {
    baseline: BaselineRecord,
    overrides: Vec<(i64, EntityId, u8, Vec<u8>)>,
    tombstones: Vec<(EntityId, Tombstone)>,
    notables: Vec<(EntityId, EntityRecord)>,
    spawned: Vec<(EntityId, EntityRecord)>,
}

/// Writes `world` to `path` as a format-3 baseline-plus-deltas save using the
/// built-in baseline registry, replacing any file there atomically.
///
/// The save is written to a sibling `.saving` file and moved into place only
/// when complete, so `path` holds either the previous file or the new save.
///
/// # Errors
///
/// [`SaveError::PendingCommands`] if commands are queued; the other
/// variants for unsaveable worlds, file-system, or SQLite failures. On error
/// `path` is untouched.
pub fn save(world: &World, path: &Path) -> Result<(), SaveError> {
    let registry = BaselineRegistry::with_builtin();
    save_with_registry(world, path, &registry)
}

/// Writes `world` to `path` as a format-3 baseline-plus-deltas save using
/// `registry` to resolve the pinned baseline dataset.
///
/// # Errors
///
/// As [`save`].
pub fn save_with_registry(
    world: &World,
    path: &Path,
    registry: &BaselineRegistry,
) -> Result<(), SaveError> {
    save_with_registry_version(world, path, registry, FORMAT_VERSION)
}

fn save_with_registry_version(
    world: &World,
    path: &Path,
    registry: &BaselineRegistry,
    format_version: u32,
) -> Result<(), SaveError> {
    if world.has_pending_commands() {
        return Err(SaveError::PendingCommands);
    }
    let parts = world.to_parts();
    let header = header_of(world, &parts, format_version)?;
    if parts.events.iter().any(|e| e.record.payload.tag() >= 7) {
        return Err(SaveError::Unsaveable(
            "slice payload requires a slice save format",
        ));
    }
    let links = parts
        .events
        .iter()
        .map(|e| {
            codec::event_links(e).ok_or(SaveError::Unsaveable("an event has an unresolved cause"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let deltas = classify_deltas(&header, &parts, registry)?;

    let tmp = sibling(path, ".saving");
    let _ = fs::remove_file(&tmp);
    let written = write_file_toy(&tmp, &header, &parts, &links, &deltas);
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

/// Saves `sim` to `path` as a format-4 slice save, replacing any file there atomically.
///
/// # Errors
///
/// Returns [`SaveError`] on SQLite, I/O or unsaveable state.
pub fn save_slice(sim: &SliceSimulation, path: &Path) -> Result<(), SaveError> {
    let parts = sim.to_parts();
    save_slice_parts(
        parts,
        sim.state_hash(),
        sim.operation_digest(),
        sim.events().digest(),
        path,
    )
}

/// Writes `parts` to `path` as a format-4 slice save, replacing any file there atomically.
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
    let header = header_of_slice(&parts, state_hash, op_digest, events_digest)?;
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

/// Spawns a background thread to write `parts` to `path` as a format-4 slice save.
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

fn classify_deltas(
    header: &Header,
    parts: &WorldParts,
    registry: &BaselineRegistry,
) -> Result<PreparedDeltas, SaveError> {
    if header.generator_version != BUILDING_GENERATOR_VERSION
        || header.generator_version != RESIDENT_GENERATOR_VERSION
        || header.rules_version != 1
    {
        return Err(SaveError::Unsaveable(
            "unsupported baseline generator or rules version",
        ));
    }
    let Some(dataset) = registry.get(TOY_DATASET_ID, header.dataset_version) else {
        return Err(SaveError::Unsaveable(
            "baseline dataset is not in the registry",
        ));
    };
    if header.population_min > header.population_max {
        return Err(SaveError::Unsaveable("invalid population bounds"));
    }
    let population = Population {
        min_per_building: header.population_min,
        max_per_building: header.population_max,
    };
    baseline_counts(header.seed, dataset, population).map_err(SaveError::Unsaveable)?;
    let base_world = ToyWorld::generate(header.seed, dataset, population, parts.toy.walk_params())
        .map_err(|_| SaveError::Unsaveable("baseline generation failed"))?;

    let mut baseline = BaselineRecord {
        seed: header.seed,
        dataset_id: TOY_DATASET_ID,
        dataset_version: header.dataset_version,
        dataset_sha256: dataset.content_sha256(),
        building_generator_version: BUILDING_GENERATOR_VERSION,
        resident_generator_version: RESIDENT_GENERATOR_VERSION,
        rules_version: header.rules_version,
        identity_policy_version: IDENTITY_POLICY_FAIL_ON_COLLISION,
        population_min: header.population_min,
        population_max: header.population_max,
        walk_speed_bits: header.walk_speed_bits,
        walk_wait_steps: header.walk_wait_steps,
        start_time_us: parts.clock.start().as_micros_since_epoch(),
        building_count: u32::try_from(base_world.buildings().len())
            .expect("fewer than 2^32 buildings"),
        resident_count: u32::try_from(base_world.residents().len())
            .expect("fewer than 2^32 residents"),
        structural_digest: base_world.structural_digest(),
        baseline_digest: 0,
        collision_overrides: Vec::new(),
    };
    baseline.baseline_digest = baseline.compute_digest();

    let toy = &parts.toy;
    for (id, t) in toy.tombstones().iter() {
        match t.address {
            BaselineAddress::Building(index) => {
                let Some(b) = base_world.buildings().get(id) else {
                    return Err(SaveError::Unsaveable(
                        "tombstone does not match a baseline building",
                    ));
                };
                if b.index != index {
                    return Err(SaveError::Unsaveable("tombstone building index mismatch"));
                }
            }
            BaselineAddress::Resident(addr) => {
                let Some(r) = base_world.residents().get(id) else {
                    return Err(SaveError::Unsaveable(
                        "tombstone does not match a baseline resident",
                    ));
                };
                if r.address != addr {
                    return Err(SaveError::Unsaveable("tombstone resident address mismatch"));
                }
            }
        }
    }
    for (id, _) in base_world.buildings().iter() {
        if !toy.buildings().contains(id) && !toy.tombstones().contains(id) {
            return Err(SaveError::Unsaveable(
                "a baseline building is missing without a tombstone",
            ));
        }
    }
    for (id, _) in base_world.residents().iter() {
        if !toy.residents().contains(id) && !toy.tombstones().contains(id) {
            return Err(SaveError::Unsaveable(
                "a baseline resident is missing without a tombstone",
            ));
        }
    }

    let mut overrides = Vec::new();
    let mut notables = Vec::new();
    let mut spawned = Vec::new();

    for (id, b) in toy.buildings().iter() {
        match id.namespace() {
            Namespace::Procedural => {
                let Some(base_b) = base_world.buildings().get(id) else {
                    return Err(SaveError::Unsaveable(
                        "a procedural building is not in the baseline",
                    ));
                };
                if b.index != base_b.index {
                    return Err(SaveError::Unsaveable(
                        "a procedural building changed its generation index",
                    ));
                }
                if toy.notables().contains(&id) {
                    notables.push((id, EntityRecord::Building(*b)));
                } else if !same_building_bits(b, base_b) {
                    overrides.push((
                        key(b.pos.cell().raw()),
                        id,
                        COMPONENT_BUILDING,
                        codec::put_building_component(b),
                    ));
                }
            }
            Namespace::Spawned => {
                if id.payload() >= toy.next_spawned_id() {
                    return Err(SaveError::Unsaveable(
                        "spawned building ID is at or past next_spawned_id",
                    ));
                }
                spawned.push((id, EntityRecord::Building(*b)));
            }
        }
    }

    for (id, r) in toy.residents().iter() {
        match id.namespace() {
            Namespace::Procedural => {
                let Some(base_r) = base_world.residents().get(id) else {
                    return Err(SaveError::Unsaveable(
                        "a procedural resident is not in the baseline",
                    ));
                };
                if r.address != base_r.address {
                    return Err(SaveError::Unsaveable(
                        "a procedural resident changed its generation address",
                    ));
                }
                if toy.notables().contains(&id) {
                    notables.push((id, EntityRecord::Resident(*r)));
                } else if r != base_r {
                    let home_cell = toy
                        .buildings()
                        .get(r.home)
                        .ok_or(SaveError::Unsaveable("resident home does not exist"))?
                        .pos
                        .cell();
                    overrides.push((
                        key(home_cell.raw()),
                        id,
                        COMPONENT_RESIDENT,
                        codec::put_resident(r),
                    ));
                }
            }
            Namespace::Spawned => {
                if id.payload() >= toy.next_spawned_id() {
                    return Err(SaveError::Unsaveable(
                        "spawned resident ID is at or past next_spawned_id",
                    ));
                }
                spawned.push((id, EntityRecord::Resident(*r)));
            }
        }
    }

    notables.sort_unstable_by_key(|(id, _)| *id);
    spawned.sort_unstable_by_key(|(id, _)| *id);
    overrides.sort_unstable_by_key(|(cell, id, tag, _)| (*cell, *id, *tag));

    let tombstones = toy.tombstones().iter().map(|(id, t)| (id, *t)).collect();

    Ok(PreparedDeltas {
        baseline,
        overrides,
        tombstones,
        notables,
        spawned,
    })
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn header_of(world: &World, parts: &WorldParts, format_version: u32) -> Result<Header, SaveError> {
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
        format_version,
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
        model_kind: crate::header::MODEL_KIND_TOY,
    })
}

fn write_file_toy(
    path: &Path,
    h: &Header,
    parts: &WorldParts,
    links: &[Vec<Link>],
    deltas: &PreparedDeltas,
) -> Result<(), SaveError> {
    let (next_event_id, _) = parts
        .events
        .len()
        .checked_add(1)
        .map(|n| (n as u64, 0u64))
        .ok_or(SaveError::Unsaveable("event counter exhausted"))?;

    let mut conn = Connection::open(path)?;
    // The file is new and moved into place only when complete, so no
    // journal is needed for crash safety.
    conn.pragma_update(None, "journal_mode", "OFF")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.pragma_update(None, "application_id", APPLICATION_ID)?;
    conn.pragma_update(None, "user_version", h.format_version)?;
    let tx = conn.transaction()?;
    if h.format_version == 3 {
        tx.execute_batch(SCHEMA_V3)?;
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
    } else {
        tx.execute_batch(SCHEMA_V4_TOY)?;
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
    }
    tx.execute(
        "INSERT INTO baseline VALUES (1, ?, ?)",
        params![RECORD_SCHEMA, codec::put_baseline(&deltas.baseline)],
    )?;
    tx.execute(
        "INSERT INTO clock VALUES (1, ?, ?)",
        params![
            CLOCK_SCHEMA_V3,
            codec::put_clock_v3(
                &parts.clock,
                parts.next_sequence,
                parts.boundary,
                parts.toy.next_spawned_id(),
                next_event_id,
            )
        ],
    )?;
    let toy = parts.toy.to_parts();
    {
        let mut st = tx.prepare("INSERT INTO structural_overrides VALUES (?, ?, ?, ?, ?)")?;
        for (cell, id, tag, data) in &deltas.overrides {
            st.execute(params![*cell, key(id.raw()), *tag, RECORD_SCHEMA, data])?;
        }
        let mut st = tx.prepare("INSERT INTO tombstones VALUES (?, ?, ?)")?;
        for (id, t) in &deltas.tombstones {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_tombstone(t)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO notables VALUES (?, ?, ?)")?;
        for (id, rec) in &deltas.notables {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_entity_record(rec)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO spawned VALUES (?, ?, ?)")?;
        for (id, rec) in &deltas.spawned {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_entity_record(rec)
            ])?;
        }
        let mut st = tx.prepare("INSERT INTO node_lod VALUES (?, ?, ?)")?;
        for (id, _, level) in &toy.buildings {
            st.execute(params![
                key(id.raw()),
                RECORD_SCHEMA,
                codec::put_node_lod(*level)
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
            st.execute(params![
                key(e.id.0),
                RECORD_SCHEMA,
                codec::put_event(e).map_err(|e| SaveError::Unsaveable(e.0))?
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

fn header_of_slice(
    parts: &SliceParts,
    state_hash: u64,
    op_digest: u64,
    events_digest: u64,
) -> Result<Header, SaveError> {
    Ok(Header {
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
        effect_vocabulary: ws_sim::commands::EFFECT_VOCABULARY_VERSION,
        model_kind: crate::header::MODEL_KIND_SLICE,
    })
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
    conn.pragma_update(None, "user_version", FORMAT_VERSION)?;
    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA_V4_SLICE)?;
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
    if found == 0 || found > FORMAT_VERSION {
        return Err(LoadError::UnsupportedVersion {
            found,
            supported: FORMAT_VERSION,
        });
    }
    let required: Box<dyn Iterator<Item = &&str>> = match found {
        1 => Box::new(TABLES_V1.iter()),
        2 => Box::new(TABLES_V1.iter().chain(TABLES_V2.iter())),
        3 => Box::new(TABLES_V3.iter()),
        4 => {
            let is_slice: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'slice_config')",
                [],
                |r| r.get(0),
            )?;
            if is_slice {
                Box::new(TABLES_V4_SLICE.iter())
            } else {
                Box::new(TABLES_V3.iter())
            }
        }
        _ => unreachable!(),
    };
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

/// Reads the pinned [`BaselineRecord`] of a format-3 save.
///
/// # Errors
///
/// [`LoadError`] if the file is not a format-3 save or its `baseline` row is
/// missing or corrupt.
pub fn read_baseline(path: &Path) -> Result<BaselineRecord, LoadError> {
    let conn = open(path)?;
    let h = header_row(&conn)?;
    if h.format_version < 3 {
        return Err(bad("baseline", "save format predates the baseline table"));
    }
    baseline_row(&conn)
}

fn baseline_row(conn: &Connection) -> Result<BaselineRecord, LoadError> {
    let (bdata, bschema): (Vec<u8>, u32) = conn
        .query_row("SELECT data, schema FROM baseline WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("baseline", "no baseline row"),
            e => LoadError::Sqlite(e),
        })?;
    if bschema != RECORD_SCHEMA {
        return Err(bad("baseline", "unsupported record schema version"));
    }
    let baseline = codec::get_baseline(&bdata).map_err(|e| corrupt("baseline", e))?;
    if baseline.baseline_digest != baseline.compute_digest() {
        return Err(bad("baseline", "baseline digest mismatch"));
    }
    Ok(baseline)
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
                model_kind: if r.as_ref().column_count() > 17 {
                    r.get(17)?
                } else if unword(r.get(4)?) == ws_world::slice::DATASET_VERSION {
                    crate::header::MODEL_KIND_SLICE
                } else {
                    crate::header::MODEL_KIND_TOY
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

/// Loads the world saved at `path` using the built-in baseline registry.
///
/// Nothing is replayed. The records are decoded, the world is rebuilt and
/// checked against every invariant a running world keeps, and its state
/// hash must equal the header's.
///
/// # Errors
///
/// [`LoadError`]; no partial world is returned.
pub fn load(path: &Path) -> Result<World, LoadError> {
    let registry = BaselineRegistry::with_builtin();
    load_with_registry(path, &registry)
}

/// Loads the world saved at `path` using `registry` to resolve the pinned
/// baseline dataset of a format-3 save.
///
/// # Errors
///
/// [`LoadError`]; no partial world is returned.
pub fn load_with_registry(path: &Path, registry: &BaselineRegistry) -> Result<World, LoadError> {
    let conn = open(path)?;
    let header = header_row(&conn)?;
    if header.is_slice() {
        return Err(bad(
            "header",
            "save file is a slice simulation; use load_slice",
        ));
    }
    if header.format_version == 3 || header.format_version == 4 {
        load_v3(&conn, &header, registry)
    } else {
        load_v1_v2(&conn, &header)
    }
}

/// Loads the raw parts and header of a format-4 slice save.
///
/// # Errors
///
/// Returns [`LoadError`] if the file cannot be opened, is not a valid format-4
/// slice save, has missing tables, or contains undecodable records.
pub fn load_slice_parts(path: &Path) -> Result<(Header, SliceParts), LoadError> {
    let conn = open(path)?;
    let header = header_row(&conn)?;
    if !header.is_slice() && header.format_version != 4 {
        return Err(bad("header", "save file is not a slice simulation"));
    }
    if header.format_version != 4 {
        return Err(LoadError::UnsupportedVersion {
            found: header.format_version,
            supported: FORMAT_VERSION,
        });
    }

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
    };
    Ok((header, parts))
}

/// Loads a [`SliceSimulation`] from a format-4 save file.
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

fn load_v1_v2(conn: &Connection, header: &Header) -> Result<World, LoadError> {
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
        notables: BTreeSet::new(),
        tombstones: Vec::new(),
        next_spawned_id: 0,
    };
    for (cell, id, data) in rows(
        conn,
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
        conn,
        "residents",
        "SELECT id, 0, schema, data FROM residents ORDER BY id",
    )? {
        let r = codec::get_resident(&data).map_err(|e| corrupt("residents", e))?;
        toy.residents.push((entity("residents", id)?, r));
    }
    read_runtime_and_finish(conn, header, clock, toy, None)
}

fn load_v3(
    conn: &Connection,
    header: &Header,
    registry: &BaselineRegistry,
) -> Result<World, LoadError> {
    let baseline = baseline_row(conn)?;
    if baseline.building_generator_version != BUILDING_GENERATOR_VERSION {
        return Err(LoadError::UnsupportedBaseline {
            field: "building_generator_version",
            version: u64::from(baseline.building_generator_version),
        });
    }
    if baseline.resident_generator_version != RESIDENT_GENERATOR_VERSION {
        return Err(LoadError::UnsupportedBaseline {
            field: "resident_generator_version",
            version: u64::from(baseline.resident_generator_version),
        });
    }
    if baseline.rules_version != 1 {
        return Err(LoadError::UnsupportedBaseline {
            field: "rules_version",
            version: u64::from(baseline.rules_version),
        });
    }
    if baseline.identity_policy_version != IDENTITY_POLICY_FAIL_ON_COLLISION {
        return Err(LoadError::UnsupportedBaseline {
            field: "identity_policy_version",
            version: u64::from(baseline.identity_policy_version),
        });
    }
    if !baseline.collision_overrides.is_empty() {
        return Err(LoadError::UnsupportedBaseline {
            field: "collision_overrides",
            version: baseline.collision_overrides.len() as u64,
        });
    }
    if baseline.seed != header.seed
        || baseline.dataset_version != header.dataset_version
        || baseline.population_min != header.population_min
        || baseline.population_max != header.population_max
        || baseline.walk_speed_bits != header.walk_speed_bits
        || baseline.walk_wait_steps != header.walk_wait_steps
        || baseline.rules_version != header.rules_version
        || baseline.building_generator_version != header.generator_version
        || baseline.resident_generator_version != header.generator_version
    {
        return Err(bad("baseline", "baseline disagrees with header"));
    }
    if baseline.population_min > baseline.population_max {
        return Err(bad("baseline", "invalid population bounds"));
    }

    let dataset = registry.resolve(
        baseline.dataset_id,
        baseline.dataset_version,
        &baseline.dataset_sha256,
    )?;

    let walk = ws_world::WalkParams {
        speed_m_s: f64::from_bits(header.walk_speed_bits),
        wait_steps: header.walk_wait_steps,
    };
    let population = Population {
        min_per_building: baseline.population_min,
        max_per_building: baseline.population_max,
    };
    let counts = baseline_counts(header.seed, dataset, population)
        .map_err(|reason| bad("baseline", reason))?;
    if counts != (baseline.building_count, baseline.resident_count) {
        return Err(bad("baseline", "regenerated baseline manifest mismatch"));
    }
    let base_world =
        ToyWorld::generate(header.seed, dataset, population, walk).map_err(LoadError::Toy)?;

    if base_world.buildings().len() != baseline.building_count as usize
        || base_world.residents().len() != baseline.resident_count as usize
        || base_world.structural_digest() != baseline.structural_digest
    {
        return Err(bad("baseline", "regenerated baseline manifest mismatch"));
    }

    let (cdata, cschema): (Vec<u8>, u32) = conn
        .query_row("SELECT data, schema FROM clock WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => bad("clock", "no clock row"),
            e => LoadError::Sqlite(e),
        })?;
    if cschema != CLOCK_SCHEMA_V3 {
        return Err(bad("clock", "unsupported record schema version"));
    }
    let clock_v3 = codec::get_clock_v3(&cdata).map_err(|e| corrupt("clock", e))?;
    if clock_v3.clock.start().as_micros_since_epoch() != baseline.start_time_us {
        return Err(bad("baseline", "start time differs from the clock"));
    }
    if clock_v3.next_spawned_id > (1 << 62) {
        return Err(bad("clock", "next_spawned_id exceeds 62-bit namespace"));
    }

    let mut buildings_map: BTreeMap<EntityId, Building> = base_world
        .buildings()
        .iter()
        .map(|(id, b)| (id, *b))
        .collect();
    let mut residents_map: BTreeMap<EntityId, Resident> = base_world
        .residents()
        .iter()
        .map(|(id, r)| (id, *r))
        .collect();

    let mut tombstones_map: BTreeMap<EntityId, Tombstone> = BTreeMap::new();
    for (k, _, data) in rows(
        conn,
        "tombstones",
        "SELECT id, 0, schema, data FROM tombstones ORDER BY id",
    )? {
        let id = entity("tombstones", k)?;
        if id.namespace() != Namespace::Procedural {
            return Err(bad("tombstones", "tombstone is not a procedural entity"));
        }
        let t = codec::get_tombstone(&data).map_err(|e| corrupt("tombstones", e))?;
        match t.address {
            BaselineAddress::Building(index) => {
                let Some(base_b) = buildings_map.remove(&id) else {
                    return Err(bad("tombstones", "tombstone is not a baseline building"));
                };
                if base_b.index != index {
                    return Err(bad("tombstones", "tombstone building index mismatch"));
                }
            }
            BaselineAddress::Resident(addr) => {
                let Some(base_r) = residents_map.remove(&id) else {
                    return Err(bad("tombstones", "tombstone is not a baseline resident"));
                };
                if base_r.address != addr {
                    return Err(bad("tombstones", "tombstone resident address mismatch"));
                }
            }
        }
        if tombstones_map.insert(id, t).is_some() {
            return Err(bad("tombstones", "duplicate tombstone ID"));
        }
    }

    let mut notables_set: BTreeSet<EntityId> = BTreeSet::new();
    for (k, _, data) in rows(
        conn,
        "notables",
        "SELECT id, 0, schema, data FROM notables ORDER BY id",
    )? {
        let id = entity("notables", k)?;
        if id.namespace() != Namespace::Procedural || tombstones_map.contains_key(&id) {
            return Err(bad(
                "notables",
                "notable is tombstoned or not in the procedural namespace",
            ));
        }
        let rec = codec::get_entity_record(&data).map_err(|e| corrupt("notables", e))?;
        match rec {
            EntityRecord::Building(b) => {
                let Some(base_b) = buildings_map.get_mut(&id) else {
                    return Err(bad("notables", "notable building is not in baseline"));
                };
                if b.index != base_b.index {
                    return Err(bad("notables", "notable building index mismatch"));
                }
                *base_b = b;
            }
            EntityRecord::Resident(r) => {
                let Some(base_r) = residents_map.get_mut(&id) else {
                    return Err(bad("notables", "notable resident is not in baseline"));
                };
                if r.address != base_r.address {
                    return Err(bad("notables", "notable resident address mismatch"));
                }
                *base_r = r;
            }
        }
        if !notables_set.insert(id) {
            return Err(bad("notables", "duplicate notable ID"));
        }
    }

    let mut raw_overrides: Vec<(i64, EntityId, u8, Vec<u8>)> = Vec::new();
    {
        let mut st = conn.prepare(
            "SELECT cell, id, component_tag, schema, data FROM structural_overrides \
             ORDER BY component_tag, id, cell",
        )?;
        let mapped = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, u8>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        for row in mapped {
            let (cell, k, tag, schema, data) = row?;
            if schema != RECORD_SCHEMA {
                return Err(bad(
                    "structural_overrides",
                    "unsupported record schema version",
                ));
            }
            let id = entity("structural_overrides", k)?;
            raw_overrides.push((cell, id, tag, data));
        }
    }
    raw_overrides.sort_unstable_by_key(|(cell, id, tag, _)| (*tag, *id, *cell));

    let mut seen_override_keys: BTreeSet<(EntityId, u8)> = BTreeSet::new();
    let mut resident_override_cells: Vec<(EntityId, i64)> = Vec::new();
    for (cell, id, tag, data) in raw_overrides {
        if id.namespace() != Namespace::Procedural {
            return Err(bad(
                "structural_overrides",
                "override targets a non-procedural entity",
            ));
        }
        if tombstones_map.contains_key(&id) {
            return Err(bad(
                "structural_overrides",
                "override targets a tombstoned entity",
            ));
        }
        if notables_set.contains(&id) {
            return Err(bad(
                "structural_overrides",
                "simultaneous notable and structural override",
            ));
        }
        if !seen_override_keys.insert((id, tag)) {
            return Err(bad(
                "structural_overrides",
                "duplicate override for entity and component",
            ));
        }
        match tag {
            COMPONENT_BUILDING => {
                let b = codec::get_building_component(&data)
                    .map_err(|e| corrupt("structural_overrides", e))?;
                if key(b.pos.cell().raw()) != cell {
                    return Err(bad(
                        "structural_overrides",
                        "key cell differs from the record's cell",
                    ));
                }
                let Some(base_b) = buildings_map.get_mut(&id) else {
                    return Err(bad(
                        "structural_overrides",
                        "building override does not match a baseline building",
                    ));
                };
                if b.index != base_b.index {
                    return Err(bad(
                        "structural_overrides",
                        "building override changes generation index",
                    ));
                }
                if same_building_bits(&b, base_b) {
                    return Err(bad(
                        "structural_overrides",
                        "redundant unchanged building override",
                    ));
                }
                *base_b = b;
            }
            COMPONENT_RESIDENT => {
                let r =
                    codec::get_resident(&data).map_err(|e| corrupt("structural_overrides", e))?;
                let Some(base_r) = residents_map.get_mut(&id) else {
                    return Err(bad(
                        "structural_overrides",
                        "resident override does not match a baseline resident",
                    ));
                };
                if r.address != base_r.address {
                    return Err(bad(
                        "structural_overrides",
                        "resident override changes generation address",
                    ));
                }
                if r == *base_r {
                    return Err(bad(
                        "structural_overrides",
                        "redundant unchanged resident override",
                    ));
                }
                *base_r = r;
                resident_override_cells.push((id, cell));
            }
            _ => {
                return Err(bad(
                    "structural_overrides",
                    "unknown structural override component tag",
                ));
            }
        }
    }

    for (k, _, data) in rows(
        conn,
        "spawned",
        "SELECT id, 0, schema, data FROM spawned ORDER BY id",
    )? {
        let id = entity("spawned", k)?;
        if id.namespace() != Namespace::Spawned {
            return Err(bad("spawned", "spawned entity is not in spawned namespace"));
        }
        if id.payload() >= clock_v3.next_spawned_id {
            return Err(bad(
                "spawned",
                "spawned entity ID is at or past next_spawned_id",
            ));
        }
        let rec = codec::get_entity_record(&data).map_err(|e| corrupt("spawned", e))?;
        match rec {
            EntityRecord::Building(b) => {
                if buildings_map.insert(id, b).is_some() || residents_map.contains_key(&id) {
                    return Err(bad("spawned", "duplicate spawned entity ID"));
                }
            }
            EntityRecord::Resident(r) => {
                if buildings_map.contains_key(&id) || residents_map.insert(id, r).is_some() {
                    return Err(bad("spawned", "duplicate spawned entity ID"));
                }
            }
        }
    }

    for (id, cell) in resident_override_cells {
        let home = residents_map[&id].home;
        let Some(home_b) = buildings_map.get(&home) else {
            return Err(bad(
                "structural_overrides",
                "resident override references unknown home building",
            ));
        };
        if key(home_b.pos.cell().raw()) != cell {
            return Err(bad(
                "structural_overrides",
                "key cell differs from the home building's cell",
            ));
        }
    }

    let mut lod_map: BTreeMap<EntityId, ws_world::Lod> = BTreeMap::new();
    for (k, _, data) in rows(
        conn,
        "node_lod",
        "SELECT id, 0, schema, data FROM node_lod ORDER BY id",
    )? {
        let id = entity("node_lod", k)?;
        if !buildings_map.contains_key(&id) {
            return Err(bad("node_lod", "level for unknown or tombstoned building"));
        }
        let level = codec::get_node_lod(&data).map_err(|e| corrupt("node_lod", e))?;
        if lod_map.insert(id, level).is_some() {
            return Err(bad("node_lod", "duplicate building level"));
        }
    }
    if lod_map.len() != buildings_map.len() {
        return Err(bad("node_lod", "missing building level"));
    }

    let buildings: Vec<_> = buildings_map
        .into_iter()
        .map(|(id, b)| (id, b, lod_map[&id]))
        .collect();
    let residents: Vec<_> = residents_map.into_iter().collect();
    let tombstones: Vec<_> = tombstones_map.into_iter().collect();

    let toy = ToyParts {
        seed: header.seed,
        dataset_version: header.dataset_version,
        walk,
        buildings,
        residents,
        l1: Vec::new(),
        l2: Vec::new(),
        illness: Vec::new(),
        notables: notables_set,
        tombstones,
        next_spawned_id: clock_v3.next_spawned_id,
    };
    let clock = codec::ClockRow {
        clock: clock_v3.clock,
        next_sequence: clock_v3.next_sequence,
        boundary: clock_v3.boundary,
    };
    read_runtime_and_finish(conn, header, clock, toy, Some(clock_v3.next_event_id))
}

fn read_runtime_and_finish(
    conn: &Connection,
    header: &Header,
    clock: codec::ClockRow,
    mut toy: ToyParts,
    expected_next_event_id: Option<u64>,
) -> Result<World, LoadError> {
    let tombstoned: BTreeSet<EntityId> = toy.tombstones.iter().map(|(id, _)| *id).collect();
    for (cell, k, data) in rows(
        conn,
        "l1_state",
        "SELECT cell, id, schema, data FROM l1_state ORDER BY cell, id",
    )? {
        let id = entity("l1_state", k)?;
        if tombstoned.contains(&id) {
            return Err(bad("l1_state", "tombstoned entity has L1 state"));
        }
        let (pos, walk) = codec::get_l1(&data).map_err(|e| corrupt("l1_state", e))?;
        if key(pos.cell().raw()) != cell {
            return Err(bad("l1_state", "key cell differs from the record's cell"));
        }
        toy.l1.push((id, pos, walk));
    }
    for (k, _, data) in rows(
        conn,
        "l2_state",
        "SELECT id, 0, schema, data FROM l2_state ORDER BY id",
    )? {
        let id = entity("l2_state", k)?;
        if tombstoned.contains(&id) {
            return Err(bad("l2_state", "tombstoned entity has L2 state"));
        }
        let bg = codec::get_l2(&data).map_err(|e| corrupt("l2_state", e))?;
        toy.l2.push((id, bg));
    }
    if header.format_version >= 2 {
        for (k, _, data) in rows(
            conn,
            "illness",
            "SELECT id, 0, schema, data FROM illness ORDER BY id",
        )? {
            let id = entity("illness", k)?;
            if tombstoned.contains(&id) {
                return Err(bad("illness", "tombstoned entity has illness state"));
            }
            let ill = codec::get_illness(&data).map_err(|e| corrupt("illness", e))?;
            toy.illness.push((id, ill));
        }
    }
    let toy = ws_world::ToyWorld::from_parts(toy).map_err(LoadError::Toy)?;

    let mut journal = Vec::new();
    for (sequence, _, data) in rows(
        conn,
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
            conn,
            "scheduled",
            "SELECT sequence, 0, schema, data FROM scheduled ORDER BY sequence",
        )? {
            let i = codec::get_scheduled(&data).map_err(|e| corrupt("scheduled", e))?;
            scheduled.push((unkey(sequence), i));
        }
    }

    let events = load_events(conn)?;
    if let Some(next_event_id) = expected_next_event_id
        && next_event_id != (events.len() as u64) + 1
    {
        return Err(bad("clock", "next_event_id disagrees with events"));
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

/// Explicitly migrates a format-1, format-2, or format-3 save at `source` to a
/// new format-3 baseline-plus-deltas save at `dest`, using the built-in
/// baseline registry (ADR 0025 §4).
///
/// `source` is never modified. On error, `dest` is untouched.
///
/// # Errors
///
/// [`LoadError`] if `source == dest`, `source` fails to load, its pinned
/// baseline cannot be resolved in the registry, or writing `dest` fails.
pub fn migrate_to_v3(source: &Path, dest: &Path) -> Result<Header, LoadError> {
    let registry = BaselineRegistry::with_builtin();
    migrate_to_v3_with_registry(source, dest, &registry)
}

/// Explicitly migrates a format-1, format-2, or format-3 save at `source` to a
/// new format-3 baseline-plus-deltas save at `dest` using `registry`
/// (ADR 0025 §4).
///
/// `source` is never modified. On error, `dest` is untouched.
///
/// # Errors
///
/// As [`migrate_to_v3`].
pub fn migrate_to_v3_with_registry(
    source: &Path,
    dest: &Path,
    registry: &BaselineRegistry,
) -> Result<Header, LoadError> {
    if source == dest || (dest.exists() && fs::canonicalize(source)? == fs::canonicalize(dest)?) {
        return Err(bad(
            "header",
            "migration destination must differ from source",
        ));
    }
    let src_header = read_header(source)?;
    if src_header.generator_version != BUILDING_GENERATOR_VERSION {
        return Err(LoadError::UnsupportedBaseline {
            field: "building_generator_version",
            version: u64::from(src_header.generator_version),
        });
    }
    if src_header.rules_version != 1 {
        return Err(LoadError::UnsupportedBaseline {
            field: "rules_version",
            version: u64::from(src_header.rules_version),
        });
    }
    if registry
        .get(TOY_DATASET_ID, src_header.dataset_version)
        .is_none()
    {
        return Err(LoadError::MissingBaseline {
            dataset_id: TOY_DATASET_ID,
            dataset_version: src_header.dataset_version,
            sha256: [0; 32],
        });
    }
    let world = load_with_registry(source, registry)?;
    // Own a fresh sibling directory: neither the source nor a prior destination
    // can alias its save/staging files. Verify before replacing the destination.
    let temporary = MigrationDirectory::create(dest)?;
    let candidate = temporary.0.join("state.wssave");
    save_with_registry_version(&world, &candidate, registry, 3).map_err(|e| match e {
        SaveError::Io(io) => LoadError::Io(io),
        SaveError::Sqlite(sql) => LoadError::Sqlite(sql),
        SaveError::PendingCommands => bad("header", "pending commands"),
        SaveError::Unsaveable(why) => bad("baseline", why),
    })?;
    let verify = load_with_registry(&candidate, registry)?;
    if verify != world || verify.checkpoint() != world.checkpoint() {
        return Err(bad(
            "header",
            "migrated save failed round-trip verification",
        ));
    }
    let header = read_header(&candidate)?;
    fs::rename(&candidate, dest)?;
    Ok(header)
}

struct MigrationDirectory(PathBuf);

impl MigrationDirectory {
    fn create(dest: &Path) -> Result<Self, std::io::Error> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let suffix = format!(
                ".migration-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let path = sibling(dest, &suffix);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "cannot reserve migration directory",
        ))
    }
}

impl Drop for MigrationDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("state.wssave.saving"));
        let _ = fs::remove_file(self.0.join("state.wssave"));
        let _ = fs::remove_dir(&self.0);
    }
}
