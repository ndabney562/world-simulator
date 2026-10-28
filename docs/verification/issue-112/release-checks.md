# Remaining desktop release verification (#112)

Runtime candidate: merged `8de9a2cb393383a61d2e66ca3ce4d1441a2de424`, tree
`c02ea2cb27d9b4496bf31b96f80b230e81c9db17`. The full player remains
**15,080,019 bytes** excluding its manifest; all nine file digests still match.
These checks add test tools and evidence; they do not add a shipping panic hook.

## Fresh-profile storage and compatibility

Playwright 1.62.1 engines, 1280 × 900 CSS viewport, separate disposable persistent
profiles, same-origin plain static hosting under a URL prefix:

| Engine | Browser restart restored | Changed live world before import | Imported original restored | Recovered storage survived restart |
|---|---|---|---|---|
| Chromium 151.0.7922.34 | tick36 / `483a79c943c378cc` | tick69 / `97b9dd350ce7c812` | tick36 / `483a79c943c378cc` | tick69 / `97b9dd350ce7c812` |
| Firefox 153.0 | tick32 / `acc47fcf38ec95ab` | tick64 / `8c90eea58a032680` | tick32 / `acc47fcf38ec95ab` | tick64 / `8c90eea58a032680` |
| Playwright WebKit 26.5 | tick26 / `080948ac001e9a0d` | tick58 / `a64ad706b91f6f2c` | tick26 / `080948ac001e9a0d` | tick58 / `a64ad706b91f6f2c` |

The real native loader validates each downloaded SQLite save before its state is
compared. A corrupt import preserves the current state; a valid import restores a
different prior state and survives another browser restart. Journal hashes may
change while paused because host operations continue to be recorded; comparisons
use authoritative tick/state, not byte equality of growing journals.

Delayed genuine IndexedDB transaction completion delays the save download by at
least three seconds. Simulated `QuotaExceededError` reaches real IndexedDB writes;
the game displays **Browser save failed; download a backup**, remains playable,
and exports a native-valid backup. Restoring writes, saving the new state and
restarting the browser proves recovered durability. A fresh profile denying
`indexedDB.open` with `SecurityError` also displays a storage-failure warning and
exports a valid backup. Canvas warnings are visually inspected, not inferred from
an HTML Ready label or from download success alone. Profiles and screenshots are
retained locally; no existing user browser profile is changed.

## Actual Rust panic and full-player recovery

A separate local `ReleaseFaultProbe` player replaces exactly the Rust x100 branch
with a marked `panic!` and redirects the Unity output directory. The builder
restores both source files and the production native archive after the build.
The panic marker is present in the probe Wasm and absent in shipping Wasm; all
shipping manifest hashes remain unchanged. Never distribute probe directories.

Chromium, Firefox and WebKit each observed the actual Rust panic marker and Wasm
abort through the full Unity player. The fail-stop overlay appears, canvas and
save/import toolbar are disabled, scheduled callbacks stop, and keyboard/document
input does not reach downstream listeners. Reload World then Load restores the
same durable native tick/state saved before the panic; the retained per-engine results record the exact paired
native ticks and state hashes.
This is full-player fault evidence, beyond the existing boundary unit tests.

## Performance and installed Safari

The three sequential engine measurements completed successfully. These are
**hardware-accelerated headless measurements**, not physical-display or all-device
acceptance. Native-validated saves prove selected scales and advancing ticks.

| Engine / GPU API | Sampling per phase | Mean sampled FPS x1 / x100 | Lowest sampled FPS x100 | RAF p95 / p99 at x100 |
|---|---|---|---|---|
| Chromium151 / Metal | 300 seconds,60 samples | 119.54 /119.65 | 107.56 | 9.2 /9.3 ms |
| Firefox153 / reported Apple GPU | 120 seconds,24 samples | 99.51 /81.33 | 35.00 | 24.98 /59.32 ms |
| Playwright WebKit26.5 / Apple GPU | 120 seconds,24 samples | 58.55 /58.94 | 52.97 | 20 /25 ms |

These results do not establish sustained60FPS across browsers. FPS is sampled
Unity FPS; RAF percentiles describe browser presentation scheduling. The configured
100x clock is not a guarantee of exact wall-clock100x throughput under load.
Wasm allocation remained128MiB; OS process RSS, thermal behavior and long-session
leak freedom were not measured. Browser-specific JS heap reporting is incomplete.
The first SwiftShader run and a headful window that closed early are excluded.
Final raw results and screenshots are retained locally; current readiness and
additional Safari/manual evidence are tracked on [issue112](https://github.com/ndabney562/world-simulator/issues/112).

Actual installed Safari has not passed the visual player check yet. WebDriver
could not connect; the first-party MCP interface ran storage operations but its
canvas control/capture results were inconsistent. Those experiments are not
counted as a Safari player pass. A direct visual Safari smoke check is pending;
no further preference change is requested. Playwright WebKit is not Safari.

## Publication decision and limits

The test commands, isolation rules and evidence requirements are in
[the harness guide](../../../tests/web/release/README.md). Public deployment remains
unapproved. Before a publish request, complete the actual-Safari
check, independent final review and latest-head CI, and present the concrete
artifact plus [hosting/settings action](publication-proposal.md). Verify HTTPS, compression and the real
repository URL prefix after approved deployment before sharing a working link.

This evidence is from one Apple Silicon Mac and isolated browser profiles.
Another physical desktop, Windows/Linux, Intel/low-memory graphics, phone browsers,
offline cold-start caching and native iPhone acceptance remain unverified. A
limited desktop preview must state those limits; this is not all-device support
or a stage-exit declaration. No paid service, new account/secret or device action
was introduced. Keep #112 open until its remaining release gates are resolved.
