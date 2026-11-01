# Unity front end (`apps/unity/`)

The Unity 6 / URP application shell. Unity owns rendering, presentation, input, and platform integration; it never owns simulation state, which stays in the Rust core (`core/`). See [03-rendering-architecture.md](../../docs/architecture/03-rendering-architecture.md) §4.1.

The RealBlock scene is the game: the real block and its residents drawn from the Rust core, tap inspection, save/load and the god-powers HUD (sections below). Current ABI: **10**. Build it for iPhone with **WorldSim → Build iOS real block** (below); device results are in [the Vertical Slice exit report](../../docs/spikes/vertical-slice-exit.md). Visual providers are not implemented.

## Editor version

**Unity 6000.3.26f1** (Unity 6.3 LTS), recorded in `ProjectSettings/ProjectVersion.txt`. Unity 6.3 is the current LTS; Unity 6.0 LTS reaches end of support in October 2026. Install it through Unity Hub with the **iOS Build Support** module. The Personal licence is enough (ADR 0007).

Changing the editor version is a deliberate change: update `ProjectVersion.txt` and this section together (`scripts/check_unity_project.py` checks that they agree).

## Layout

| Path | Assembly | Purpose |
|---|---|---|
| `Assets/WorldSim/Bridge/` | `WorldSim.Bridge` | C# bindings over `ws-ffi` and snapshot decoding (TP-003). References no other WorldSim assembly. |
| `Assets/WorldSim/Rendering/` | `WorldSim.Rendering` (references Bridge) | Simulation-geometry layer, agents, overlays. `Rendering/Pipeline/` holds the URP asset and renderer. |
| `Assets/WorldSim/Providers/` | none yet | Visual backdrop providers; the only place provider code may live ([17-repository-structure.md](../../docs/architecture/17-repository-structure.md) §18.2 rule 4). Empty until TP-012. |
| `Assets/WorldSim/UI/` | none yet | UI Toolkit views. Empty placeholder. |
| `Assets/WorldSim/Tests/EditMode/` | `WorldSim.Tests.EditMode` (Editor only) | Edit-mode tests. |
| `Assets/WorldSim/Scenes/RealBlock.unity` | | The game and the only build scene: the real block, its residents and the HUD (`SliceDemo`). |

## Settings

| Setting | Value | Why |
|---|---|---|
| Render pipeline | `Rendering/Pipeline/UrpMobile.asset` (URP, Forward renderer) | ADR 0005 |
| URP defaults | HDR off, MSAA off, render scale 1, one shadow cascade at 50 m (1024 px, hard shadows), no additional-light shadows, no depth or opaque texture, SRP Batcher on | Conservative mobile defaults that keep 30 fps on an iPhone 16 Pro possible. Tuning on device is TP-011. |
| Colour space | Linear | Physically based lighting (§4.4) |
| iOS scripting backend | IL2CPP | The only backend iOS supports |
| iOS architecture | arm64 | The only architecture Unity 6 builds for iOS |
| iOS graphics API | Metal only | Checked through `PlayerSettings.GetGraphicsAPIs`; automatic selection is acceptable when the resulting list is Metal only. |
| iOS target device | iPhone only | ADR 0004 |
| iOS minimum version | 26.0 | ADR 0004: the current (iOS 27) and previous major version. Raise it each year when a new major iOS ships (the edit-mode test holds the value). |
| iOS SDK | Device SDK | |
| Bundle identifier | `org.worldsimulator.placeholder` | Placeholder. The real identifier is chosen with signing in TP-011, which is a Product Owner gate. |
| Asset serialization | Force Text, Visible Meta Files | Reviewable diffs; Unity YAML merge |

## Opening the project

1. In Unity Hub choose **Add → Add project from disk** and select `apps/unity/`.
2. Run `scripts/build-native-macos.sh` from the repository root first (the pinned Rust toolchain must be installed), then open it with 6000.3.26f1. The first import creates `Library/` and fills in the rest of `ProjectSettings/` and `Packages/packages-lock.json`; those regenerated files are committed (only `Library/`, `Temp/`, `Logs/`, `UserSettings/`, `obj/`, and builds are ignored).
3. Open `Assets/WorldSim/Scenes/RealBlock.unity`.

## Running the tests

In the editor: **Window → General → Test Runner → EditMode → Run All**.

From the command line on macOS (the editor must be closed):

```sh
/Applications/Unity/Hub/Editor/6000.3.26f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -projectPath apps/unity \
  -runTests -testPlatform EditMode \
  -testResults "$PWD/TestResults-EditMode.xml" -logFile -
```

