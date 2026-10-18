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

## Results

Device: Noah's iPhone 15 Pro, build of `main` at `1da855a` (PR #63), free personal-team signing. Logs: `tp011-50-20261018-194027.csv`, `tp011-500-20261018-195815.csv`, `tp011-5000-20261018-200421.csv` (one row per second). iOS version, room temperature and battery level were not recorded.

| Scale | Duration | Mean fps | 1% low fps | Frame ms (mean / max) | Core step ms (mean / max) | Fill ms (mean / max) | Process MB (start / end) | Thermal |
|---|---|---|---|---|---|---|---|---|
| 50 | 17.3 min | 60.0 | 59.9 | 16.68 / 181 | 0.06 / 2.0 | 0.04 / 0.11 | 232 / 216 | nominal throughout |
| 500 | 6.1 min | 60.0 | 59.9 | 16.67 / 22 | 0.18 / 3.1 | 0.28 / 0.35 | 216 / 217 | nominal throughout |
| 5,000 | 12.2 min | 60.0 | 60.0 | 16.67 / 25 | 1.06 / 8.6 | 2.74 / 3.22 | 220 / 224 | nominal throughout |

Notes on the table:

- The 50-resident log has a 181 ms frame at 240 s and one 21 s stall at about 263 s (a single 20.9 s frame), most likely the app being backgrounded or the phone locked. The 50 row excludes 250 s to 275 s around the stall; the 181 ms frame is kept in. Nothing else in any run exceeds 26 ms.
- None of the runs reached the planned 20 minutes: 50 ran 17 min, 500 ran 6 min, 5,000 ran 12 min. Noah stopped when he had waited as long as he wanted to.
- Fill time includes the wait for the native lock. At 5,000 residents its 99th percentile is 3.17 ms and its maximum 3.22 ms, under the 4 ms threshold for adding a double buffer, so none is added.
- The CSV has no column for the speed multiplier, so the one-minute ×10 check cannot be read from these logs and is unverified.
- Process memory is flat within about 5 MB over each run (the 50 run starts higher while loading); no growth trend.
- Frame time and fps show no trend between the first and second half of any run.

## S-5 recommendation: 30 fps with opportunistic 60, or 60 sustained

Recommend **60 sustained** for the prototype toy world. The decision rule fixed beforehand was: 5,000 residents holding 60 fps with thermal state at most *fair* for the full 20 minutes. The 5,000 run held 60.0 fps (1% low 60.0) with thermal state *nominal* for every sample, but it lasted 12.2 minutes, not 20, so the rule is met for 12 minutes and untested for the last 8. The log is flat (no fps, frame-time or memory drift) and the core step plus fill use about 4 ms of a 16.7 ms frame, so there is large headroom; I judge a late throttle unlikely but it is not proven. The start thermal state and room temperature were not recorded, so a cool start cannot be ruled out.

No ADR is proposed: the evidence is too short to freeze a frame-rate policy, and the toy world renders only boxes and capsules. The decision should be repeated with the Vertical Slice's real rendering load, where thermal behaviour will differ, and the first 20-minute run on that build should be the one that settles it.

## S-6 recommendation: Unity iOS CI

Recommend **local builds on Noah's Mac**, with the static Unity checks and the Rust iOS build staying in free CI. The Rust side is already built for `aarch64-apple-ios` in free macOS CI (TP-003). A Unity iOS build in CI needs a Unity licence activated on a runner (GameCI, see [ci.md](../process/ci.md)), which needs Noah to add secrets and a Unity account, and an Xcode build on top. Signing for a device cannot be done in CI with free personal-team signing at all. In practice the Mac build took Noah one menu click plus Xcode Run, and the two device-only bugs found (stripped instancing variant, stripped `CapsuleCollider`) would not have been caught by CI without a device either. Revisit if a second person needs to build.

## Not verified here

The 20-minute duration was not reached on any run (see Results). The ×10 check is not in the logs. Unity cannot run in the cloud environment, so the device numbers above are Noah's, read from his CSVs; I did not observe the phone.
