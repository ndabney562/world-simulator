# NYC PLUTO (Primary Land Use Tax Lot Output)

| Field | Value |
|---|---|
| Publisher | NYC Department of City Planning (DCP) |
| Dataset | NYC Open Data ID `64uk-42ks`, <https://data.cityofnewyork.us/resource/64uk-42ks.json>. Release history: <https://www.nyc.gov/content/planning/pages/resources/datasets/mappluto-pluto-change> |
| Licence | NYC Open Data, as for [NYC Building Footprints](nyc-building-footprints.md): free, no registration, no restrictions on use, no warranty. DCP asks users not to imply its endorsement |
| Attribution (credits text) | "Tax lot data: PLUTO, NYC Department of City Planning, NYC Open Data." |
| Version | **26v2** (the `version` column of every row). Retrieved 2026-10-18; dataset last updated 2026-08-24. Quarterly releases |
| Account or key | None. Anonymous SODA API requests |

## Fields used

Only the columns listed in `PLUTO_FIELDS` in [`scripts/vs_block_extract.py`](../../scripts/vs_block_extract.py) are fetched: lot identity and address, building class and land use, floors, residential and total units, floor areas (sq ft), lot and building frontage and depth (ft), year built, zoning district, and the lot's representative point (WGS84). Owner names and assessed values are not fetched.

Building class codes follow the NYC Department of Finance list (for example A = one-family, B = two-family, C = walk-up apartments, S = mixed residential and store, K = store building, O = office).

## Accuracy

PLUTO describes tax lots, not individual buildings: a lot's floors and units are lot totals, and `numfloors` is the tallest building's floors. Areas come from the Department of Finance and are sometimes estimated.

## Used by

- [Bay Ridge block 6049 extract](../../data/vertical-slice/bay-ridge-block-6049/README.md) (VS-001, [ADR 0023](../adr/0023-vertical-slice-block.md))
