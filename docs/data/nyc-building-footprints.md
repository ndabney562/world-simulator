# NYC Building Footprints

| Field | Value |
|---|---|
| Publisher | NYC Office of Technology and Innovation (OTI) |
| Dataset | "BUILDING", NYC Open Data ID `5zhs-2jue`, <https://data.cityofnewyork.us/resource/5zhs-2jue.json> |
| Documentation | <https://github.com/CityOfNewYork/nyc-geo-metadata/blob/master/Metadata/Metadata_BuildingFootprints.md> |
| Licence | NYC Open Data. Published under the NYC Open Data Law (Local Law 11 of 2012, NYC Administrative Code §23-502), which makes the data available free of charge without registration, licence requirements, or restrictions on use, subject to the [NYC Open Data Terms of Use](https://opendata.cityofnewyork.us/overview/#termsofuse) (no warranty; the City is not liable for use). Free for this project under [ADR 0007](../adr/0007-zero-cost-policy.md) |
| Attribution (credits text) | "Building footprints: NYC Office of Technology and Innovation, NYC Open Data." Not legally required; given anyway |
| Version | No release numbers; updated weekly. Snapshot retrieved 2026-10-18; dataset last updated 2026-10-11 08:17 UTC. Each row's `last_edited_date` is kept in the extract |
| Account or key | None. Anonymous SODA API requests |

## Fields used

| Field | Meaning | Units |
|---|---|---|
| `the_geom` | Footprint outline, MultiPolygon | WGS84 longitude, latitude |
| `bin` | Building Identification Number | |
| `base_bbl` | Borough-block-lot of the tax lot the building stands on | |
| `height_roof` | Roof height above the building's ground elevation | feet |
| `ground_elevation` | Lowest ground elevation at the building | feet, NAVD88 |
| `construction_year`, `feature_code`, `geom_source`, `last_edited_date` | Year built, structure type (2100 building, 5110 garage), capture method, last edit | |

## Accuracy

Photogrammetric footprints are within ±2 ft (95 %); "Other (Manual)" footprints are digitized from imagery and less accurate. A zero or missing `height_roof` means unknown. Buildings under 400 sq ft or 12 ft tall may be missing. See the documentation link above.

## Used by

- [Bay Ridge block 6049 extract](../../data/vertical-slice/bay-ridge-block-6049/README.md) (VS-001, [ADR 0023](../adr/0023-vertical-slice-block.md))
