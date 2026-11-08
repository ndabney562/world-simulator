# Project

- **Repo URL:** https://github.com/ndabney562/world-simulator
- **Branch:** One branch per feature (`claude/<issue>-<slug>`), one PR per feature.
- **Merging:** the implementer squash-merges its own PR once latest-head CI is green, the review in [human-approval-gates](docs/process/human-approval-gates.md#standing-delegation-2026-10-29) is clean and no Blocker or Major finding is open (standing delegation 2026-10-29; Noah reconfirmed it in chat 2026-11-05). Noah keeps cost, credentials, licences and data, Apple and device actions, repository settings, irreversible actions, test weakening and stage exit.
- **Test command:** `sh scripts/build-native-macos.sh`, then with Unity closed: `/Applications/Unity/Hub/Editor/6000.3.26f1/Unity.app/Contents/MacOS/Unity -batchmode -projectPath apps/unity -runTests -testPlatform EditMode -testResults "$PWD/TestResults-EditMode.xml" -logFile -`; Rust: `cd core && cargo test --locked`; static: `python3 scripts/check_unity_project.py && python3 scripts/check_docs.py`; scripts and web: `python3 -m unittest discover -s scripts -p 'test_*.py' && node --test tests/web/page.test.cjs` (both also run in CI).
- **Stack:** Rust core (`core/`, ws-* crates, FFI via ws-ffi) + Unity 6000.3.26f1 (URP, UI Toolkit) targeting iPhone 16 Pro and desktop browser; Python check scripts.

AGENTS.md and the GitHub issues remain canonical; this file is the short summary.

## Milestones

- [x] Technical Prototype complete
- [x] Vertical Slice VS-001 to VS-007 merged (#65 to #71)
- [x] Real block rendering and tap-to-inspect merged (#95, #98)
- [x] Illness and persistence merged (#74, #53, #75)
- [x] Desktop browser preview published on GitHub Pages (#112)
- [x] Game HUD, god powers, visible reactions, faith and daily goals merged (GP-001 to GP-004, #124 to #127)
- [x] Toy world deleted from Unity, FFI and crates (#139)
- [ ] Review follow-ups closed (#132, #133, #134, #138)
- [ ] Faster test loop measured (#140)
- [ ] Refreshed desktop preview with the god powers published (#136)
- [ ] On-device iPhone 16 Pro run, and Noah declares the Vertical Slice complete (#77; exit report #145)
- [ ] A playable god simulator with a fun interface (#122)
