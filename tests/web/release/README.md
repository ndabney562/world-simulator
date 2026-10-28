# Full-player desktop release checks (#112)

These local, optional test tools run the actual Unity/Rust export. They are not a
replacement game or a production runtime dependency. Use Node 24+, Playwright
**1.62.1** with its matching Chromium/Firefox/WebKit downloads, Python 3.9+, and
the repository-pinned Rust/Unity tools. Browser engine names are recorded in
results; Playwright WebKit is **not** installed Safari.

## Run against disposable profiles

1. Build the normal player with `scripts/build-desktop-web.sh` and the native save
   validator with `cargo +1.97.0 build --release --locked -p ws-cli --manifest-path core/Cargo.toml`.
   Set `WORLD_SIM_PLAYWRIGHT` to the installed Playwright module directory and
   `WORLD_SIM_NATIVE_CLI` to the absolute `core/target/release/ws-cli` path.
2. Serve `apps/unity/Builds` on a loopback-only HTTP server. Set
   `WORLD_SIM_RELEASE_URL` to its `/DesktopWeb/` URL. Run
   `node tests/web/release/compatibility.cjs chromium /tmp/release-chromium`,
   then repeat for `firefox` and `webkit`. Each run uses a new profile and checks
   genuine IndexedDB writes, browser restart, changed-world import, corrupt
   import, delayed acknowledgment, quota error, write recovery, and denied storage.
3. Visually inspect each quota/denial screenshot. The canvas warning must say
   browser storage failed, not that it was saved. Downloading a native-valid
   backup is intentionally still supported during storage failure; a successful
   download alone does not prove durability. Successful recovered writes must
   survive another browser restart and restore the new state.
4. Build the separate panic player with
   `python3 tests/web/release/build_fault_probe.py`. Do not run another writer or
   Editor against the project during this build. It temporarily changes one Rust
   x100 branch to `panic!`, redirects Editor output, then restores the exact Rust, Editor source and production archive bytes in `finally`. Serve the generated
   `ReleaseFaultProbe` at `http://127.0.0.1:8769/ReleaseFaultProbe/`. Run
   `node tests/web/release/recovery.cjs chromium /tmp/recovery-chromium` and repeat
   for the other engines. Never distribute any `*Probe` directory. The normal
   `DesktopWeb` manifest must still match, with no panic marker in shipping Wasm.
5. Run `node tests/web/release/performance.cjs chromium 300 /tmp/performance-chromium`.
   `WORLD_SIM_METAL=1` explicitly selects ANGLE Metal for Chromium on this Mac;
   `WORLD_SIM_REQUIRE_GPU=1` rejects software renderers. `WORLD_SIM_HEADFUL=1`
   selects a normal window if desired. Run engines sequentially, not against
   competing browser workloads. A run needs `completed: true` and the full
   sampling duration for both x1/x100 phases. Native-validated saves prove the
   selected clock scale and advancing ticks. Mean FPS is sampled Unity FPS;
   RAF percentiles measure presentation scheduling. Wasm/JS heap metrics are
   not OS process memory. No device or all-desktop performance claim follows.

`results.json` and screenshots identify incomplete runs or visual-review needs.
A test process failure, closed window, missing native validator, software renderer,
or wrong speed is not a pass. Saves/profiles/results stay outside version control.

## Installed Safari

Installed Safari has its own manual visual smoke record; Playwright WebKit is not
Safari. Safari 27 automation experiments could execute storage operations but did
not reliably capture/operate the Unity canvas. They are not counted as a Safari
player pass. Experimental drivers are retained privately for diagnosis rather
than shipped as a passing release harness.

Open the normal loopback player in Safari, confirm the block and HUD render,
Pause/Save, reload and Load the same tick. Verify native download/import through
the real file picker separately. Players do not need developer settings.
A separate explicitly labeled fault player can exercise Save -> x100 panic ->
Reload World -> Load without touching the normal player's origin or saves.

## Release limits

These are local browser checks on one Mac. They do not prove another physical
computer, Windows/Linux, low-memory hardware, phone browsers, cold-start offline
caching, or native iPhone acceptance. Public HTTPS/Pages-path checks happen after
separate publication/settings approval and before sharing a verified live link.

