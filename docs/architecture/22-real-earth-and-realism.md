# Real Earth: Data Sources, Licensing, Realism, and Cost

Covers the consequences of ADRs 0001 (real Earth), 0002 (realistic visuals), 0006 (Bay Ridge), and 0007 (zero cost). How visual sources plug in is in [23](23-visual-providers.md); how data becomes the world's baseline is in [11](11-procedural-generation.md).

## R.1 Two rules

1. **The simulation runs on open data we are allowed to process and store.** Real geography (terrain, streets, buildings, land use, population statistics) comes from open datasets, processed by our pipeline into versioned simulation tiles.
2. **Google data is display-only and optional.** Google Photorealistic 3D Tiles may be used as a visual backdrop through the provider abstraction ([23](23-visual-providers.md)). They are never stored beyond Google's terms, never used to derive anything, and never required.

## R.2 What Google data can and cannot be used for

Google's Map Tiles API [policies](https://developers.google.com/maps/documentation/tile/policies) (read 2026-10-14; the binding text is the Google Maps Platform agreement, which needs a proper read before any release) say, in summary:

| Allowed | Not allowed |
|---|---|
| Streaming and **displaying** Photorealistic 3D Tiles in our own 3D renderer (Cesium for Unity is a named supported renderer) | Pre-fetching, storing, or caching beyond the narrow conditions in the terms; **offline use** |
| Placing our own 3D objects over the tiles | Objects "extracted, traced, or otherwise derived by hand or machine" from the tiles: **no deriving footprints, heights, meshes, or navigation data** |
| Mixing with other data, with clear attribution of which parts are Google's | Image analysis, machine interpretation, or object detection on the tiles |
| | Hiding the Google logo or the aggregated attribution |

