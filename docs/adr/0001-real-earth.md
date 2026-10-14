# ADR 0001: The world is real Earth

- Status: Accepted
- Date: 2026-10-14
- Decided by: Noah

## Context
The original proposal assumed a fictional, procedurally generated planet (assumption A5). Noah chose real Earth, using Google Earth / Google Maps data where useful.

## Decision
The world is real Earth, inside the real solar system.
- Google Maps Platform data (Photorealistic 3D Tiles) is used **for display only**, as its terms require: it is streamed, never stored beyond what the terms allow, never used to derive geometry or simulation data, and always attributed.
- Simulation data (roads, buildings, terrain, land use, population density) comes from open datasets: OpenStreetMap/Overture, Copernicus DEM, GHSL/WorldPop, and ESA WorldCover. These are processed into versioned simulation tiles that saves pin.
- People are synthetic, calibrated to real statistics. No real individuals are modelled.

## Consequences
- The coordinate system uses WGS84/ECEF, ellipsoidal heights, real time, and real ephemerides.
- An offline data pipeline and a CDN-hosted tile backend are required.
- Photoreal visuals need a network connection and incur usage costs.
- Licence obligations (ODbL attribution and share-alike, Google attribution) need legal review before release.
- Details: [docs/architecture/22-real-earth-and-realism.md](../architecture/22-real-earth-and-realism.md).
