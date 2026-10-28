# Desktop browser release preparation (#112)

## Scope and decisions

Noah authorized work in the Desktop local checkout on 2026-10-28: desktop browsers first, phone browsers later. He subsequently approved the ADR0035 browser approach and MIT for code he owns; recorded on #112. ADR acceptance is a separate docs-only PR #113. Publication remains a final decision. Native iPhone acceptance (#72/#73/#76) is separate.

## Reproduce the complete player

Use macOS, Unity 6000.3.26f1 plus its WebGL Support module, Rust 1.97.0 with `rust-src`, and Node for the contract tests. No paid server/API, new account, secret or cloud Editor licensing setup is introduced.

```sh
scripts/build-desktop-web.sh
node --test tests/web/page.test.cjs
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The build writes `apps/unity/Builds/DesktopWeb/`, an ignored generated directory. It includes the real RealBlock scene and Rust ABI8/format-4 simulation, gzip with JavaScript decompression fallback for plain static hosting, browser storage acknowledgment, save download/import, loading progress and fatal-error reload. `build-manifest.json` records every exported file's byte size and SHA-256; total payload includes credits/notices and is not the standalone Rust module measurement from #106.

The native archive command rebuilds Rust std and the simulator together with pinned MVP flags, avoiding Unity Binaryen112's unsupported `bulk-memory-opt` flag. `RUSTC_BOOTSTRAP=1` is scoped to build-std; three known compiler feature warnings remain on this experimental target. Native compilation retains unwind/poisoning. Generated assets/settings are restored byte-for-byte after the Editor exits; do not run a second writer/Editor against this project during the export.

## Verification record (2026-10-28)

- Complete pinned Unity WebGL export succeeded. Total distribution payload: **15,079,452 bytes**, excluding the manifest itself and including notices; see [artifact manifest](build-manifest.json). Hashes identify this export; bit-for-bit reproducibility across machines remains unverified.
- Native Rust: workspace fmt and clippy with warnings denied passed; complete release suite **493 passed**, five preexisting ignored tests unchanged. The original debug suite also passed before the save fixes (489 tests). Current affected save/restore tests passed in debug; the generated ABI header and native C consumer passed.
- Python: **64 tests passed**; docs, Unity static and dependency-direction checks passed.
- Browser boundary Node tests: **4 passed**, including a real Wasm trap, document/window input quarantine, deferred storage success/errors and late chooser/sync callbacks after failure.
- Real Wasm harness: **7/7 passed**, now including irregular HostElapsed operations before format-4 save/load, with Unity's 512 KiB stack and identical loaded state hash.
- Unity 6000.3.26f1 Editor bridge tests: **12/12 passed**, including paced save/load continuation. This targeted suite is not a full graphical Editor test or device performance run.
- Desktop in-app Chromium browser, served at `/world-simulator/` with plain HTTP and no compression-header customizations: real block renders, pause/save succeeds, portable save downloads, page reload + Load restores paused tick **122**, a corrupt import leaves that world intact, valid import succeeds and survives another reload + Load at tick **122**.

Evidence: [loaded after reload](screenshots/reloaded.png), [import acknowledged by browser storage](screenshots/import-saved.png), [corrupt import preserves the live world](screenshots/corrupt-import.png). Incidental FPS figures in screenshots are not sustained-performance acceptance.

## Defects found and repaired

1. Editor compilation: `SliceBridgeTests.cs:217` referenced nonexistent `ConsequenceCount`. Compare complete `Events` arrays instead of dropping the round-trip assertion.
2. [#115](https://github.com/ndabney562/world-simulator/issues/115): canonical restore hashed the final pacing remainder for every historical host call. Reconstruct the clock alone, validate historical outcomes and final clock, preserve failed prefixes/overflow, and keep state/history/format unchanged. The previously rejected downloaded browser save now loads in the native CLI too.
3. [#116](https://github.com/ndabney562/world-simulator/issues/116): rejected commands hashed error checkpoints but omitted them from the existing optional record field. Retain that metadata without changing digest rules or file layout; rejection remains nonterminal. Rejected command -> later commands -> save/load/replay is tested. Existing valid fixtures are retained. Older rejected records lacking checkpoint metadata remain explicitly unreadable; no historical checkpoint is fabricated.
4. Independent review: guard document/window input and late file-picker/storage callbacks after a fatal fault, including reload input without letting the event reach Unity.
5. Browser integration: remove the imported staging file before starting asynchronous syncfs; deleting it during sync caused an FS error despite a successful world load.

## Remaining release evidence

This proves the local desktop Chromium path. Firefox, Safari, a clean external desktop, full-player Rust panic injection, browser storage denial/quota UI, sustained browser performance, phone browsers and cold-start offline caching remain unverified. Boundary unit tests are not claimed as full-player fault-injection evidence. Native iPhone criteria remain open. Independent review and hosted CI apply to the final published head; publication remains a separate decision.

## Release checklist

1. Verify the complete pinned player and desktop browser interaction/storage matrix; record the full manifest, tested commit and screenshots.
2. Independent actual-diff review and latest-head hosted checks for both PRs. Existing checks remain; new Browser boundary contracts job adds coverage but does not claim to build Unity.
3. Retain original-code MIT, dependency texts, NYC attribution, Rust std inventory and the exact Unity Player notice PDF. Confirm developer Unity tier eligibility before publication; see [distribution notices](../../legal/README.md).
4. Noah approves publication/settings for the concrete reviewed artifact. Serve under the repository URL prefix, test compressed resources, and share the verified link only after deployment succeeds.
5. Phone-browser support, offline reload caching, broader desktop browser/device measurements, and native iPhone acceptance remain separately evidenced. This export introduces no service worker and does not promise cold-start offline availability.
