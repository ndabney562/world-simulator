# Unity front end (`apps/unity/`)

The Unity 6 / URP application shell. Unity owns rendering, presentation, input, and platform integration; it never owns simulation state, which stays in the Rust core (`core/`). See [03-rendering-architecture.md](../../docs/architecture/03-rendering-architecture.md) §4.1.

This is the TP-002 skeleton: folders, assemblies, one scene, iOS player settings, and a mobile URP asset. It has no simulation integration (TP-003), no providers (TP-012), and no UI.

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
| `Assets/WorldSim/Scenes/Bootstrap.unity` | | The only scene: a camera and a directional light. |

## Settings

| Setting | Value | Why |
|---|---|---|
| Render pipeline | `Rendering/Pipeline/UrpMobile.asset` (URP, Forward renderer) | ADR 0005 |
| URP defaults | HDR off, MSAA off, render scale 1, one shadow cascade at 50 m (1024 px, hard shadows), no additional-light shadows, no depth or opaque texture, SRP Batcher on | Conservative mobile defaults that keep 30 fps on an iPhone 15 Pro possible. Tuning on device is TP-011. |
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
2. Open it with 6000.3.26f1. The first import creates `Library/` and fills in the rest of `ProjectSettings/` and `Packages/packages-lock.json`; those regenerated files are committed (only `Library/`, `Temp/`, `Logs/`, `UserSettings/`, `obj/`, and builds are ignored).
3. Open `Assets/WorldSim/Scenes/Bootstrap.unity`.

## Running the tests

In the editor: **Window → General → Test Runner → EditMode → Run All**.

From the command line on macOS (the editor must be closed):

```sh
/Applications/Unity/Hub/Editor/6000.3.26f1/Unity.app/Contents/MacOS/Unity \
  -batchmode -projectPath apps/unity \
  -runTests -testPlatform EditMode \
  -testResults "$PWD/TestResults-EditMode.xml" -logFile -
```

The exit code is 0 when every test passes. `ProjectSkeletonTests` checks the assembly layout, the iOS player settings, the URP asset, text serialization, and the Bootstrap scene, so a drift from the settings above fails a test.

The static checks that run in CI without the editor:

```sh
python3 scripts/check_unity_project.py
```

How Unity tests relate to CI is in [ci.md](../../docs/process/ci.md) (S-6).

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
