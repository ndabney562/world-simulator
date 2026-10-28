# Friend distribution decision brief

Requirement from Noah on 2026-10-26: friends must install and play without building source. Noah also asked whether the simulator can be accessed by a GitHub link. Whether that link should launch browser play or download an installer is pending. This brief does not change the accepted iPhone stage scope, select a new platform or authorize spending/deployment.

## Options

| Route | Friend experience | Current gap / cost |
|---|---|---|
| GitHub Releases | Download a prebuilt app for a supported OS, install and play | No release packages exist yet. Need target builds, third-party notices, clean-device verification and documented installation/security prompts. A source ZIP is not an installer. |
| GitHub Pages | Open a URL and play a client-side WebAssembly/WebGL build | Proposed in [ADR 0035](../adr/0035-browser-delivery-webgl-wasm.md) (#107): client-side Unity WebGL + Rust WebAssembly build running single-threaded with IndexedDB format 4 saves and export/import. Runs on static GitHub Pages with zero operational cost. Complete-client feasibility demonstrated in [#106](https://github.com/ndabney562/world-simulator/issues/106); production release pending reviewed implementation. |
| iPhone TestFlight | Accept an invitation/link and install the beta | Apple Developer membership is normally $99/year, first external build needs review, and beta builds expire after 90 days. Friends need no compiler; developer distribution is not zero cost. |

Sources checked 2026-10-26: [GitHub Pages overview](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages), [Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits), [Unity 6000.3 Web performance](https://docs.unity3d.com/6000.3/Documentation/Manual/webgl-performance.html), [Apple membership](https://developer.apple.com/programs/), [TestFlight](https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/).

## Proposed next decisions

Confirm browser play versus installer and the first supported devices. For a browser target, assess the real Rust simulation, Unity transport/host threading, bundled data, browser save durability/import/export, initial download size and mobile memory/performance before committing to a port. A static mock simulation is not an acceptable substitute. Keep the existing iPhone path until a reviewed scope/architecture decision changes it. Publishing Pages or releases has a separate final approval; no website or binary is published by this brief.

The free default path must remain offline-capable and require no paid runtime service or API. Optional visual providers must not become required. Free-to-play and zero developer distribution cost are different requirements; do not purchase membership or introduce paid hosting without Noah's explicit decision. Browser compatibility, quotas and current platform terms must be checked before release.

## License and notices proposal

The repository currently has no original-code license file. Propose MIT for Noah's original code, subject to his choice and an authorship/dependency review. This brief adds no license and grants no rights to third-party material. A source-code license alone does not produce an installable game. Before a binary/web release, retain required dependency notices and dataset attribution from [the source register](../data/README.md); review bundled assets and Unity runtime distribution terms for the selected target.

No credentials, accounts, signing, platform modules or repository settings are changed by this issue. Device acceptance for #72/#73/#76 remains open.
