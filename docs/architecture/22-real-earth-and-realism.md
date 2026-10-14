# Real Earth and Realistic Visuals (Revision 2)

Added 2026-10-14 after Noah's decisions:

1. **Real Earth**, using Google Earth / Google Maps data where useful.
2. **Realistic** visuals preferred over stylized.
3. **Single-player** only.
4. **iPhone 15 Pro** minimum.

Decisions 3 and 4 confirm the original proposal. Decisions 1 and 2 reverse assumptions A5 (fictional world) and A6 (stylized look), so they change the stack recommendation, the data pipeline, the coordinate system details, rendering, persistence, and the MVP. This document describes the changes; the affected sections have been updated to point here. ADRs recording all four decisions are in [`docs/adr/`](../adr/).

## R.1 What Google data can and cannot be used for

Google offers the data most relevant here as **Photorealistic 3D Tiles** (the photogrammetry mesh behind Google Earth's 3D view) through the Google Maps Platform Map Tiles API. Its [policies](https://developers.google.com/maps/documentation/tile/policies) (read 2026-10-14; the binding text is the Google Maps Platform agreement, which needs a proper read before launch) say, in summary:

| Allowed | Not allowed |
|---|---|
| Streaming and **displaying** the tiles in our own 3D renderer (Cesium for Unity and Cesium for Unreal are named as supported renderers) | Pre-fetching, storing, or caching tiles beyond the narrow conditions in the terms; **offline use** |
| Placing our own 3D objects over the tiles | Objects "extracted, traced, or otherwise derived by hand or machine" from the tiles: **no deriving building footprints, heights, meshes, or navigation data** |
| Mixing with other data, with clear attribution of which parts are Google's | Image analysis, machine interpretation, or object detection on the tiles |
| | Hiding the Google logo or the aggregated per-tile attribution line |

Other Google Maps Platform data (Places, roads, and so on) has similar no-storage and no-scraping terms.

**Consequence:** Google data can be the **visual backdrop** but **cannot be the simulation's data**. The simulation needs to know where roads, sidewalks, buildings, entrances, land uses, and terrain are, and that knowledge has to come from open datasets we are allowed to process and store.

Other properties of the Google tiles that matter:
- They are a **static, monolithic mesh with baked lighting**. Buildings are not separate objects; parked cars and pedestrians from capture time are baked into the mesh; there is no dynamic time of day in the textures; there are no interiors.
- They **change over time** as Google updates captures, and older versions cannot be pinned. The view of a place can change between sessions independently of our save.
- They require a **network connection** and are **billed per use**, with coverage and quality that vary by city.

## R.2 Open data for the simulation layer

| Need | Source (candidates) | License notes |
|---|---|---|
| Roads, paths, sidewalks, building footprints, building heights and levels (where tagged), land use, points of interest | **OpenStreetMap** (via **Overture Maps** releases or direct OSM extracts) | OSM is ODbL (share-alike on derived databases, attribution on produced works). Overture themes carry ODbL or CDLA-Permissive licences per theme, which needs checking for each one. |
| Terrain elevation | **Copernicus DEM GLO-30** (about 30 m, global), plus national lidar where available | Free licences with attribution. Heights are relative to the geoid (EGM2008), not the ellipsoid. |
| Population density | **GHSL** (Global Human Settlement Layer) or **WorldPop** grids, plus national census tables where available | Open licences with attribution |
| Land cover / biomes | **ESA WorldCover** | CC BY |
| Bathymetry / oceans | GEBCO | Public-domain style terms |
| Planet imagery from orbit (fallback or offline) | NASA Blue Marble and Black Marble (night lights) | NASA imagery is generally public domain |
| Solar system bodies | NASA/JPL ephemerides and planetary imagery | Public domain |

These are processed by our own **offline data pipeline** into compact, versioned **simulation tiles** keyed by the same S2-style cell IDs the core already uses ([07](07-coordinates-and-time.md)). Licensing needs a legal check before launch, especially the ODbL obligations.

## R.3 Revised world-state model: "real skeleton, synthetic life"

The seed + deltas model ([08](08-world-state.md)) survives with one change. The procedural baseline becomes a **data-driven baseline**:

> **World = f(dataset version, seed, generator versions) + canonical deltas + journal**

- **Real:** geography, terrain, road network, building footprints and heights, land use, points of interest, population density, and the actual date, time of day, sun position, and Earth's rotation and orbit.
- **Synthetic (procedurally generated, consistent with the real data):** every person (a **synthetic population** calibrated to real density and census statistics, with no real individuals), households, jobs, schedules, building interiors, building use where OSM is silent, and missing details such as untagged building heights or the layout of minor paths.
- **Pinned dataset versions:** each save records the dataset release it was created with. OSM and Overture change continuously; a save must keep seeing the same streets even after we update the data.

**Ethics and content:** players can drop disasters on real cities and their real neighbourhoods. Residents must stay synthetic and never be modelled on real, identifiable private people. App Store review may examine destructive interventions on real places, so the presentation needs care.

## R.4 Revised rendering approach: hybrid

The pure-procedural renderer in [03](03-rendering-architecture.md)/[06](06-rendering-lod.md) is replaced by a **hybrid** renderer:

| Distance from camera | What is drawn |
|---|---|
| Orbit to ~1 km altitude | Google Photorealistic 3D Tiles, plus our atmosphere, clouds, and night lighting effects layered on top; open-data or NASA imagery fallback when offline |
| City and neighbourhood band | Google tiles for context, plus simulation-driven overlays (traffic flow, crowd density, fires, floods, weather) |
| Street band, and anywhere the world has been changed | **Our own semantic geometry** built from open data (footprints, heights, levels, roof shapes, facades), so buildings can be destroyed, modified, lit dynamically, and entered. Google tiles are **clipped out** (polygon exclusion) where our geometry replaces them. |

Why the near field cannot be Google's mesh alone:
1. Interventions need individual, editable buildings ("burn this house"), and the Google mesh cannot be split into buildings because deriving geometry from it is prohibited.
2. Baked people and cars in the mesh conflict with simulated ones.
3. Baked lighting conflicts with simulated time of day and weather.

**The hardest new visual problem** is the seam between photoreal Google geometry and our generated geometry. Our buildings will look less real than the photogrammetry next to them, and OSM footprints can be misaligned with the Google mesh by a few metres, so NPCs walking on OSM sidewalks may clip into Google-mesh walls. Mitigations to evaluate in Phase 0/1:
- Use our geometry for the whole near-field disc around the camera rather than single buildings, so the seam sits at a distance where detail is lower.
- Generate facades with realistic PBR materials matched per region (brick, render, glass).
- Align OSM data to elevation data and apply small, local, automatic corrections to footprints.
- Fall back to the Google mesh with simulation overlays only at street level wherever our near-field geometry is not ready.

**Realism budget on iPhone (an honest limit):** Google tiles give photorealism from a distance cheaply, because the realism is in the data. At street level, realism has to come from our own assets and renderer, and on a phone that means good PBR lighting, a few dozen realistic animated characters near the camera, and simpler LODs beyond. A street scene of thousands of realistic humans at AAA quality is not achievable on iPhone; crowd LOD ([06](06-rendering-lod.md)) still applies.

## R.5 Revised stack recommendation

The original recommendation (Rust core plus a custom Swift/Metal renderer) assumed a stylized look and procedural content. Real Earth with Google tiles and a realistic look changes the balance:
- Streaming and rendering Google 3D Tiles needs a full 3D Tiles client: tile selection, decoding, caching rules, attribution handling, and clipping. Cesium has built this as **cesium-native** (C++, Apache 2.0), wrapped as **Cesium for Unity** and **Cesium for Unreal**. Google names both as supported renderers, and Cesium for Unity [lists iOS builds](https://cesium.com/learn/cesium-unity/ref-doc/changes.html) and is actively maintained (v1.26.0, 2026-10-01).
- A realistic near field (PBR, lighting, character animation, post-processing) is far cheaper to reach in a mature engine than in a custom Metal renderer.

**New recommendation:** keep the **headless Rust simulation core** unchanged (all of its reasons still hold: determinism, multi-scale LOD, Linux testability), and replace the custom Swift/Metal front end with **Unity (URP) plus Cesium for Unity** on iOS. This is exactly the fallback the original proposal kept open ([02](02-technology-stacks.md) §3, point 5), and it is now the primary path.

| Option | Verdict |
|---|---|
| Rust core + **Unity/URP + Cesium for Unity** | **Recommended.** Mature iOS path, Google tiles support out of the box, realistic rendering tools, and the simulation stays engine-agnostic. The Rust core ships as a native plugin (static library) to Unity. |
| Rust core + Unreal + Cesium for Unreal | Best realism, but the original iPhone concerns (mobile renderer, memory, thermals, app size) remain. Worth a short spike only if Unity's realism falls short. |
| Rust core + custom Swift/Metal + cesium-native | Possible, but we would have to write a 3D Tiles renderer and realistic lighting ourselves. Not justified now. |

Remaining Unity caveats from [02](02-technology-stacks.md) still apply but are contained: Unity's 32-bit transforms are handled by Cesium's georeference (origin shifting) and our camera-relative snapshots, and determinism lives entirely in the Rust core, so Unity's float behaviour never affects simulation results.

## R.6 Revised coordinate details

The frame hierarchy in [07](07-coordinates-and-time.md) stays. These details change for Earth:
- **The body-fixed frame for Earth is ECEF (WGS84)**: a rotating, Earth-centred frame on the **WGS84 ellipsoid**, not a sphere. This matches Google tiles and Cesium, which are ECEF-native.
- **Sector cells** still use the S2-style cube-sphere hierarchy, addressed through latitude and longitude. Each cell's local ENU tangent frame is computed on the ellipsoid.
- **Heights:** Google tiles and Cesium use ellipsoidal heights, while Copernicus DEM uses geoid (EGM2008) heights. The two differ by up to roughly ±100 m, so the pipeline converts everything to ellipsoidal heights, and a geoid model is part of the data.
- **Time:** the epoch and calendar are real (UTC, with a proper conversion to the time scale used by ephemerides), and Earth's rotation, sun position, and seasons are real. Polar motion and similar fine effects are ignored.
- **Solar system:** body positions come from real ephemerides (JPL approximate orbital elements for planets, plus a better lunar model) rather than invented Kepler orbits. The analytic, deterministic evaluation approach stays the same.

## R.7 Revised persistence and connectivity

- **Offline-first no longer fully holds.** Photoreal visuals need a network connection. Proposal: the simulation runs fully offline, and the visuals degrade offline to our open-data geometry plus NASA imagery from orbit.
- **We need a small backend.** The processed open-data simulation tiles for the whole planet are far too large to ship in the app, so they are served as static, versioned files from a CDN (for example in a single-file tile archive format such as PMTiles) and cached on device. Caching our own processed open data is allowed; caching Google tiles is not.
- **Saves record the dataset version** and only store deltas, as before. A save that points at a dataset version must keep working, so old dataset versions stay available on the CDN, or are migrated.
- **Costs:** Google Maps Platform usage fees and CDN costs scale with players. That needs a budget model before any public release.

## R.8 Revised MVP

The MVP from [20](20-mvp.md) keeps its architecture-first scope but moves to a **real neighbourhood**:
- **Noah picks one real neighbourhood**, roughly 6–12 blocks, in a city with good Google 3D coverage and good OSM building data.
- Google Photorealistic 3D Tiles as the backdrop, with camera range from street level to about 1–2 km, and attribution displayed as required.
- Near-field semantic buildings generated from OSM/Overture footprints, with Google tiles clipped out under them.
- A synthetic population of a few hundred residents calibrated to the area's population density, using the same schedules, LOD, materialization, three interventions, inspector, and save/load as before.
- Real date, time of day, and sun position.
- Out of scope, as before: planet-wide data, other cities, and space. The data pipeline processes only the MVP area.

New MVP risk to retire first: **visual seam and alignment** between Google geometry and our buildings and NPCs (R.4).

## R.9 Updated roadmap notes

- **Phase 0 adds:** a Unity + Cesium + Google tiles spike on iPhone 15 Pro (frame time, memory, thermals, clipping); a Rust-core-as-Unity-native-plugin spike; the open-data pipeline for one area; a review of Google and ODbL terms; and a cost model.
- **Phase 3 (city):** data pipeline for a full metro area; traffic on the real road network.
- **Phase 4 (planet):** planet-wide pipeline and CDN; synthetic populations for every settlement from GHSL/WorldPop; offline fallback visuals.
- **Phase 5 (solar system):** real ephemerides and NASA imagery.

## R.10 New risks

| Risk | Impact | Mitigation |
|---|---|---|
| Google terms or pricing change | Photoreal backdrop becomes unavailable or expensive | Keep the open-data fallback renderer working; isolate Google behind a "visual backdrop provider" interface |
| Seam between Google and generated geometry | Breaks the sense of realism at street level | Phase 0/1 spike; near-field disc approach; regional PBR materials |
| OSM data gaps (missing heights, entrances, sidewalks) | NPCs walk through walls; buildings have wrong shapes | Procedural fill-in; per-area quality scoring; MVP area chosen for good data |
| Data scale (planet-wide open data) | Pipeline and hosting become a real engineering project | Stage it: one neighbourhood, then a metro area, then countries; reuse Overture's processed releases |
| Network dependence | Poor experience offline or on weak connections | Offline simulation and fallback visuals |
| Disasters in real places | Reputational and App Store review risk | Tasteful presentation; avoid real identifiable individuals; review policies early |

## R.11 Revision 3 (2026-10-14): Bay Ridge, stack accepted, zero cost

Noah's answers: the MVP area is **Bay Ridge, Brooklyn**; the revised stack (Rust core + Unity + Cesium for Unity) is **accepted**; and the project must be **free**.

### What "free" means for each part

| Item | Cost | Notes |
|---|---|---|
| Rust, Cargo, crates, GitHub repo and Actions for a personal repo | Free | GitHub Actions has free minutes; macOS runners use them faster, so keep macOS CI jobs lean. |
| Unity | Free under Unity Personal | Personal is free below a revenue threshold. Check the current terms before release. |
| Cesium for Unity / cesium-native | Free | Apache 2.0 licence. |
| Open data (OSM/Overture, Copernicus, GHSL, NYC Open Data, NASA) | Free | Attribution required; ODbL share-alike on derived databases. |
| **Google Photorealistic 3D Tiles** | Free only up to a monthly cap | It is an Enterprise SKU with **1,000 free billable events per month**, then about $6 per 1,000 ([pricing](https://developers.google.com/maps/billing-and-pricing/pricing)). The quota counts root tileset requests, and one root request starts a session of up to three hours ([usage and billing](https://developers.google.com/maps/documentation/tile/usage-and-billing)). That is enough for development, but not for a public release with many players. A billing account (with a card) is still needed to get an API key. |
| Apple developer account | **Not free: $99 per year** | Needed for TestFlight and the App Store. Without it, Noah can still install builds on his own iPhone with a free Apple ID, but they expire after 7 days and can't be distributed. |
| Data hosting | Free at MVP scale | Bay Ridge's processed data is small enough to **ship inside the app**, so no CDN is needed for the MVP. A free-tier object store can serve later phases until scale makes that impossible. |

### Consequence: open-data realism is the default, Google is optional

To keep the project free at any number of players, the **primary visual path uses only open data**, and Google tiles become an **optional enhancement** with a hard usage cap and automatic fallback. New York City is one of the best places in the world for this, because the city publishes free, high-quality datasets:
- The **NYC 3-D Building Model** (citywide building massing, GML/multipatch) on [NYC Open Data / Data.gov](https://catalog.data.gov/dataset/3-d-building-model).
- **Building footprints with heights** on NYC Open Data.
- **2017 topobathymetric LiDAR** ([Data.gov](https://catalog.data.gov/dataset/topobathymetric-lidar-data-2017)) for terrain, plus tree and roof shape detail.
- Citywide **orthoimagery** (aerial photos), which gives realistic ground and roof textures.
- **OpenStreetMap** for streets, sidewalks, entrances, shops, and points of interest.
- **US Census and American Community Survey** tables for Bay Ridge's population, households, ages, and occupations, to calibrate the synthetic population.

The NYC Open Data terms of use need a quick check before release. The datasets are published for free public use, but their exact attribution requirements should be confirmed.

Revised rendering plan for the MVP:
1. **Default (free, offline):** realistic PBR buildings generated from the NYC 3D model and footprints, LiDAR terrain, orthoimagery on ground and roofs, and procedurally generated facades matched to Bay Ridge's typical building styles (brick rowhouses, limestone and brownstone, apartment blocks along 4th and 5th Avenues). Everything is editable, so interventions work.
2. **Optional (developer setting at first):** Google Photorealistic 3D Tiles as a distant backdrop, beyond about 1 km (Manhattan skyline, Verrazzano Bridge, Staten Island), with a hard monthly cap below the free tier and automatic fallback to path 1.

This also removes much of the seam risk from R.4 for the MVP, because the near field is entirely our own geometry. The seam only appears at a distance, where detail is low.

### Bay Ridge as the MVP area: notes

- **Bounds proposal:** about 6–12 blocks around 3rd–5th Avenue and roughly 75th–86th Street, to be refined with Noah. This area includes rowhouses, apartment buildings, and the 3rd and 5th Avenue commercial corridors (shops, restaurants), so daily schedules have places to go. The R train runs along 4th Avenue, and stations can act as entry and exit points for commuters leaving the simulated area.
- **Population scale:** Bay Ridge is dense (tens of thousands of residents across the neighbourhood), so even a 6–12 block MVP area may hold a few thousand residents. The MVP materializes only the blocks near the camera; the rest stay at L2/L3. That exercises the LOD system from day one, which is what we want.
- **Data size:** the processed data for this area (footprints, 3D massing, LiDAR-derived terrain, imagery tiles, the OSM graph) is expected to be well under the app-size budget. It will be measured in the Phase 0 pipeline spike.
