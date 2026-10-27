# Browser-core feasibility evidence (2026-10-26)

This isolated experiment uses `ws-sim` from main `e5276e347fc68ab18fad8497055f00e7ea50f343`. It does not join the core workspace, add a runtime dependency, change any ABI/platform, or build a player-facing web app. It answers only whether real slice simulation logic can compile and execute as WebAssembly without a Rust server.

The exact source in `lib.rs` generates the real block's 30 residents at seed71449, applies finite illness, advances100 physical ticks, obtains a bulk view, and verifies paused host time does not advance canonical state. Assertions execute inside WebAssembly. Each exported call starts from the same configuration. Two independent runs produce the same hash in this Wasm runtime; no cross-platform Tier2 hash guarantee is claimed.

## Reproduce

From repository root, with pinned Rust1.97.0, its wasm32-unknown-unknown target and Node.js:

```sh
rustup target add --toolchain 1.97.0 wasm32-unknown-unknown
cargo +1.97.0 build --locked --offline --release --manifest-path docs/verification/browser-core-probe/Cargo.toml --target wasm32-unknown-unknown --target-dir /tmp/worldsim-browser-probe-target
node -e 'const fs=require("fs");const b=fs.readFileSync("/tmp/worldsim-browser-probe-target/wasm32-unknown-unknown/release/worldsim_browser_core_probe.wasm");WebAssembly.instantiate(b,{}).then(({module,instance:{exports:e}})=>console.log(JSON.stringify({bytes:b.length,imports:WebAssembly.Module.imports(module),population:e.probe_population(),tick:String(e.probe_tick()),events:e.probe_events(),hash1:String(e.probe_hash()),hash2:String(e.probe_hash())})))'
```

Offline builds require dependencies already cached. The isolated lockfile pins the existing libm0.2.16; the other dependencies are the unchanged workspace crates. The exact manifest/source above compiled and ran successfully with this output:

```json
{"bytes":290850,"imports":[],"population":30,"tick":"100","events":4,"hash1":"4521798260408798088","hash2":"4521798260408798088"}
```

Module byte size can differ with manifest/build identity. This size is a probe artifact, not the complete game's download size. No existing tests or budgets were weakened.

## Limits

Node's WebAssembly engine executed the module; a browser, Unity Web export, graphics, input, SQLite saves, browser storage durability, mobile memory/performance, offline caching and GitHub Pages deployment were not tested. The installed Unity6000.3.26f1 has Mac Standalone support only, with no Web Build Support. Current SliceHost uses a managed Thread, and NativeWorld/NativeSlice import the native library; those require a reviewed web adaptation. The probe deliberately does not depend on ws-persist or its bundled SQLite, so it proves nothing about browser persistence. Embedded tool-runtime Wasm execution was disallowed by its embedder; the normal local Node executable successfully ran the module.

A web compatibility spike should test the complete client and real simulation before selecting a web port. GitHub Pages serves static assets and requires no paid runtime server for a client-only build. Repository Pages settings and public publication remain separate approval gates. See [the distribution brief](../../process/friend-distribution.md).
