# Native transport fixtures

Retain all ABI1–4 samples as compatibility/refusal references. The ABI5 toy snapshot, UTC label and toy sky samples advance only the first version word of their ABI4 equivalents; existing behavioral coverage remains intact.

The new `slice-geometry-v5`, `slice-snapshot-v5` and `slice-sky-v5` samples use seed71449, configured UTC2026-06-22T11:22:30Z, camera target ENU(0,0,0) with500m interest, then20 explicit ticks at1×. Geometry is the full58-row extract. The snapshot contains all30 identities, including actual fine positions and four exact summaries. Both snapshot and sky deliberately store hash0 as a transport sentinel; live hash semantics are checked separately. The matching captured instant is11:22:32Z.

Source identities/topology/counts/time/version words are exact. Static/dynamic positions/normals are Tier3: tests compare their numeric values/invariants with explicit metre/normal tolerances, rather than promoting every coordinate byte into a Tier1 golden. Dedicated calendar/sun output retains ADR0030's bit guarantee. Rust's source-topology golden separately pins integer ear selection; source coverage, winding and normals are checked independently.

The new samples were generated on arm64 macOS from the built library with `python3 core/crates/ws-ffi/tests/generate-slice-fixtures.py /absolute/path/to/libws_ffi.dylib` (use the matching `.so` on Linux). This is an explicit maintenance command, never a test/CI regeneration step. Any accepted version's later golden change follows the testing policy.
