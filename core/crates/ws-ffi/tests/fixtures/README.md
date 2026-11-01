# Native transport fixtures

ABI 11 samples only ([ADR 0040](../../../../../docs/adr/0040-delete-toy-world-and-old-formats.md)); older ABI words are refused by tests, not kept as fixtures. Rust and Unity both read these files.

Every file is generated from a built library with `python3 core/crates/ws-ffi/tests/generate-slice-fixtures.py /absolute/path/to/libws_ffi.dylib` (use the matching `.so` on Linux). This is an explicit maintenance command, never a test or CI step. Any later golden change follows the testing policy.

- **Calendar:** `utc-label` is the 40-byte UTC label 2026-06-21T17:00Z. `calendar-sky` is the 160-byte sky of a seed-7 slice created at that instant, at tick 0.
- **Slice:** seed 71449, configured UTC 2026-06-22T11:22:30Z, camera target ENU (0,0,0) with a 500 m interest radius, then 20 explicit ticks at 1×. `slice-geometry` is the full 58-row extract. `slice-snapshot` holds all 30 identities, with actual fine positions and four exact summaries. `slice-sky` is the matching sky (captured instant 11:22:32Z).
- **Inspection:** `inspect-resident` (1135 bytes) and `inspect-building` (299 bytes) from that boundary, schema 3. `inspect-ill` and `consequences` follow one severity-5, two-tick affliction of the sample resident.
- **God powers (ADR 0038):** after that sequence, `consequences-bless` (bless curing the sample illness: rows 14, 12), `consequences-storm` (a one-minute storm run past its end: rows 16, 18, 19, subject 0) and `consequences-gathering` (a gathering at the first summary building: root row 20 only). The generator reads each root with `ws_slice_last_root`, since a storm root has no actor to inspect.

Every sample stores a zero canonical-hash word as a transport sentinel; live hash semantics are checked separately. Source identities, topology, counts, time and version words are exact. Static and dynamic positions and normals are Tier 3: tests compare them with explicit metre and normal tolerances instead of treating every coordinate byte as a Tier 1 golden. Calendar and sun output keeps ADR 0030's bit guarantee. `ws_slice_god` has no fixture; the Rust and C smoke tests pin its layout directly.
