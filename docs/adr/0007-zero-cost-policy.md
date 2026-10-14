# ADR 0007: Zero-cost policy

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah ("I want it to be free")

## Decision
The project must not require paid services to build, run, or play.
- The default visual path uses **only free, open data** (NYC Open Data, OSM/Overture, Copernicus, NASA). It works offline, and the MVP's data ships inside the app.
- Google Photorealistic 3D Tiles are **optional**, used only as a distant backdrop, with a hard monthly usage cap below Google's free allowance (1,000 billable events per month at the time of writing) and automatic fallback to the open-data path.
- Tools and libraries must be free: Rust, Unity Personal, Cesium for Unity (Apache 2.0), and GitHub with free Actions minutes.

## Known exception
Distributing through TestFlight or the App Store requires an Apple Developer Program membership ($99 per year). Without it, development builds can still be installed on Noah's own device with a free Apple ID, but they expire after 7 days.

## Consequences
- Planet-wide coverage in later phases must come from open data plus free hosting. When player numbers grow, hosting costs need a new decision (a superseding ADR).
- Google's pricing and terms must be re-checked before any public release.