The exit code is 0 when every test passes. `ProjectSkeletonTests` checks the assembly layout, the iOS player settings, the URP asset, text serialization, and the build scene, so a drift from the settings above fails a test.

The static checks that run in CI without the editor:

```sh
python3 scripts/check_unity_project.py
```

How Unity tests relate to CI is in [ci.md](../../docs/process/ci.md) (S-6).

## Input

The game uses Unity's built-in Input Manager (`UnityEngine.Input`) for mouse/touch orbit, pan, pinch/wheel zoom, and HUD pointer coordinates. **Player → Other Settings → Active Input Handling** is explicitly **Input Manager (Old)** (`activeInputHandler: 0`); no Input System package or Both-mode requirement is introduced. Static CI rejects an unset or incompatible backend, and the Editor suite checks the effective legacy input define and API calls. Restart the Editor after changing this setting.

## Merging scene and asset files

`.gitattributes` routes `.unity`, `.prefab`, `.asset`, and `.meta` files through Unity's Smart Merge. To enable it locally, add this to your git config (path for a Hub install on macOS):

```ini
[merge "unityyamlmerge"]
    name = Unity SmartMerge
    driver = '/Applications/Unity/Hub/Editor/6000.3.26f1/Unity.app/Contents/Tools/UnityYAMLMerge' merge -p %O %B %A %A
    recursive = binary
```

No Git LFS is used (ADR 0007); there are no large binaries yet.

## iOS graphics test troubleshooting

Unity 6000.3.26f1 can report automatic iOS graphics selection while the API list remains Metal only. Its Player inspector hides graphics API selection when fewer than two APIs are available. Do not look for a missing Auto Graphics API checkbox or change platforms to fix this. The EditMode test verifies the actual iOS API list is exactly Metal; the automatic-selection flag does not determine whether this requirement is met.

The original skeleton test also required the automatic-selection flag to be false, causing a fresh import to fail despite selecting Metal. That assertion was corrected in the fix for issue #37; no other Player requirement was relaxed. If the API-list assertion fails, record the reported list and Editor version on the bug issue.

The static CI checker also requires the serialized `iOSSupport` API list to contain exactly Metal (`m_APIs: 10000000`, the little-endian encoding of graphics API value 16). It deliberately does not constrain `m_Automatic`; both 0 and 1 pass when Metal is the only API. This catches API-list drift without needing the Unity Editor in CI. The Editor test still verifies the effective imported API list.

## Native plugin

Close any Editor using this checkout and run `scripts/build-native-macos.sh` from the repository root before opening the project. Rebuilding a loaded dylib requires restarting Unity. A missing library or ABI mismatch fails with a Console error rather than falling back to managed simulation. The macOS plugin matches the running machine's architecture; a Rosetta Editor needs a matching x86-64 build. Generated dylibs and their metadata are ignored. Run the full EditMode suite with graphics enabled; `-nographics` cannot verify rendering.

The toy world and its Bootstrap scene were removed in #139; the slice is the only native path.

## VS-007 calendar and sun

`SliceDemo` takes an explicit UTC start in the Inspector. `NativeWorld.UtcToSim` delegates civil validation and epoch/leap conversion to Rust; the integer start config is never inferred from Unity frame time. The HUD presents paired core UTC/New York labels, offset and committed speed for pause/1×/10×/100×.

One serialized slice fill plus calendar query publishes a matching tick, with caller-owned buffers and value structs. `CalendarPresentation` caches labels by core UTC second/committed scale and directs the existing directional light opposite the core toward-sun vector (ENU→Unity axes only). Lit materials respond to it; at night/unavailable dates direct sunlight is off, with fixed ambient presentation illumination. No renderer solar model or second clock exists. `CalendarTests` cover layout, leap conversion, pairing, HUD/light, pause and allocation. [Verification and device limitations](../../docs/verification/issue-71/README.md).


## Real block (VS-008 / current ABI10)

Open `Assets/WorldSim/Scenes/RealBlock.unity` and press Play after rebuilding/restarting the native plugin. This scene uses the separate `NativeSlice`/`SliceHost` path:58 exact source footprint extrusions, four featured simulation identities and the seed-generated10–30 residents. Default seed71449 has30people and starts at explicit UTC2026-06-22T11:22:30Z (Monday07:22:30 New York), never from a renderer clock. The complete context stays source-only massing. Coloured footprints identify the four featured buildings; orange capsules are actual local fine residents. Blue bars show actual current occupants only for coarse buildings.

