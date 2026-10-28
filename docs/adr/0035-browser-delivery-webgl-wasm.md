# ADR 0035: Browser delivery with Unity WebGL and a pinned Rust Wasm build

- Status: Accepted
- Date: 2026-10-27
- Decided by: Noah (Product Owner), explicitly accepted 2026-10-28; implementation review remains required
- Issue: #107 (design), #106 (spike), #112 (desktop release preparation)
- Supersedes: none; extends ADR 0005, ADR 0018, ADR 0031, and ADR 0036

Noah accepted the browser approach directly on 2026-10-28, recorded on #112. Desktop browsers are the first release target; phone browsers follow later. This does not authorize public deployment or replace native iPhone acceptance.

## Context

The Product Owner requested a zero-barrier distribution route for friends: clicking a GitHub link (e.g. GitHub Pages) and playing the simulator directly in a modern desktop browser without downloading binaries, building source code, paying runtime fees, or requiring Apple Developer signing.

The repository currently targets native execution:
- Authoritative Rust simulation core (`ws-sim`, `ws-world`, `ws-events`, `ws-life`, `ws-persist`).
- Coarse-grained C ABI boundary (`ws-ffi`).
- Unity 6 (URP) front end with C# bridge (`NativeSlice`, `SliceHost`, `SliceDemo`).
- Primary target: iPhone 15 Pro (ADR 0004, ADR 0005) and macOS/Linux developer environments.

The technical prototype and vertical slice prove that Rust owns simulation state while Unity owns presentation. An isolated feasibility probe (`docs/verification/browser-core-probe/`) demonstrated that `ws-sim` compiles to `wasm32-unknown-unknown` and executes identically under WebAssembly. However, delivering the complete interactive game to the browser raises concrete architectural questions:
1. Engine & Runtime: How does Unity WebGL interface with the Rust WebAssembly build?
2. Threading & Pacing: On native platforms, `SliceHost` runs a background thread for pacing and off-thread SQLite saves. Standard GitHub Pages hosting cannot serve custom HTTP headers (`COOP`/`COEP`), precluding `SharedArrayBuffer` and WebAssembly threads.
3. Panic & Poisoning: ADR 0018 specifies native panic-to-poisoned-handle via `catch_unwind`. WebAssembly with `panic = abort` traps fatally.
4. Persistence: How are save format 4 SQLite saves stored durably in the browser?
5. Visual Providers & Costs: How to maintain zero runtime cost and full offline capability without paid external services.

## Decision

### 1. Delivery Architecture: Unity WebGL + Pinned Rust WebAssembly

Retain the authoritative Rust core and Unity URP presentation. Do not replace the simulation with JavaScript or a mock engine:
- Compile `ws-ffi` to WebAssembly (`wasm32-unknown-emscripten`) matching Unity 6000.3.26f1's bundled Emscripten toolchain, exporting the existing coarse C ABI (`ws_slice_*`).
- Pin the toolchain: Rust 1.97.0, MVP-compatible Wasm features, avoiding post-MVP features that break Emscripten optimizer passes.
- Unity WebGL links the Rust WebAssembly module statically as a native plugin or dynamic Emscripten library, maintaining the existing `NativeSliceMethods` P/Invoke declarations.

### 2. Single-Threaded Browser Host & Pacing

Because free static GitHub Pages hosting cannot supply `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` headers, the browser build must run **strictly single-threaded**:
- **Pacing:** In WebGL builds, `SliceHost` does not spawn a worker thread (`#if !UNITY_WEBGL`). Instead, `SliceHostClock.Advance` is driven on the main thread during Unity's `Update()` loop, converting frame elapsed time to microseconds and submitting them to Rust. Rust continues to own simulation cap/drop/remainder and timescale scaling.
- **Save Execution:** In WebGL builds, `ws_slice_save` executes synchronously during a dedicated save step (or split across cooperative micro-steps) rather than spawning a background `std::thread`.

### 3. Panic Policy: Browser Fail-Stop Contract

On native platforms, ADR 0018 defines handle poisoning through `catch_unwind`. In WebAssembly without stack unwinding, unexpected Rust panics trigger a WebAssembly trap (`unreachable`):
- **Fail-Stop Quarantine:** If a fatal WebAssembly trap occurs, the front end immediately stops drawing, suppresses further FFI calls, displays a clean in-canvas error overlay explaining that an unrecoverable simulation fault occurred, and offers a single "Reload World" button.
- **Normal Validation:** Semantic rejections and boundary errors (e.g. invalid commands, out-of-range inputs, unrecognized entity IDs) continue to return typed error status codes (`WS_STATUS_INVALID_ARGUMENT`, `WS_STATUS_STEP_FAILED`) without trapping.

### 4. Persistence in the Browser

Browser saves must preserve the canonical snapshot contract defined in ADR 0025 and ADR 0036 (Save Format 4):
- **Storage:** Persist `.wssave` SQLite databases in the browser using Emscripten's virtual filesystem backed by IndexedDB (`IDBFS`), followed by explicit asynchronous sync (`FS.syncfs`).
- **File Portability:** Provide browser HUD buttons to:
  - **Download Save:** Export the current active `.wssave` file from IndexedDB as a downloadable file to the player's desktop.
  - **Upload Save:** Allow the player to upload a local `.wssave` file into the browser's storage to resume play.
- Canonical state reproduction on load remains identical to native platforms.

### 5. Deployment and Provider Isolation

- **Hosting:** Static deployment on GitHub Pages with gzip/brotli pre-compression.
- **Zero Cost & Offline:** The game runs 100% client-side in the user's browser. Visual providers (Google 3D Tiles, Cesium) remain strictly isolated (ADR 0008, ADR 0026) and are disabled by default in the web build; the real-block massing renders via native mesh geometry.

### 6. Relationship to iPhone 15 Pro Stage Target

Browser delivery is an additive distribution channel for instant sharing. It does **not** replace, weaken, or waive the Stage 2 exit criteria on iPhone 15 Pro:
- Vertical Slice exit criteria (#76) remain evaluated on device against iOS performance and memory budgets.
- Desktop browser performance data is reported separately and not substituted for mobile verification.

## Consequences

### Positive
- Friends can play the real simulation instantly via a standard web link without setup, downloads, or Apple developer accounts.
- Zero hosting and runtime operational costs.
- Exactly the same authoritative Rust simulation logic and state hashes as native builds.
- Save files (`.wssave`) are fully compatible and interchangeable between native desktop, mobile, and browser.

### Negative / Trade-offs
- WebGL memory limits: browser tabs typically cap WebAssembly memory to 1–2 GB.
- Single-threaded execution means heavy save serialization runs on the main thread, requiring optimization or small save payloads.
- Fatal Wasm traps require full canvas reload rather than handle-level poisoning.
