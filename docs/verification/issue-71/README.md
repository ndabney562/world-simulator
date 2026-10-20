# VS-007 calendar, sun and controls verification

Implements [#71](https://github.com/ndabney562/world-simulator/issues/71) under accepted [ADR0030](../../adr/0030-calendar-sun-and-sky-query.md). Base: `82ee787b2ed0fa30286b2a859500e5b11f01c5e5`. Branch: `codex/71-calendar-sun`. The PR records the published/tested head and independent review. [File hashes](tested-files.sha256) pin the runtime, tests, assets, settings, fixtures and check scripts used here; verify from the repository root with `shasum -a 256 -c docs/verification/issue-71/tested-files.sha256`.

## Environment and results

2026-10-20, Apple M1 Pro / arm64, macOS27.0.1, workspace-pinned Rust1.97 toolchain, Unity6000.3.26f1 with Metal and graphics enabled. Unity ran in an isolated copy with a freshly built arm64 ABI4 plugin and the existing licence. Every copied Unity source/asset/configuration file still matched the author checkout after import. No signing, device deployment or account change occurred.

- Full release workspace: **406 passed, 0 failed, 4 existing ignored**. Existing nightly tests remain unchanged.
- Rust format, workspace/all-target clippy with warnings denied, all-target build and dependency checks passed.
- Python: **61 passed**. Docs, exact source-extract regeneration and static Unity checks passed.
- Pinned Editor: **44 passed, 0 failed, 0 skipped**, including all37 prior tests. [Test XML](unity-editmode.xml).
- Generated header, native C consumer, iOS Rust archive and CLI record/replay/save/load are checked locally and again by hosted CI. The PR records their final outcomes and CI links.

No old core ID, position, schedule, behavior, event or save golden changed. New solar/query-frame bit goldens are checked on both Rust CI platforms. Existing pixel, framing, allocation, ingress, pause, overflow and buffer assertions remain enabled.

## Published solar reference

The site is the complete58-footprint envelope midpoint of the registered block6049 extract, computed with integer i128 sums and floor division: latitude40.622079256973°, longitude−74.030160593242°; dataset `0x85183751d4f793b9`. No new geospatial source is used.

Reference: [NOAA calculation details](https://gml.noaa.gov/grad/solcalc/calcdetails.html), [published calculator JavaScript](https://gml.noaa.gov/grad/solcalc/main.js), retrieved2026-10-20. Source SHA256: `3832956f24724eafacf9e18173b9299b1554d6252bed0784d9c9aca6d9f54856`.

The inspected pure calculation section ran independently in an isolated Node VM with no DOM, cookie, filesystem or network capability. `calcRefraction` alone returned zero to obtain geometric elevation, matching ADR0030. Inputs: `T = calcTimeJulianCent(getJD(year,month,day)+UTC_minutes/1440)`, UTC minutes as localtime, zone0, and the coordinates above. Reference JavaScript is not included in the product or tests. The Rust implementation computes the unit ENU direction directly from declination/hour angle using pinned libm.

| UTC | Azimuth, clockwise from north | Geometric elevation |
|---|---:|---:|
|2026-03-20T12:00:00Z|99.32855895324268°|10.63581004236454°|
|2026-06-21T17:00:00Z|181.56003142758370°|72.81041186202198°|
|2026-12-21T02:00:00Z|281.75774769010470°|−49.82822974574560°|

Each angle must agree within **0.1°**, using circular azimuth difference. Separate new rule1 fixtures pin exact f64 bits for site angles/vectors and the dedicated site ENU→ECEF→actual render ENU conversion, across four seeds. Property tests cover finite/unit output, range/poles, angle consistency, circular rounding and leap-label continuity. General coordinate conversions retain Tier3 policy.

## Calendar, replay and native coverage

Integer tests cover spring gaps/fall repeats under both2000–2006 and2007+ New York rules, leap days/seconds, full-range exact UTC, unavailable local/solar status, and model-Monday lower-edge/year/DST boundaries. An independent host `America/New_York` oracle checked UTC/local labels and predecessor Mondays at500 creation instants spanning every supported year; [result](local-policy-check.txt). This is verification only, with no product timezone dependency.

Initial civil alignment stores an explicit uniform model-week epoch. Later DST/leap boundaries do not retime accepted synthetic appointments. Raw creation configurations remain compatible; the new alignment helper rejects unsupported creation years with a typed error. The special1999-12-27 EST predecessor applies only to alignment at the supported lower edge.

Native tests cover every40/160-byte field offset/alignment, null/misaligned pointers, exact version/size/reserved checks, wide invalid civil fields, conversion overflow, sub-grid conversion versus creation rejection, request-only initialized output, poison and unchanged output on errors. Queries preserve pending commands, state, history and ticks. Managed fill holds one gate across render/sky and checks matching ticks before publication. Speed scripts compare every calendar/state/checkpoint boundary in the same build and in separate processes; paused host pacing advances no time.

ABI4 retains all v1/2/3 fixtures and the prior C consumer source. Only obsolete ABI success expectations, the new fixture selections and the C CI entrypoint advance under ADR0030's explicit acceptance. Command envelope2 and existing structure/record shapes stay unchanged. New shared `utc-label-v4.hex` and `calendar-sky-v4.hex` fixtures are checked against Rust/native and actual C# marshalled bytes. The sky transport fixture deliberately uses a zero diagnostic-hash sentinel; real canonical hash semantics are asserted separately, avoiding a new cross-platform Tier2 world golden.

## Actual Editor rendering and worker measurement

The final control test enters Play Mode, explicitly loads Bootstrap and waits for activation. It exercises the same `SetSpeed` path used by pause/1×/10×/100× buttons, paired snapshot ticks, unchanged paused time, core-derived HUD labels and the directional light. Test code containing captured lambdas is created after the Play Mode domain reload. Prior marker/framing/pixel and allocation checks remain intact. New warmed tests verify zero managed allocation for native fill/calendar pairing and unchanged-label presentation.

After2 seconds warmup per scale, each scale runs for10 seconds, uncapped to measure headroom. Every measured iteration submits a URP render request to960×540 and synchronously reads one pixel to wait for preceding GPU work. The interval includes the actual Editor loop, native fill/query/decode, completed rendering and measurement overhead. The final image must contain native geometry. This avoids treating an unrendered batch update loop as frame-rate evidence. [Unity render-request API](https://docs.unity.com/en-us/engine/6000.0/manual/cameras/urp/multiple/user-render-requests), [synchronous ReadPixels behavior](https://unity.com/blog/engine-platform/accessing-texture-data-efficiently).

| Scale | Completed frames/s | Mean frame | p95 frame | Max frame | Mean fill/query/decode | Mean fenced render |
|---|---:|---:|---:|---:|---:|---:|
|1×|643.06|1.555ms|2.375ms|5.096ms|0.0236ms|0.761ms|
|100×|633.25|1.579ms|2.486ms|6.119ms|0.0236ms|0.769ms|

Worker throughput was9.999 and999.917 physical ticks/s respectively. At100× the measured mean native step was0.0047ms, maximum0.0457ms. Assertions require at least60 completed frames/s and worker throughput within98–102% of the requested rate. The backend is explicitly **toy**, with30 residents and5 buildings; the real slice's maximum population is30.

Evidence: [raw per-frame CSV, gzip](editor-frames.csv.gz), [summary](editor-summary.json), [rendered image](editor-100x.png), [full Editor test result](unity-editmode.xml). The CSV identifies backend, population, scale, wall time, tick, frame/fill/worker/fenced-render timing. Reproduce with the full EditMode command in the [Unity README](../../../apps/unity/README.md), with graphics enabled and no competing CPU benchmark.

## Real-slice core CPU headroom

`cargo bench --locked -p ws-sim --bench calendar` separately measures the pure committed query and actual real-slice100× host pacing/domain work. [Criterion output](calendar-bench.txt):

- Toy30 committed calendar query including canonical hash: **3.1197–3.1293µs**95% confidence interval.
- Real slice30, seed71449, one16/17-tick100× host frame plus calendar/hash projection: **49.110–49.428µs**95% confidence interval.

Cloning and destruction are outside the measured batched workload. At100× the accepted camera policy caps detail atL2. The real-slice projection here uses an explicitly supplied site frame; it is a core CPU benchmark, not the future native real-slice renderer. No other CPU measurement ran concurrently.

## Remaining platform evidence

#72 owns real footprints/resident/camera transport and its actual device screenshot/frame CSV. #76 owns the20-minute iPhone run and the Product Owner's stage-exit decision. Desktop Editor headroom does not establish phone resolution, thermal behavior or device memory. No device or stage-exit result is claimed here.
