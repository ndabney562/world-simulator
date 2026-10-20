# ADR 0030: real calendar labels, deterministic block sun and coarse sky query

- Status: Proposed
- Date: 2026-10-20
- Decided by: Noah under the named technical delegation on#77, after independent review; proposed by Codex
- Issue: #71 (VS-007 design phase)
- Supersedes: none; extends ADRs0014/0018/0022 and coordinates ADR0026's conditional ABI reservation

## Context

VS-004–006 are merged. [#71](https://github.com/ndabney562/world-simulator/issues/71) requires a configured real start date, UTC/New York labels, a core-owned reproducible sun,100× controls, a date/time/speed HUD, and a light driven by that sun. Integer SimTime and exact UTC/leap conversion already exist, as do native pause/1/10/100 scales and worker pacing. The demo exposes only pause/1/10 buttons and no calendar/sky snapshot.

The current Unity bridge still presents the toy world; #72 owns real footprint/resident/camera transport. The new read-only environment contract must work now and remain explicit about its site/frame. No renderer may invent simulation time or calculate its own authoritative sun. This PR changes only docs; reviewed acceptance and merge precede separate implementation.

## Options considered

1. Compute date/sun in Unity using its clock or platform timezone. Rejected: replay and cross-platform identity depend on unrecorded host state.
2. Add a timezone/astronomy service or package now. Rejected: unnecessary runtime dependency/network/data feed for this block.
3. Reuse core UTC time, add a bounded versioned New York labelling policy and libm solar model, and expose one coherent coarse query. Chosen: independently testable with no new dependency or service.

## Decision

### Clock, configuration and civil labels

Preserve ADR0014's signed uniform-SI microseconds, UTC epoch, leap table and full SimTime range. Existing Rust/native `start_us` remains canonical configuration; no float time or new time representation is introduced. A pure coarse UTC-label conversion lets a host pass a chosen civil start without duplicating leap arithmetic. A configured start must still lie on the100-ms grid before world creation. Invalid labels, unsupported SimTime conversion and invalid start grids fail explicitly without partial creation.

A host may capture current UTC once at new-world creation, or submit an explicitly chosen UTC label; it stores the resulting SimTime and never reads the device clock to drive subsequent calendar/sun evolution. Tests and verification scenes choose fixed starts. Changing the population creates a new configured world, not a hidden calendar mutation of an existing one. Existing low-level callers may still choose raw SimTime, including extreme test instants.

UTC output remains exact across the full SimTime range, including second60. Add a distinct local-label type with offset/zone tag; never construct a New York leap-second label as though it were a UTC label. The supported New York/solar policy window is UTC years2000–2099 inclusive. Within it, use EST−18,000s and EDT−14,400s: for2000–2006, start at02:00 local standard time on April's first Sunday and end at02:00 local daylight time on October's last Sunday; from2007, use March's second Sunday and November's first Sunday. These are frozen policy-v1 rules, not a claim that future laws cannot change. See [NIST rules](https://www.nist.gov/pml/time-and-frequency-division/popular-links/daylight-saving-time-dst) and the [2007 federal change notice](https://www.fdic.gov/news/inactive-financial-institution-letters/2007/fil07017.html).

Compute transition instants in UTC with integer Gregorian arithmetic and decide the offset from the instant. UTC→local is unique even in a repeated hour; include the offset so those labels remain distinguishable. Preserve second60 while shifting day/hour/minute fields. Test spring gaps, fall repeats, leap days, year boundaries and tabulated leap seconds. Outside the supported window, return exact UTC plus explicit unavailable local/solar status, rather than invent historical/far-future timezone or ephemeris data. Clock stepping itself remains unrestricted.

Real-calendar labels do not silently retime accepted ADR0027/0028 synthetic appointments or utility counters. Provide an explicit helper that aligns a new slice's stored model-week epoch to the most recent New York Monday midnight at creation, using the supported civil policy. That value is stored as SimTime and versioned configuration, not inferred each frame. The existing synthetic timetable remains a uniform elapsed model week after that epoch, including across a later DST/leap boundary; the UTC/local HUD always shows the correct policy label for the actual instant. This phase alignment is not a claim of civil appointment retiming across DST. A later civil timetable policy must use new schedule/behavior versions and reconciliation tests, not replace old model-week goldens. No such retiming is hidden in this calendar/presentation issue.

The alignment helper accepts only creation instants in the supported UTC window. At its lower edge, the predecessor Monday can be in December1999: explicitly allow the bounded alignment-only predecessor1999-12-27T00:00 New York with EST−18,000s (1999-12-27T05:00Z). This does not extend general local/solar queries outside UTC2000–2099. For example, creation at2000-01-01T00:00Z uses that predecessor, never the next Monday. Reject creation outside the window with a typed unsupported-alignment error, and test the lower edge, year crossing, exact Monday midnight and both DST transition weeks.

### Core-owned solar model and frames

Keep the existing dependency direction: `ws-time` owns integer civil/start/local-label helpers; `ws-math` owns pure libm solar mathematics with numeric inputs and no dependency on a peer foundation crate; `ws-world` composes the fixed registered site with time/math; `ws-sim` exposes a coherent immutable world-boundary view; `ws-ffi` marshals through its existing ws-sim dependency. Unity owns presentation. No new crate or third-party dependency is needed.

Use the already pinned block6049 extract's complete footprint-envelope midpoint as the fixed site longitude/latitude. For each source-picodegree coordinate, compute `(minimum+maximum).div_euclid(2)` with an i128 sum, rounding down even for negative longitude. No new geospatial source, elevation or terrain claim. Pin solar rules version1, New York policy version1, existing leap-table version and site dataset version in the environment output.

Implement the NOAA/Meeus solar-position method using the existing pinned pure-Rust libm for every transcendental operation. The calculation includes apparent solar longitude/obliquity, declination, equation of time and local hour angle, yielding geometric azimuth (clockwise from true north), elevation and a unit vector pointing toward the sun. Reference: [NOAA calculation details](https://gml.noaa.gov/grad/solcalc/calcdetails.html). Do not include atmospheric refraction, weather, terrain occlusion, moon/planets or a network ephemeris. NOAA's calculator is a reference model, not a newly required live service.

Build a nominal UTC Julian day from the integer civil day and within-day label only at the point of use; do not treat uniform SimTime microseconds as POSIX/UTC days or mislabel its epoch as TT. UTC approximates the rotational time used by this model; UT1/polar-motion and astronomical TT/TDB precision are outside scope. At a tabulated leap second, the nominal day formula may share the following midnight's phase; preserve the distinct integer instant/UTC label and test finite, continuous bounded solar change. The stated geometric reference tolerance accounts for this approximation.

Use existing frame-tagged vectors and explicit transforms. The core solar result is in the site's geodetic ENU frame. The native query below returns that direction rotated through ECEF into the **same ENU basis used by the paired render snapshot**; its azimuth/elevation remain site values. Do not pass ADR0028 navigation millimetres as ENU. Unity performs only `(east,north,up)→(east,up,north)` and points the directional light opposite the toward-sun vector. Night/unsupported status turns direct sunlight off; controlled ambient illumination remains presentation. Conversion into a later real-slice frame belongs to #72's explicit adapter.

Solar values and the dedicated query-direction conversion must meet #71's Tier1 requirement: pin operation order/constants, use libm rather than platform trig, avoid FMA-dependent paths, and compare new exact bit goldens on both Rust CI platforms. This does not reclassify all existing coordinate conversions from Tier3. Separately compare azimuth/elevation at three fixed block instants to an independently executed published NOAA reference, with maximum0.1° error (circular azimuth difference). Freeze source URL/method, input coordinates/UTC labels and reference values; cover different day phases/seasons, daylight/night and leap/DST boundaries. No old Tier1 golden changes.

The sun is a pure derivation of the committed instant, site and pinned policy; queries do not add mutable sky state, consume RNG, change hashes/history, flush commands or advance time. Return a canonical world hash for query-independence tests, plus explicit solar/site/time versions for interpreting derived output. Later slice persistence must pin those versions alongside its existing time/baseline configuration.

### ABI4 and coherent coarse output

Consume bridge ABI4 for this implementation if still unused at implementation start. ADR0026 explicitly makes its4 reservation conditional; this accepted implementation consumes it first. Before #13 later implements its provider query, reconcile that ADR's number to the next unused ABI as it requires. #72 also chooses its next unused version with its own ADR. Never ship incompatible definitions of the same number.

Bump native preflight and config/control/render-header ABI fields, generated header, managed binding/decoder and new fixtures together. Existing config32-byte/control16-byte/render-header24-byte/instance32-byte shapes and version2 command envelopes/opcodes remain unchanged. Keep old fixtures for explicit ABI1/2/3 refusal; stale native libraries fail version preflight before a missing symbol is called.

Add a40-byte,4-byte-aligned `WsUtcLabel`: ABI u32 at0, size u32 at4, year i32 at8, month/day/hour/minute/second/microsecond u32 at12/16/20/24/28/32, reserved-zero u32 at36. `WsStatus ws_utc_to_sim(const WsUtcLabel*, int64_t*)` validates exact version/size/reserved/civil label and writes the converted integer only on success. It is a pure configuration conversion with normal thread-local diagnostic/panic behavior; it accepts valid sub-grid instants, while existing world creation separately enforces its grid.

Add `WsStatus ws_get_calendar_sky(WsHandle*, WsCalendarSky*)`. Caller sets ABI/exact size; success writes this160-byte,8-byte-aligned record. Invalid input/version/size, poisoned handle or caught panic writes no output. Pointer ownership, diagnostics and poison rules stay consistent with existing queries. All floats are finite.

| Offset | Type | Field |
|---:|---|---|
|0/4 |u32/u32 |ABI4 / size160 |
|8/16/24 |u64/i64/u64 |committed tick / SimTime microseconds / canonical state hash |
|32 |i32 |UTC year |
|36–56 |6×u32 |UTC month/day/hour/minute/second/microsecond |
|60 |i32 |New York year |
|64–84 |6×u32 |New York month/day/hour/minute/second/microsecond |
|88/92 |i32/u32 |UTC offset seconds / authoritative committed scale tag |
|96/100/104/108 |4×u32 |solar version / NY policy version / flags / leap-table version |
|112 |u64 |site dataset version |
|120/128/136 |3×f64 |toward-sun east/north/up in the paired render basis |
|144/152 |2×f64 |site azimuth/elevation degrees |

Flags bit0 means supported local/solar output; other bits are zero. If unsupported, local fields/offset and all solar floats are zero, bit0 is clear, and UTC/tick/hash/scale/version/site fields still describe the actual committed world. Unsupported range is a successful explicit-status query, not a poisoned handle or fabricated date. Read-only errors preserve caller output bytes.

The host holds its existing single-handle gate across render fill and this query and verifies equal ticks before publishing the paired frame. Worker mutation cannot interleave. Return managed value structs and reuse caller-owned buffers; do not introduce per-entity calls or per-frame allocations. Raw C callers remain responsible for serialized ownership. Sun/environment computation is core work; Unity consumes it once per published frame, not once per resident.

### Unity controls, verification and migration checks

Expose pause/1×/10×/100× using the existing structured scale command. Show configured UTC/New York date/time/offset and committed speed in the HUD, and update the directional light from the coherent core direction. UI formatting/caching is presentation; it does not maintain a second simulation clock or recalculate the sun. Preserve worker cap/drop/remainder and paused command-flush behavior. Deterministic tests use fixed starts and no wall-clock dependence; real host creation may capture UTC once as described above.

ABI migration necessarily replaces obsolete success-version expectations. Explicit technical acceptance covers **only** advancing ABI-specific expected version fields/new ABI4 fixtures and the C-consumer CI entrypoint, while preserving all previous ingress/flush/pause/control/layout/error/buffer/pixel/allocation properties. Retain old ABI3 fixture/consumer source as compatibility references; the new consumer and managed/native tests explicitly assert refusal of1/2/3 before use. Do not simply run an obsolete success consumer and accept any failure. Add exact layout/offset/coherence/null/size/version/poison/unchanged-output/read-only-pending-command tests for both new functions. No core ID/position/rule/save golden is replaced, and no unrelated static/scene/provider/registry/graphics check is relaxed.

Run the pinned Unity6000.3.26f1 Editor with graphics for control/HUD/light/decoder/layout/play-mode and existing marker/allocation tests. Use fixed civil starts in deterministic render tests, preserving their behavioral/pixel assertions. Record actual100× worker throughput and Editor frame/step/fill CSV at slice-scale population, plus explicit population/backend: a toy-demo30-person run is not a claim that #72 already renders real residents. Repeat the actual real-slice native/renderer budget in #72/#76. If an Editor requirement cannot be exercised, leave that evidence outstanding rather than call static/iOS compilation equivalent.

## Required implementation evidence

- Integer UTC/start and bounded New York labels: valid/invalid civil dates, grid checks, leap days/seconds, spring gaps/fall repeats, offsets, range status and model-epoch alignment/phase preservation.
- New exact cross-platform solar/frame bit goldens and three published-reference comparisons at≤0.1°, with unit/finite/night/boundary properties and explicitly pinned versions/site inputs.
- Both new native functions' layout/ownership/errors/no-output-on-failure; read-only query invariance and coherent tick pairing; ABI4 fixtures in Rust/C#, explicit old-client refusal and fresh generated header/native C/iOS Rust archive checks.
- Same fixed-start speed/host scripts replay per-boundary state/history/operation outputs; pause adds no host-paced ticks; existing overflow and integer-time properties remain unchanged.
- Actual pinned Editor control/light/HUD/play-mode/allocation tests and100× worker/frame CSV, with source SHA/backend/population and honest missing device evidence.
- All existing tests/static checks, no unrelated validation weakening, and separate implementation PR after accepted design merge.

## Consequences

The host can configure a real date without reimplementing the core epoch/leap table; one coherent query supplies display time and the authoritative sun. A bounded explicit NY/solar model avoids new services while making unsupported instants visible. The native version advances with compatibility refusals and retained behavioral coverage. Synthetic model-week appointments remain explicitly distinct from civil labels; future retiming needs a versioned decision.

Revisit when civil appointment retiming, a larger site range, changed timezone/leap policy, weather/terrain/refraction or astronomical precision is required. #72 real-slice rendering/bridge, #73 inspection, #74 illness, #53/#75 whole-slice persistence and #76 device evidence/stage exit retain their responsibilities. No credential/cost/signing/device or stage-exit approval is included.
