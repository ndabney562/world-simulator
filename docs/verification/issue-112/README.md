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

## Verification record

- Full Python suite: 63 tests passed before the additional build-isolation test; manifest and build-isolation focused tests passed.
- Browser boundary tests: real Wasm trap stops subsequent scheduled game work and UI calls; save success is deferred until `syncfs` completion and storage errors are reported.
- Wasm harness: all seven real simulation/illness/save-load contracts passed with Unity's 512 KiB stack. This is a Node check, not durable browser evidence.
- Native Rust and full-player/browser checks: results recorded below after completion.

The full Editor import exposed an existing compilation failure at `SliceBridgeTests.cs:217`: stale `ConsequenceCount` references. The test now compares the actual complete `Events` arrays, preserving the save/load round-trip assertion rather than dropping it.

## Release checklist

1. Verify the complete pinned player and desktop browser interaction/storage matrix; record the full manifest, tested commit and screenshots.
2. Independent actual-diff review and latest-head hosted checks for both PRs. Existing checks remain; new Browser boundary contracts job adds coverage but does not claim to build Unity.
3. Retain original-code MIT, dependency texts, NYC attribution, Rust std inventory and the exact Unity Player notice PDF. Confirm developer Unity tier eligibility before publication; see [distribution notices](../../legal/README.md).
4. Noah approves publication/settings for the concrete reviewed artifact. Serve under the repository URL prefix, test compressed resources, and share the verified link only after deployment succeeds.
5. Phone-browser support, offline reload caching, broader desktop browser/device measurements, and native iPhone acceptance remain separately evidenced. This export introduces no service worker and does not promise cold-start offline availability.
