# TP-011: iPhone 15 Pro performance spike

Status: **awaiting device data.** The build, scene, HUD and run procedure are done and checked as far as Linux CI can. The numbers in this note come from Noah's iPhone sessions; nothing below is measured on a device yet, and the S-5 and S-6 recommendations stay open until it is.

Issue: [#12](https://github.com/ndabney562/world-simulator/issues/12). Contract: [ADR 0022](../adr/0022-toy-world-render-snapshot-abi-3.md).

## What was built

- `scripts/build-native-ios.sh` builds `ws-ffi` as a static library for `aarch64-apple-ios` into the git-ignored `Assets/Plugins/iOS/`, with iOS-only import settings so the Mac Editor never loads it.
- **WorldSim → Build iOS performance spike** (`Assets/WorldSim/Editor/IosBuild.cs`) sets IL2CPP, ARM64, Metal only, adds the URP Unlit shader to the build, enables Files-app sharing for the run logs, and writes an Xcode project to `apps/unity/Builds/iOS` (git-ignored).
- The Bootstrap scene now draws the toy world: five buildings as boxes, residents as capsules, instanced, no markers.
- A worker thread (`SimHost`) owns pacing and `Advance`. The main thread submits time commands (pause, ×1, ×10 through `ws_submit`, so they are journaled) and decodes one snapshot per frame. No per-entity FFI calls exist.
- Orbit camera: drag to orbit, pinch or wheel to zoom.
- HUD: FPS, frame time (mean and max over 0.5 s), core step time (mean and max per step), snapshot fill time, entity count, managed, Unity and process memory, thermal state, and the log file name. The 50, 500 and 5,000-resident buttons rebuild the world at that scale.
- Every run writes a one-second CSV to the app's documents folder (`tp011-<residents>-<UTC time>.csv`).
- Edit-mode tests cover the ABI-3 decoder with the toy kinds, the shared byte fixture, corrupt-frame rejection, v1 and v2 refusal, and the worker thread.

## Double buffering

Both threads use the one native handle behind a single lock. The main thread's fill therefore waits while the worker is inside `Advance`. The 5,000-resident step costs about 4 ms on the Linux CI runner (see the TP-003 and TP-008 notes), and at ×1 the worker steps ten times a second, so overlap is rare and short. The HUD's fill time includes that wait, so a double buffer is justified only if the device logs show a fill time well above its decode cost (rule of thumb: the 99th percentile fill above 4 ms at 5,000 residents). I have not added a double buffer: it would be optimisation work without data, which the issue excludes.

## Run checklist (Noah's Mac and iPhone 15 Pro)

Free personal-team signing only; nothing here needs a paid account.

1. `git pull` the merged branch, then `scripts/build-native-ios.sh` from the repository root.
2. Open `apps/unity` in Unity 6000.3.26f1 with iOS Build Support. Run **Window → General → Test Runner → EditMode → Run All**. Every test must pass (these are the first runs of the ABI-3 and worker tests; please paste any failure). Then check the **Console** for a warning that the legacy Input Manager (`UnityEngine.Input`, Active Input Handling **Input Manager (Old)**) is deprecated in Unity 6; paste it if there is one. The demo depends on it (#58), so a warning would need a follow-up issue.
3. **WorldSim → Build iOS performance spike**.
4. Open `Builds/iOS/Unity-iPhone.xcodeproj`, choose your personal team under Signing & Capabilities, select your iPhone, and Run. Trust the developer profile on the phone if asked.
5. Per scale (50, 500, 5,000): let the phone cool to *nominal* thermal state, tap the scale, run at ×1 for **20 minutes** with the screen on and the phone not charging, orbiting the camera now and then. The run starts at the full overview, where residents are smaller than a pixel: **pinch in on a building until the orange residents are visible**, and rotate the phone once to check the zoom is kept. Try ×10 for one minute at the end. Note the room temperature and battery level at start and end.
6. Copy the CSVs out: Files app → On My iPhone → the app, or Finder → iPhone → Files.
7. Send the three CSVs and the Xcode Console output for any warning. I will fill in the tables below.

## Results (to fill from the device data)

Device, iOS version and build commit: _pending_.

| Scale | Mean fps | 1% low fps | Frame ms (mean / max) | Core step ms (mean / max) | Fill ms (mean / max) | Process MB (start / end) | Thermal at end |
|---|---|---|---|---|---|---|---|
| 50 | | | | | | | |
| 500 | | | | | | | |
| 5,000 | | | | | | | |

Drift over 20 minutes (frame time, memory): _pending_.

## S-5 recommendation: 30 fps with opportunistic 60, or 60 sustained

_Pending data._ The decision rule, fixed now so the data cannot bend it: if the 5,000-resident run holds 60 fps with thermal state at most *fair* for the full 20 minutes, recommend 60 sustained; otherwise recommend 30 sustained with opportunistic 60. A Proposed ADR follows only if the data supports a recommendation; the HUD and logs cannot distinguish a device that never throttles from one that was cool at the start, so the start state is recorded.

## S-6 recommendation: Unity iOS CI

_Pending, but the facts so far:_ the Rust side is already built for `aarch64-apple-ios` in free macOS CI (TP-003). A Unity iOS build in CI needs a Unity licence activated on a runner (GameCI, see [ci.md](../process/ci.md)), which needs Noah to add secrets and a Unity account, and an Xcode build on top. Signing for a device cannot be done in CI with free personal-team signing at all. The likely recommendation is local builds on Noah's Mac, with the static Unity checks and Rust iOS build staying in CI; the final call waits for the time this procedure takes in practice.

## Not verified here

Unity could not run in the environment that wrote this code: the C# was written against the same APIs as the passing TP-003 and TP-008 code but has not been compiled or run, and nothing has run on a device. The first edit-mode run on the Mac is part of this PR's verification.
