# Visual Backdrop Providers

Revision 4 addition (ChatGPT review item 1). Related: [03](03-rendering-architecture.md), [06](06-rendering-lod.md), [22](22-real-earth-and-realism.md), ADR 0008.

## 23.1 Principle

**Visual providers are interchangeable display sources. They are never part of the world simulation.**

- The simulation core never reads from a visual provider, never depends on whether one is available, and never stores anything derived from one.
- A provider never writes simulation state.
- Swapping, adding, or removing a provider (open data, Google, NASA, or something in the future) changes what the player sees, but not one bit of simulation state, saves, events, or test results.

This is what lets Google Photorealistic 3D Tiles be an **optional backdrop**: the game is fully functional, and the simulation is identical, with it switched off.

## 23.2 Two kinds of external data, kept apart

| | Simulation data sources | Visual backdrop providers |
|---|---|---|
| Examples | NYC 3-D Building Model, NYC footprints, NYC LiDAR, OSM/Overture, Copernicus DEM, Census/ACS, GHSL, JPL ephemerides | Open-data visual tiles (our own), Google Photorealistic 3D Tiles, NASA Blue Marble / Black Marble imagery, future sources |
| When used | Offline, by the data pipeline ([11](11-procedural-generation.md)) | At runtime, by the Unity front end |
| Stored? | Yes, processed into versioned simulation tiles that saves pin | Only as permitted by each provider's terms (Google: no persistent caching) |
| Affects simulation? | Yes, it is the world's baseline | **Never** |
| Licence must allow | Processing, storing, deriving | Display |

An open dataset can feed both sides. For example, the NYC 3D model defines buildings for the simulation **and** is the source of the default visual geometry. Google data can only ever be on the visual side.

## 23.3 The provider contract

Each provider is a front-end (Unity C#) component implementing one conceptual interface. It is described here as a contract, not as code:

| Aspect | What a provider declares or does |
|---|---|
| **Identity and kind** | Name, version, and kind: 3D mesh tiles, imagery draped on terrain, terrain elevation for display, sky/space imagery |
| **Coverage** | Answers "do you have content for this region at this level of detail?", which may be asynchronous and network-dependent |
| **Scale range** | The camera altitude or screen-space error range in which it is appropriate (for example Google tiles beyond 1 km from the camera, NASA imagery from orbit) |
| **Availability** | Online or offline, current network state, and whether the quota or budget is exhausted |
| **Cost and quota** | Its usage model (free; metered with a monthly cap), its current usage, and a hard cap enforced locally (ADR 0007) |
| **Attribution** | Attribution strings and logos it requires, with placement rules; fed to a single attribution manager that renders them as the provider's terms require |
| **Exclusion support** | Whether it can hide content inside polygons supplied by the core (needed wherever simulation geometry replaces the backdrop) |
| **Lighting and appearance hints** | Whether its content has baked lighting, so the front end can adjust its blending, fog, or time-of-day treatment |
| **Lifecycle** | Initialise, update with the camera each frame, release resources under memory pressure, shut down |

## 23.4 How the core and providers meet

The core's render snapshot ([03](03-rendering-architecture.md)) contains, among other things:
- **Simulation geometry**: buildings, terrain patches, people, vehicles, and effects that the simulation owns, which are editable and interactive.
- **Exclusion zones**: polygons (with height ranges) where simulation geometry replaces any backdrop. These are always generated around the near-field disc and around anything the simulation has changed (a burned building, a new park).
- **Overlays**: simulation-driven visuals that must sit on top of any backdrop (fire, smoke, flood water, weather, traffic and crowd density, selection highlights).

The front end's **provider manager** then:
1. Picks providers per scale band, from a configured priority list filtered by coverage, availability, and quota.
2. Applies exclusion zones to every provider that supports them. A provider that cannot exclude is not allowed in bands where exclusion is needed.
3. Draws simulation geometry and overlays on top.
4. Shows the combined attribution.

**Picking (taps) never resolves against provider geometry.** A tap gives the core a ray; the core resolves it against its own data (building footprints, entities). Tapping a building in the Google mesh selects the simulation's building at that location, or nothing, if the simulation has no building there.

## 23.5 Initial providers

| Provider | Role | Default? | Notes |
|---|---|---|---|
| **Open-data provider** | Realistic geometry and textures built from the NYC 3D model, footprints, LiDAR terrain, and orthoimagery (for Bay Ridge); OSM/Overture and Copernicus elsewhere later | **Yes** | Free, offline, ships with the app for the MVP area. Also supplies the near-field simulation geometry's visual style. |
| **Google Photorealistic 3D Tiles** | Photoreal distant backdrop (skyline, harbour, Verrazzano Bridge) | No, opt-in | Streamed through Cesium for Unity; hard monthly cap below the free allowance; automatic fallback; required attribution; never cached beyond its terms. |
| **NASA imagery** | Earth from orbit (day and night lights), other planets and moons | Yes, at planet and system scale | Public domain; ships at modest resolution. |
| **Procedural fill** | Anything with no data (distant regions before the planet pipeline exists) | Fallback | Plain terrain and generic cityscape from coarse data. |

## 23.6 Fallback and consistency

- Fallback is automatic and per band: if Google becomes unavailable (network, quota, terms change), the open-data provider takes over without any change to simulation or saves.
- A **uniform lighting and atmosphere layer** is applied on top of all providers so that switching provider changes detail, not the time of day or the weather.
- Visual differences between providers (Google's baked lighting versus our dynamic lighting) are accepted. The provider is a cosmetic choice for the player, exposed as a setting ("photoreal backdrop: on/off").

## 23.7 Testing

- Simulation tests never instantiate a provider.
- A front-end test asserts that switching providers mid-session leaves the core's state hash unchanged.
- Provider quota enforcement and fallback are tested with a fake provider that simulates network loss and quota exhaustion.

## Pending TP-012 implementation contract

[ADR 0026](../adr/0026-provider-spike-read-only-frame-query.md), tracked by [#13](https://github.com/ndabney562/world-simulator/issues/13), proposes a read-only native hash/frame query, isolated provider assembly, exact Cesium registry/package pin and explicit replacements for the pre-provider test prohibitions. It is **Proposed**; ABI 3 and all existing checks remain active until the Product Owner accepts the design and named validation changes.
