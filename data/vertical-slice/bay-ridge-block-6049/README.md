# Bay Ridge block 6049 extract (Vertical Slice)

Real data for the Vertical Slice block chosen in [ADR 0023](../../../docs/adr/0023-vertical-slice-block.md): Brooklyn tax block 6049, 87th to 88th Street between 3rd and 4th Avenues. Issue #65 (VS-001).

## Files

| File | Contents |
|---|---|
| `raw/footprints.json` | The block's rows from [NYC Building Footprints](../../../docs/data/nyc-building-footprints.md), as returned by the API (keys sorted). Pinned snapshot, retrieved 2026-10-18 |
| `raw/pluto.json` | The block's rows from [NYC PLUTO](../../../docs/data/nyc-pluto.md) 26v2, selected columns only (no owner names or assessed values). Pinned snapshot, retrieved 2026-10-18 |
| `buildings.geojson` | 58 footprints (52 buildings, 6 garages) as a GeoJSON FeatureCollection, one feature per line, sorted by BIN. Geometry is WGS84 longitude/latitude. Properties below |
| `lots.csv` | The block's 51 tax lots from PLUTO, sorted by lot number. Units in the column names (`_sqft`, `_ft`) |

`buildings.geojson` properties: `bin`, `bbl`, `lot`, `address`, `feature_code` and `feature_type` (2100 building, 5110 garage), `height_roof_ft` and `height_roof_m` (roof above ground), `ground_elevation_ft_navd88` and `ground_elevation_m_navd88`, `construction_year`, `geom_source`, `last_edited_date`, and from the lot: `bldgclass`, `numfloors`, `unitsres`. `centroid_lon` and `centroid_lat` are the footprint's area centroid, rounded to 6 decimals (about 0.1 m). `featured` names the role of the four buildings the slice models in detail (rowhouse, two-family house, small apartment building, shop with apartment above) and is null elsewhere. Lot values (floors, homes) are per lot, so a garage on a lot repeats its main building's lot values.

## Regenerating

From the repository root, with Python 3 and nothing else:

```sh
python3 scripts/vs_block_extract.py build   # raw/ -> buildings.geojson, lots.csv (offline)
python3 scripts/vs_block_extract.py check   # fails if the committed extract is stale
python3 -m unittest discover -s scripts -p 'test_vs_*.py' -v
```

`build` reproduces the committed `buildings.geojson` and `lots.csv` byte for byte from `raw/`; CI checks this. To refresh the snapshot from the live datasets (needs access to `data.cityofnewyork.us`), run `python3 scripts/vs_block_extract.py fetch` and then `build`. The city edits footprints weekly and PLUTO quarterly, so a refresh can change the files; review the diff, and update the retrieval dates here and in `docs/data/`.

## Attribution

Building footprints: NYC Office of Technology and Innovation, NYC Open Data. Tax lot data: PLUTO, NYC Department of City Planning, NYC Open Data. Used under the NYC Open Data terms (free, no restrictions on use, no warranty).