Drag to orbit; right/middle mouse or two-finger drag pans; wheel/pinch zooms. **Overview** fits all58 source bounds and resets the camera-interest latch; **Street** first requests nearby detail, then focuses an actually published nearby perimeter walker when one is available. Camera shortcuts preserve the selected speed/pause; resume1× or10× when nearby fine detail needs a physical policy tick. Indoor/off-map/coarse people are not invented at street positions. Portrait/wide framing uses the camera's actual viewport; the HUD has reserved space so it cannot cover the default massing. Pause/1×/10×/100×, UTC/New York labels and light consume committed core output. Above10× the existing core L2 cap remains active.

The per-frame native/managed pair validates tick/time/hash/scale/dataset before publication, reuses bounded buffers/matrices and submits instanced people/occupancy bars. Geometry uploads once into a shared five-submesh cache. The warmed fill/decode/render path allocates no managed bytes; changing date labels and the periodic HUD/CSV naturally format text. A worker sends elapsed integer host microseconds to the core, which owns cap/drop/remainder. There is no second simulation-step accumulator in this host.

CSV files `vs008-real-<seed>-<capture-time>.csv` in the app documents folder include backend, seed, population, mesh counts, committed scale/tick, frame/fill/native-apply timing, count partitions and memory/thermal fields. Process memory is blank/unavailable on desktop, not reported as a measured zero. Editor GPU-fenced full960×540 measurements and native/managed/source evidence are recorded in [issue-72 verification](../../docs/verification/issue-72/README.md); they do not establish iPhone performance.

**WorldSim → Build iOS real block** explicitly builds this scene to `Builds/iOS-RealBlock`, includes URP Lit/Unlit and instancing variants, and retains the committed minimum iOS version. It is the only build scene. The new scene's Editor tests use Unity's asset-path play-mode loader. Install the pinned Editor's iOS Build Support, run `scripts/build-native-ios.sh`, then use the menu to export an unsigned Xcode project. Signing, installation and actual iPhone screenshots/CSV remain the separately authorized device handoff. The current local Editor has only Mac Standalone Support, so no Unity iOS export/device result is claimed.

## Tap inspection (VS-009 / ABI10)

In RealBlock, tap a visible resident or featured building to open a read-only timestamped inspector. Re-tap to refresh; Close inspection dismisses it. Drag/two-finger/pinch camera controls remain. A gesture that ever moves more than8logical points, lasts over0.35seconds, enters the HUD or gains another finger cannot become a tap. Context footprints occlude picking but never acquire simulated identities/population. The panel is a sample; no inspection query or text formatting runs each frame. Core ownership, shared handle gate and sky coherence remain. Paired sky supplies the sample’s civil timestamp without reconstructing it in presentation. Current native preflight requires ABI10; restart Editor after rebuilding. Actual iPhone screenshot/core-dump acceptance remains required.

Resident inspection follows the Product Owner’s five-field preference: age, sex, employment, health and alive/dead. ABI10 (inspection schema 3) supplies all values from core identity and the retained vitals column. Sex is generated independently of names. Initial residents have health100/100 and Alive status; disease/death dynamics are separate work. Strict decoding rejects invalid sex/range/life tags or contradictory health/life pairs before replacing the panel. Re-tap refreshes the held sample; building inspection is unchanged.

## VS-010 illness action

The selected person's ordinary panel remains age, sex, employment, health and alive/dead. **Curse with illness** applies severity5 for two model hours and opens a separate consequence readout. **Refresh consequences** reads the same root without applying another command; tap the person again to return to the five fields. Native apply, inspection, consequence query and sky are serialized into one coherent managed publication. No per-frame selection polling is added. Invalid samples retain the previous display with an error. Direct versus contributing confidence is shown explicitly.

## God powers (GP-002 / ADR 0038)

The RealBlock HUD offers four powers: Curse (illness), Bless, Storm and Gathering. Bless and Gathering are select-then-tap: choose the power, then tap a resident (Bless) or a featured residential building (Gathering). Storm casts at once for one hour. A Gathering from the HUD lasts 30 minutes. The managed bridge issues slice operation tags 6 to 8 and reads each new root with `ws_slice_last_root`, because a storm root has no actor to inspect; the feed lists the powers' consequences from the core's per-root consequence read. Inspection schema 3 now shows override kind 3 (gathering), the only kind valid away from home. The HUD reads, and never owns, power state.
