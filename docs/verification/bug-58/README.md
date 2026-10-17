# Unity rendering/input regression verification (#58)

Unity 6000.3.26f1, Apple Silicon macOS Editor, graphics enabled, isolated checkout of main `4d43b729b70bacd432c5ed7e429f6bcf5a036067` plus this PR's fix. No changes to Noah's local project, saved settings, or running Editor were made.

## Reproduction and cause

- Unmodified main: 28/30 Editor tests passed. The demo stopped on a legacy `UnityEngine.Input` call because the serialized input backend was the unset value `-1`; the geometry assertion also failed by exactly `0.5`.
- With only `activeInputHandler: 0` selected: still 28/30 passed, but the bootstrap failure moved to **zero cyan pixels**. The simulation now advanced successfully.
- The overview distance was about 1.5 km while the camera's far plane stayed at 1 km. The fixed camera fits every corner of the building bounds using the current perspective/aspect and updates its far plane with viewing distance and scene radius.
- The geometry assertion compared a scaled forward vector with a unit vector. The resident capsule intentionally has `(0.5, 0.85, 0.5)` scale. Noah explicitly approved correcting this assertion on 2026-10-17. The new assertions independently check normalized heading and all three scale components, at the original tolerances. No rendering threshold was reduced, no tests were removed or skipped.

## Results

The full Editor run on the final source passed **33/33**, zero failed/skipped, at 2026-10-17 18:49 UTC. The [result XML](unity-results.xml) redacts local checkout paths. The original bootstrap test still requires tick advancement and more than 100 cyan pixels. The [captured image](bootstrap.png) was visually inspected: all five buildings are visible.

New regression coverage:

- `DemoInputBackendSupportsLegacyMouseAndTouch` checks the effective compile-time backend and mouse/touch API access.
- `OverviewFitsEveryBuildingAndKeepsZoomOutInsideClipRange`, at landscape 16:9 and portrait 9:16, checks all eight corners of all five native building transforms, including a 2× zoom-out. Reinstating the old 1,000 m far plane makes **both** cases fail: native building depths 1,186.32 m and 1,834.49 m exceed it. Restoring the fix returns the full suite to 33/33.
- The Python checker rejects unset, Input System-only, Both, and missing-value settings; canonical Input Manager-only is supported without adding a package.

Commands (from repository root):

```sh
scripts/build-native-macos.sh
/Applications/Unity/Hub/Editor/6000.3.26f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -projectPath apps/unity -runTests -testPlatform EditMode \
  -testResults /tmp/worldsim58-final.xml -logFile /tmp/worldsim58-final.log
python3 scripts/check_docs.py
python3 scripts/check_unity_project.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The PR handoff records the branch/head SHA and remaining workspace/hosted checks. No iPhone touch-gesture or thermal/performance result is claimed. Buildings are still actual metre-scale boxes across a roughly kilometre-wide dataset; residents remain small at overview distance. Unity's Game-view Scale control only magnifies its preview; wheel/pinch changes the actual camera distance.