Other properties that matter:
- The tiles are a static, monolithic mesh with baked lighting: buildings are not separate objects, capture-time cars and people are baked in, and there are no interiors.
- Their content changes as Google updates captures, and versions cannot be pinned.
- They need a network connection and are metered: an Enterprise SKU with **1,000 free billable events per month**, then about $6 per 1,000 ([pricing](https://developers.google.com/maps/billing-and-pricing/pricing)). The quota counts root tileset requests, and a session token covers up to three hours of tile requests ([usage and billing](https://developers.google.com/maps/documentation/tile/usage-and-billing)). A billing account is required even for free usage.

## R.3 Open data for the simulation (and the default visuals)

| Need | Bay Ridge / NYC (MVP) | Elsewhere (later stages) | Licence notes |
|---|---|---|---|
| Building massing | **NYC 3-D Building Model** ([Data.gov](https://catalog.data.gov/dataset/3-d-building-model)) | Overture / OSM buildings | NYC Open Data terms (confirm attribution); Overture per-theme licences (ODbL or CDLA-Permissive) |
| Building footprints and heights | NYC building footprints | Overture / OSM | As above |
| Terrain | **NYC 2017 topobathymetric LiDAR** ([Data.gov](https://catalog.data.gov/dataset/topobathymetric-lidar-data-2017)) | Copernicus DEM GLO-30 | Free with attribution; vertical datum conversion required |
| Aerial imagery | NYC orthoimagery | Sentinel-2 (10 m), NAIP (US) | Public / free licences |
| Streets, sidewalks, entrances, points of interest | OpenStreetMap | OSM / Overture | ODbL (attribution on produced works; share-alike on derived databases) |
| Street trees | NYC street tree census | OSM where tagged | NYC Open Data |
| Population and demographics | US Census and American Community Survey (block and tract) | GHSL / WorldPop plus national statistics | Public domain (US Census) / open licences |
| Land cover, climate normals | ESA WorldCover, public climate normals | Same | CC BY and similar |
| Earth from orbit; planets | NASA Blue Marble and Black Marble; NASA planetary imagery | Same | Generally public domain |
| Ephemerides | NASA/JPL | Same | Public domain |

Every source is recorded in a data-source register (`docs/data/`), with licence, attribution text, and version, before it is used ([17](17-repository-structure.md)). Licensing (especially ODbL obligations) gets a review before public release.

## R.4 Real skeleton, synthetic life

- **Real:** geography, terrain, streets, building footprints and massing, land use, points of interest, population statistics, the real date and time, the sun position, and Earth's rotation and orbit.
- **Synthetic (procedurally generated, consistent with the real data):** every person (calibrated to Census statistics; never real individuals), households, jobs, schedules, relationships, interiors, building use where data is silent, facades, and missing details.
- **Pinned dataset versions:** each save records its dataset version, so a save keeps seeing the same streets even after the data is updated ([14](14-persistence.md)).
- **Ethics and content:** players can intervene, including destructively, in real places. Residents are always synthetic. Disasters in real neighbourhoods need tasteful presentation, and App Store review policies should be checked early.

## R.5 How realism is achieved

| Layer | Source of realism |
|---|---|
| Near field (street to neighbourhood) | Real massing and footprints, LiDAR terrain, orthoimagery on the ground, realistic PBR facades matched to Bay Ridge building types (brick rowhouses, limestone and brownstone, apartment blocks along 4th and 5th Avenues), the real sun position, weather, and realistic characters near the camera |
| Mid and far field | Open-data visual tiles by default; optional Google photoreal backdrop beyond about 1 km (Manhattan skyline, the harbour, the Verrazzano Bridge) |
| Orbit and space | NASA imagery, atmosphere, real ephemerides |

Near the camera, and anywhere the simulation has changed the world, **simulation geometry is always our own**, because interventions need individual, editable, dynamically lit buildings. Exclusion zones hide any backdrop provider's version of those areas ([23](23-visual-providers.md)).

**Honest limits on iPhone.** Realism at a distance is cheap because it is in the data. At street level it has to come from our own assets and renderer, which on a phone means good PBR lighting, a few dozen realistic animated characters near the camera, and simpler LODs beyond. A street of thousands of AAA-quality humans is not achievable on iPhone; crowd LOD applies ([06](06-rendering-lod.md)).

**The seam risk.** Where the optional Google backdrop meets our geometry, ours will look less photographic, and small misalignments between open data and Google's mesh can show. Keeping the Google backdrop to beyond about 1 km keeps the seam far from the camera, where detail is low. The default open-data provider has no such seam, because its visual tiles come from the same data as the simulation.

## R.6 Coordinates and time for real Earth

Summarised from [07](07-coordinates-and-time.md): ECEF on the WGS84 ellipsoid as Earth's body-fixed frame; S2-style cells with ENU tangent frames on the ellipsoid; ellipsoidal heights, with geoid (EGM2008) and NAVD88 conversions in the pipeline; a real UTC calendar and time zones; real ephemerides for the Sun, Moon, and planets.

## R.7 Cost: what "free" means for each part (ADR 0007)

| Item | Cost |
|---|---|
| Rust, crates, GitHub repository and free Actions minutes | Free |
| Unity Personal | Free below Unity's revenue threshold (re-check terms before release) |
| Cesium for Unity / cesium-native | Free (Apache 2.0) |
| Open data (NYC, OSM/Overture, Copernicus, Census, NASA) | Free (attribution required) |
| Google Photorealistic 3D Tiles | Free up to 1,000 billable events per month; optional, capped below that, with automatic fallback |
| Data hosting | MVP-area tiles ship inside the app; free-tier static hosting for later stages until scale requires a new decision |
| Apple Developer Program | **$99 per year**: the one known cost, needed for TestFlight and the App Store. Free 7-day personal provisioning works for Noah's own device. |

## R.8 Stage notes

- **Technical Prototype:** a pipeline spike for the Bay Ridge area (download, reproject, convert heights, produce tiles, measure size); a Cesium spike; a Google provider spike with quota cap and fallback; a licence register started.
- **Vertical Slice:** about one Bay Ridge block, with 3–5 real buildings from NYC data.
- **Neighbourhood MVP:** 6–12 Bay Ridge blocks with full open-data realism; the Google backdrop optional.
- **City:** citywide NYC data; free-tier hosting.
- **Planet:** a global pipeline staged by region; NASA imagery from orbit.
- **Solar System:** JPL ephemerides; NASA planetary imagery.

## R.9 Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Google terms or pricing change | Backdrop unavailable or costly | The provider is optional; open-data default; isolated in the provider layer |
| Open-data gaps or misalignment | Wrong building shapes; NPCs walking through walls | Reconcile sources in the pipeline; procedural fill flagged as "estimated"; MVP area chosen for data quality |
| Data scale beyond NYC | Pipeline and hosting become real engineering | Stage by region; reuse Overture releases; review hosting costs before the Planet stage |
| Licence obligations (ODbL, NYC, Google attribution) | Compliance problems at release | Data-source register; attribution manager; legal review before release |
| Real places plus destructive interventions | Reputational and App Store review risk | Synthetic residents; tasteful presentation; early policy review |
| Realism expectations on iPhone | Disappointment at street level | Clear art targets per stage; visual polish not required before the Neighbourhood MVP |
