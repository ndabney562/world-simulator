# ADR 0023: Vertical Slice block is Brooklyn tax block 6049 (S-8)

- Status: Proposed
- Date: 2026-10-18
- Decided by: proposed by Claude for VS-001; the Product Owner (Noah) confirms the block
- Issue: #65 (VS-001)
- Supersedes: none

## Context
The Vertical Slice runs every architectural spine on one real Bay Ridge block with 10 to 30 synthetic people ([20-mvp.md](../architecture/20-mvp.md) §21.3). Decision S-8 ([21-decisions-now-vs-later.md](../architecture/21-decisions-now-vs-later.md)) asks which block, chosen on data quality. Every later Vertical Slice issue builds on the block's footprints, so it is fixed first. Sources must be free and open ([ADR 0007](0007-zero-cost-policy.md)); Google data is excluded ([ADR 0008](0008-visual-backdrop-providers.md)).

## Options considered
All 28 Bay Ridge blocks between 3rd and 4th Avenues (ZIP 11209 and 11220, tax blocks 5854 to 6137) were scored from NYC Building Footprints and PLUTO 26v2: does every tax lot have a footprint, does every footprint have a roof height, how many footprints are photogrammetric rather than manual, how many are sheds or garages, and how varied are the building classes.

1. **Block 6049, 87th to 88th Street**: 51 lots, 58 footprints (52 buildings, 6 garages), every lot has a footprint, every footprint has a height, all photogrammetric. Mixed: 9 one-family, 17 two-family, 13 walk-ups, 6 store-plus-apartment, 4 store, 2 office lots; shops on 3rd Avenue, houses on the side streets. Quiet residential streets, 149 homes.
2. Block 6044, 86th to 87th Street: also complete, but half its lots face 86th Street, Bay Ridge's main shopping street; a store-dominated block is a poorer test of homes, households and schedules.
3. Block 5908, 72nd to 73rd Street: complete, but 44 of 60 lots are the same two-family class, so it has little variety.
4. Block 5940, 76th Street to Bay Ridge Parkway: complete, but few one-family houses and two office lots.
5. Others: one or more lots without a footprint, footprints without heights, more manual footprints, or many sheds.

## Decision
The Vertical Slice block is **Brooklyn tax block 6049**: 87th Street to 88th Street between 3rd Avenue and 4th Avenue, Bay Ridge, ZIP 11209. Footprint extent: longitude −74.031468 to −74.028853, latitude 40.621533 to 40.622626 (WGS84), about 220 m by 120 m.

The slice models these four representative buildings in detail (centroids of their footprints, WGS84):

| Role | Address | BIN | Lot | Class | Homes | Floors | Roof height | Centroid (lat, lon) |
|---|---|---|---|---|---|---|---|---|
| Rowhouse | 314 87th Street | 3153708 | 10 | A5, one-family attached | 1 | 2 | 8.07 m | 40.622455, −74.030919 |
| Two-family house | 334 87th Street | 3153715 | 18 | B2, two-family frame | 2 | 2 | 8.29 m | 40.622311, −74.030366 |
| Small apartment building | 351 88th Street | 3153734 | 54 | C2, walk-up, 6 families | 6 | 3 | 12.01 m | 40.621830, −74.029959 |
| Shop with apartment above | 8703 3rd Avenue | 3153706 | 8 | S1, store plus one family | 1 | 2 | 7.33 m | 40.622523, −74.031186 |

That is 10 homes, enough for the slice's 10 to 30 residents. The block's corner lots on 3rd Avenue are a bank (87th Street) and an office (88th Street), so the shop is one lot in from the 87th Street corner. The rest of the block's footprints are in the extract for context and rendering; whether they get residents is VS-003's call.

The extract, its sources and the steps that regenerate it are in [`data/vertical-slice/bay-ridge-block-6049/`](../../data/vertical-slice/bay-ridge-block-6049/README.md); the sources are registered in [`docs/data/`](../data/README.md).

## Consequences
- Easier: VS-002 onwards build on fixed, reviewed footprints, heights and uses; the pinned raw snapshot means a city edit cannot change the slice silently.
- Harder: PLUTO data is per lot, not per building, so floors and homes on lots with a garage belong to the main building by convention. Heights are roof heights above ground (feet in the source, metres in the extract); roof shapes are not modelled.
- Not decided here: terrain (ground elevation in the footprints varies by about 2 m across the block, 21.6 m to 23.5 m NAVD88), streets and sidewalks beyond the block, the offline data pipeline, any 3D building model, and how many residents each building gets.
- Revisit if: the Product Owner prefers another block, or the city's data for this block changes in a way that breaks the slice.
