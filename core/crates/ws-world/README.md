# ws-world

## Responsibility

The world model of the Vertical Slice: the real Bay Ridge block selected in [ADR 0023](../../../docs/adr/0023-vertical-slice-block.md), its synthetic population and their vitals, the site's calendar sky, and the block's immutable render geometry. The Technical Prototype toy world (TP-006 to TP-010: box buildings, L1/L2 walking residents, toy illness and its baseline-plus-deltas saves) was deleted by [ADR 0040](../../../docs/adr/0040-delete-toy-world-and-old-formats.md) (#139).

| Module | Provides |
|---|---|
| `slice` | `SliceWorld`, fixed-point source geometry, four detailed buildings and the 58-footprint context baseline (VS-002); `slice::population` (VS-003) and `slice::vitals` (ADR 0033) |
| `ids` | Procedural identities (ADR 0016): `building_id`, `resident_id`, `ResidentAddress`; the crate's `Purpose` labels (the toy-only ones stay reserved) |
| `environment` | `SOLAR_SITE`, `site_frame`, `calendar_sky` (VS-007) |
| `slice_render` | Immutable block render geometry (VS-008, ADR 0031) |

## Owned state

The immutable structural baseline (`SliceWorld`), the immutable population (`PopulatedSlice`) and the vitals column, all held by `ws-sim`'s `SliceSimulation`. Vitals are the only mutable state here, and only through checked living-health mutation (ADR 0034). Render geometry and the calendar sky are derived and own no state.

## Emitted event types

None. `ws-world` does not depend on `ws-events`; `ws-sim` records history.

## Invariants

- **Tier 1 identity.** Building IDs are `derive(seed, "ws-world/id/building", [dataset_version, key])` and resident IDs `derive(seed, "ws-world/id/resident", [dataset_version, building_key, slot])`, both in the procedural namespace; the slice's building key is the BIN. All integer; `tests/golden/ids.txt` was produced by an independent Python implementation and must match on Linux and macOS CI.
- **Collisions fail.** A duplicate ID fails the whole constructor; nothing is renumbered (see the sections below).
- Integer simulation time only; no wall clock, no threads, no `unsafe`, no unordered maps.

## Dependencies

`ws-ecs`, `ws-math`, `ws-rng`, `ws-time`.

## Real block baseline (VS-002)

`slice::SliceWorld::new(seed)` builds the four detailed buildings selected in
[ADR 0023](../../../docs/adr/0023-vertical-slice-block.md). Its immutable
`buildings()` column iterates in ascending `EntityId` order. Every row retains
BIN, BBL, lot, address, PLUTO use class, lot-level floors and residential units,
roof height above ground, source ground elevation, centroid and full footprint.
`slice::BLOCK_BUILDINGS` retains all 58 source footprints (including 6 garages)
for surrounding context; only the four `featured` rows become entities here.
No synthetic residents or events are emitted by this structural constructor.

The reviewed [VS-001 extract](../../../data/vertical-slice/bay-ridge-block-6049/README.md)
is compiled offline by `scripts/vs_core_buildings.py` into `src/block_6049.rs`.
This small adapter only reads the normalized extract, never the raw upstream
snapshots. It has no runtime I/O, new dependencies, simplification, projection,
or generic simulation-tile pipeline. Refresh with `python3 scripts/vs_core_buildings.py build`
and verify with `python3 scripts/vs_core_buildings.py check` from the repo root.
The existing CI Python discovery checks exact regeneration for every source row.

**Exact geometry (Tier 1):** coordinates retain all twelve decimal places as
signed picodegrees; roof heights and original NAVD88 ground elevations use
integer millimetres. MultiPolygon parts, holes, winding, source vertex order,
and closing vertices are retained. The centroid retains the extract's six
places (about 0.1 m); it is not recomputed from a simplified box. Validation
checks coordinate range, closure, minimum ring length, nonzero integer area,
and positive height; it does not claim general polygon-topology validation.

**Coordinates and vertical datum:** `GeoPoint::surface_position(ellipsoidal_height_mm)`
projects to ADR 0015's `SurfacePos` through portable `ws-math`. The caller must
explicitly supply WGS84 ellipsoidal ground height. Passing zero means the
ellipsoid, not real street elevation. The source `ground_navd88_mm` is retained
as NAVD88 metadata and is never implicitly treated as ellipsoidal; terrain and
a geoid conversion are outside VS-002 (ADR 0023). Roof height remains relative
to ground and does not get added to the footprint location.

**Identity and baseline:** `baseline()` retains seed, dataset version
`0x85183751d4f793b9` and structural generator version 1 for the later save header.
The dataset version is the first 64 bits (big-endian) of the exact GeoJSON's
SHA-256, whose full value is exported as `EXTRACT_SHA256`. Building IDs reuse
ADR 0016's existing purpose and integer address `[dataset_version, BIN]`;
BIN replaces the toy's ordinal, so source order cannot renumber buildings.
No float, address string, centroid, or runtime allocation enters ID derivation.
A changed extract gives a new version; migration between dataset versions is
not currently supported. Later slice persistence must pin/validate its version
before regenerating (#53 / #75); no codec or older save fixture changes here.

**Collisions (#30):** duplicate BINs are rejected before hashing, and duplicate
entity IDs fail the whole constructor with `SliceError::Collision`. There is no
silent overwrite, salt, renumbering, or chosen resolution policy. Unit tests
inject a collision between distinct addresses. The existing City-stage ADR
follow-up remains open; it is not a dependency of a four-building slice.

**Verification:** `tests/golden/block_6049.txt` freezes IDs, integer centroids,
heights and use for seeds 0, 42 and `u64::MAX`; the ID oracle was an independent
Python SipHash-2-4 implementation checked against the then-existing toy building
goldens. Both Rust CI platforms run it. New tests also cover exact extract
regeneration, nested geometry/closure, rejected malformed data, source-order
independence, 10,000 seeds (40,000 building IDs), and fixed-seed properties for
`SurfacePos` containment, idempotence and ECEF error below 0.1 mm at explicit
ellipsoidal heights.

This constructor owns only structural baseline data. No ADR decision changes
are needed: real-data building keys are already allowed by ADR 0016.

Load time is negligible at 58 validated source rows and four inserted entities
(#66); there is no tick-loop change and no separate performance measurement.


## Synthetic block population (VS-003)

`ws_world::slice::population::PopulatedSlice::new(seed)` wraps the validated real-building `SliceWorld` with an immutable synthetic population. The building-only constructor is unchanged.

The four selected source classes/residential-unit counts (S1=1, A5=1, B2=2, C2=6) give exactly ten occupied homes. One household is generated per home; each gets 1–3 people from its own counter-based stream, guaranteeing 10–30 people for every seed. The first member is an adult aged 22–80; other members are 0–90. This is a small explicit synthetic policy, not a Census-calibrated demographic model or a claim about actual occupants. Ages 0–4 are preschool, 5–21 students, 22–64 employed, 65–90 retired. Names come from small hand-authored tables; no real-person dataset is used. Household members share a generated family name.

Household IDs use `ws-world/id/household` with `[dataset_version, BIN, unit]`. Resident IDs reuse ADR 0016's purpose/address, with BIN as building key and flat slot `unit*3 + member_slot`. Workplace IDs use `ws-world/id/workplace` with `[dataset_version, BIN, 0]` for the shop and `[dataset_version, 0, 0|1]` for off-map work/school. New IDs are checked against **every** existing building/household/resident/workplace ID; any collision fails without renumbering or a partial result. Adoption of ADR 0024's City collision policy remains a separate implementation, not a silent change to these generators.

Random attributes use separate `Stream(seed, purpose, [stable_id]).draw(0)` choices and integer `below`; source order and unrelated draws cannot shift identities. The immutable columns iterate by ascending ID. The shop's synthetic job capacity comes from its floor count (clamped 1–3). Canonical lowest-ID employed residents fill that capacity; other employed residents use an off-map employment destination, and all students use an off-map school. Preschool/retired residents have no workplace. Every resident has exactly one home and household.

Two exact-integer synthetic gateways sit on the west/east edges of the pinned extract's footprint extent, halfway between its north/south extremes. They are coarse map exits, **not surveyed sidewalks, road intersections or converted ground elevations**. Employment/school records reference their exit index; no resident disappears simply because the destination is off-map. VS-005 supplies the actual sidewalk graph.

`PopulationBaseline` retains the unchanged structural seed/dataset/loader version and a separate population generator version (1). Changes to address mapping, size/age/assignment policy or name tables require a population-version bump and normal golden review. Existing building Tier 1 fixtures are unchanged. An independent Python SipHash oracle, first checked against every existing real-building ID golden, freezes all household/resident/workplace IDs, names/ages/roles, memberships, assignments and gateway coordinates for seeds 0, 42 and u64::MAX. Rust compares the entire canonical output on both CI platforms. Regenerate only after an authorized version change:

```sh
python3 scripts/vs_population_golden.py check
# After version/fixture review approval only:
python3 scripts/vs_population_golden.py build
```

Fixed-seed property tests verify the home/unit/member counts, adult head, exact role ranges, cross-kind unique IDs, accepted resident address derivation, family/name attributes, destination/capacity references and regeneration. Unit tests force cross-kind collisions, reverse source order, reject changed source-home counts and verify purpose separation. No tick-path cost is added; measurements are unnecessary at this baseline's scale per #67. No new dependency or data source.

## VS-007 derived calendar/sky

`environment` composes exact UTC, bounded New York policy1 and pure solar rule1. `SOLAR_SITE` is computed at compile time from the complete58-source-footprint envelope with i128 midpoint/floor arithmetic; the pinned registered dataset supplies its version. `calendar_sky(time, render_basis)` returns exact UTC throughout SimTime and explicit unavailable local/sun status outside UTC2000–2099. The sun is rotated site ENU→ECEF→the supplied render ENU basis; azimuth/elevation remain fixed-site values. The query direction has an exact Tier 1 golden on `site_frame()` (`tests/golden/solar_site_v1.txt`; the toy-frame golden was replaced by ADR 0040); general coordinate policy stays Tier3. This owns no mutable state/events and consumes no RNG.


## VS-008 immutable render geometry

`slice_render` implements ADR0031's source/frame version1: all58 registered footprints become concave roof/wall extrusions in the fixed site ENU frame, with exact integer topology and separate BIN/featured EntityId mapping. Deterministic integer ear quality and source-order tie breaking avoid sliver roof shading; all original wall edges remain. Geometry is bounded, derived and cached by the adapter, never authoritative state or an event producer. Explicit geodetic/ECEF/ENU conversions use ellipsoidal height0 plus source above-ground roof height; NAVD88 is not treated as ellipsoidal. New topology goldens, exact area/source coverage, winding/unit normals and Tier3 coordinate round trips preserve every earlier baseline/golden.

## Resident vitals (ADR0033)
The separate version1 vitals column has one retained row per existing resident ID, with an independent seeded female/male draw, health0..100 and explicit Alive/Dead status. Initial health is100 and life status Alive. Checked values require living health1..100 or dead health0. This increment adds stored state, with no disease/mortality transitions. Sex is independent of names and old population streams; existing population1 goldens remain unchanged. The versioned digest covers every ID/field. Future transitions need event, behavior, occupancy and persistence integration.

ADR0034 adds only checked living-health mutation to the retained slice vitals column. It preserves sex/life and rejects missing/dead residents or health outside1..100. The orchestrator stages illness effects and recovery atomically; no raw component setter is exposed through FFI. Mortality, contagion and new initial health sources remain outside #74.
