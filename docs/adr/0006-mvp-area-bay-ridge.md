# ADR 0006: MVP area is Bay Ridge, Brooklyn

- Status: Accepted (staging refined by ADR 0009: the Vertical Slice uses about one Bay Ridge block; the 6–12-block area is the Neighbourhood MVP)
- Date: 2026-10-14
- Decided by: Noah

## Decision
The first playable MVP simulates a small part of Bay Ridge, Brooklyn, New York: about 6–12 blocks, with exact bounds to be agreed. A proposed starting point is around 3rd–5th Avenue and 75th–86th Street.

## Consequences
- The data pipeline in the Technical Prototype targets this area: the NYC 3-D Building Model, NYC building footprints, 2017 NYC LiDAR, NYC orthoimagery, OpenStreetMap, and US Census/ACS tables for population calibration.
- Residents are synthetic, calibrated to census statistics; no real individuals are modelled.
- The area's density means thousands of residents, so the LOD system is exercised from the start.
