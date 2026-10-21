# VS-009 engineering verification: tap-to-inspect

Issue [#73](https://github.com/ndabney562/world-simulator/issues/73), accepted [ADR0032](../../adr/0032-read-only-slice-inspection.md). Engineering evidence on macOS; #73 remains open for iPhone acceptance. No signing, installation, device performance or stage-exit claim is made.

## Source and contract

Branch `codex/73-inspection`, based on main `9bd1e4bb16c98a626fae3364f5b19a1c08ce5669`, after real-block implementation #95 and inspection design #97 merged. `tested-files.sha256` records runtime code, tests, fixtures, scripts, Unity assets/settings and data inputs; verify at repository root with `shasum -a256 -c docs/verification/issue-73/tested-files.sha256`. Documentation-only edits do not alter those hashes. Remote commit, final independent review and CI are recorded on the implementation PR.

Rust projects a borrowed snapshot of an existing resident/building: identity, CommonSnapshot needs/activity/logical location, exact model timetable or building summary, sampled tick/time/hash and at most16 factual actor events. It does not reconstruct fine state, run simulation or create history. Recent events use a derived per-actor occurrence-time/EventId index, inclusive cutoff and at most17 visited matches after a binary search. The original append-order actor index/query remains intact; backdated entries, duplicate actor roles and rebuilds are covered.

ABI6 adds one bounded8192-byte inspection query. Other record layouts remain unchanged. Old ABI1–5 fixtures and consumer sources remain; six current-layout v6 fixtures preserve the v5 body bytes after the version word. Two new inspection fixtures share native/managed decoding checks. Existing fixture hashes use the documented zero sentinel only for hash fields. Short-buffer, invalid-target/control-pointer, poison and injected-unwind tests preserve caller-data publication rules.

Unity uses the last rendered building triangles and actual capsule transforms for nearest-hit picking. Drags, returning drags, second touches, UI gestures and long presses cancel selection. A successful tap alone acquires the native gate and validates the complete sample and matching calendar frame before replacing the panel. Frames never poll the selection. Re-tap refreshes, close/empty clears, context footprints identify source BIN without inventing people. Narrow views replace the HUD with the inspector; wide views place it beside the HUD. Both remain inside the reserved250-pixel panel with scrolling.

## Checks and evidence

- Pinned Rust1.97.0: workspace formatting, all-target locked clippy with denied warnings, release build and workspace release tests passed:434 passed,0 failed,4 pre-existing ignored. Final native suite29/29 passed. Actual C ABI6 consumer, dependency-direction check and generated-header comparison with cbindgen0.29.2 passed.
- All61 Python tests, docs/static Unity checks and whitespace checks passed. CLI record/replay10000ticks and save2000/load500 smoke checks passed. Rust `aarch64-apple-ios` static archive builds; Unity iOS Build Support is absent locally, so no Unity iOS export is claimed.
- Actual Unity6000.3.26f1 on Apple M1 Pro arm64/macOS27.0.1/Metal: full63 Editor tests passed, no skips/failures. Existing58 tests remain; five inspection tests cover fixtures/all identities, malformed atomic decode, gesture cancellation, mesh occlusion/scaled distances and native-backed actual-scene taps without polling. XML is committed alongside this note. Per-case durations include play-mode work; root NUnit duration can reset during domain reload.
- The chronological-index probe retains128bytes for its16-reference result at1000,10000 and100000events; median query times were130,155 and135ns respectively on this desktop. `recent-index.csv` includes the entire arena/index capacity at each size. The history itself still grows; bounded query work is not a retention policy or device-memory result.

`building-inspection.png` and `resident-inspection.png` are actual visible Editor Game-view captures of the native-backed panel, not mockups or device screenshots. The separate visible scene test passed1/1 (`unity-visible-scene.xml`). Text files retain the complete selected record, including content below the scroll fold. ScreenCapture does not emit these PNGs in batch mode, so the scene test was also run in a visible Editor with a repaint and0.5second wait before capture. The world camera is not reliably refreshed in that screenshot path at the street pose; the resident PNG establishes the inspector display, not street rendering. Existing issue72 camera-render evidence/tests establish the actual capsule rendering separately.

Visual QA found that IMGUI content outside the reserved top strip did not survive this camera/Editor render path. The panel now stays inside that strip; narrow layouts replace the main HUD. An opaque strip clear prevents old glyphs accumulating where the world camera never clears. The corrected captures were visually inspected; visible-test code and runtime inputs are pinned by the manifest. `editor-frames.csv.gz` and `editor-summary.json` come from the full suite's existing real-slice GPU-fenced benchmark, not from inspection screenshots and not from an iPhone.

## Independent review fixes

Local independent review found and rechecked two Major fixes: inverse capsule transforms must retain unnormalized ray direction so intersection distance stays in world units; commute arrivals/endpoints and sampled model-week activity must agree with neighboring timetable intervals. Separate regressions now cover capsules just before/behind a roof and corrupted arrival/destination/activity bytes. The reviewer also checked explicit invalid-target versus query-failure classification. Published final-head review and all five CI checks remain required before merge.

## Remaining acceptance

On the authorized device run, tap a visible native resident and each featured building, verify identity/needs/model timetable/counts/recent events, scroll, close and re-tap refresh. Verify drag/orbit/pan/pinch and HUD controls do not trigger inspection, then retain device screenshots and performance evidence. Issues #72/#73 and the stage-exit/device gates remain open. No new source, dependency, persistence format or intervention is introduced.
